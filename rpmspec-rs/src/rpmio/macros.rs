/// Welcome to the world of chaos
/// The code was kinda ported from C
/// see gh `rpm-software-management/rpm` -> `rpmio/macro.c`
///
/// == Guide to read docs ==
/// "=>" indicates rust fn behaviours
/// "->" indicates original C fn behaviours
///
/// Without pointers, a lot of functions were subjected to
/// change (some rewritten, some get to take a shower)
use crate::{
	error::{self, ParserError},
	rpmio::error::MacroErr,
	spec::Macro,
	utils::popen,
};
use color_eyre::{eyre::eyre, Report, Result};
use std::{
	collections::{BTreeMap, HashMap},
	fs::File,
	io::{stderr, BufRead, BufReader, Write, self},
	string,
	sync::{Arc, Mutex, MutexGuard}, os::fd::AsRawFd,
};
use tracing::{debug, error, info, warn};

type Func = fn(MacroBuf, Entry, Vec<String>, &usize);
type MacroFunc = Func;
#[derive(Clone, Default)]
struct Entry {
	prev: Option<Box<Self>>, // Macro entry stack
	name: String,            // Macro name
	opts: String,            // Macro parameters
	body: String,            // Macro body
	func: Option<Func>,      // Macro function (builtin macros)
	nargs: u8,               // No. required args
	flags: i16,              // Macro state bits
	level: i16,              // Scoping level
	arena: String,           // String arena
}

#[derive(Default, Clone)]
struct MacroContext {
	// in the original C code, Entry[] was used.
	// we switched to BTreeMap to improve find_entry() perf.
	table: BTreeMap<String, Entry>,
	n: u16,     // No. macros
	depth: u8,  // Depth tracking when recursing from Lua
	level: i16, // Scope level tracking when recursing from Lua
}
impl MacroContext {
	fn new() -> Self {
		Self {
			..Default::default()
		}
	}
}
type Context = Arc<Mutex<MacroContext>>;
#[derive(Clone)]
struct MacroBuf {
	buf: String, // Expansion buffer
	tpos: usize, // Current position in buf
	// nb: usize,			// No. bytes remaining in buf
	depth: u8,         // Current expansion depth
	level: i16,        // Current scoping level
	error: bool,       // Errors encountered during expansion?
	mtrace: bool,      // Pre-print macro to expand?  (macro_trace)
	etrace: bool,      // Post-print macro expansion? (expand_trace)
	flags: i16,        // Flags to control behaviour
	me: Option<Entry>, // Current macro
	args: Vec<String>, // Current macro arguments
	mc: Context,
}
pub(crate) fn _dummy_context() -> Context {
	Arc::from(Mutex::from(MacroContext::new()))
}
impl Default for MacroBuf {
	fn default() -> Self {
		Self {
			mc: Arc::from(Mutex::from(MacroContext::new())),
			buf: String::default(),
			depth: 0,
			level: 0,
			mtrace: PRINT_MACRO_TRACE,
			etrace: PRINT_EXPAND_TRACE,
			flags: 0,
			tpos: 0,
			args: vec![],
			error: false,
			me: None,
			// nb: 0,
		}
	}
}
struct MacroExpansionData {
	tpos: usize,
	mtrace: bool,
	etrace: bool,
}
// ==================== CONST ====================
const MAX_MACRO_DEPTH: u8 = 64;
const PRINT_MACRO_TRACE: bool = false;
const PRINT_EXPAND_TRACE: bool = false;
const ME_NONE: i16 = 0;
const ME_AUTO: i16 = 1 << 0;
const ME_USED: i16 = 1 << 1;
const ME_LITERAL: i16 = 1 << 2;
const ME_PARSE: i16 = 1 << 3;
const ME_FUNC: i16 = 1 << 4;
const RMIL_MACROFILES: i16 = -13;
const RMIL_GLOBAL: i16 = 0;

// ==================== MACROS ====================
macro_rules! mbErr {
	($mb:expr, $error:expr, $fmt:expr, $($ap:tt)*) => {{
		let emsg = format!($fmt, $($ap,)*);
		let pfx = rpm_expand([
			"%{?__file_name:%{__file_name}: }",
			"%{?__file_lineno:line %{__file_lineno}: }",
		]);
		// I have no idea why original C code incl'd NULL in args
		if $error {
			error!("{pfx}{emsg}");
		} else {
			warn!("{pfx}{emsg}");
		}
		$mb.error = $error;
	}};
	($mb:expr, $error:expr, $fmt:expr) => {{
		let emsg = format!($fmt);
		let pfx = rpm_expand([
			"%{?__file_name:%{__file_name}: }",
			"%{?__file_lineno:line %{__file_lineno}: }",
		]);
		if $error {
			error!("{pfx}{emsg}");
		} else {
			warn!("{pfx}{emsg}");
		}
		$mb.error = $error;
	}};
}
macro_rules! copyname {
	($ne:ident, $s:ident, $c:ident) => {
		let _s = $s.trim_start();
		$s = _s
			.trim_start_matches(|_c: char| {
				$c = _c;
				_c.is_ascii_alphanumeric() || _c == '_'
			})
			.as_mut();
		$ne = $ne[_s.len() - $s.len()..].as_mut();
		drop(_s);
	};
}
macro_rules! copyopts {
	($oe:ident, $s:ident, $c:ident) => {
		let _s = $s.trim_start();
		$s = _s
			.trim_start_matches(|_c: char| {
				$c = _c;
				_c != ')'
			})
			.as_mut();
		$oe = &$oe[_s.len() - $s.len()..];
		drop(_s);
	};
}
// ^^^^^^^^^^^^^^^^^^^^ MACROS ^^^^^^^^^^^^^^^^^^^^

impl MacroBuf {
	fn new(mc: MacroContext, flags: i16) -> Self {
		Self {
			depth: mc.depth,
			level: mc.level,
			mc: Arc::from(Mutex::from(mc)),
			flags,
			..Default::default()
		}
	}
	fn init(&mut self, med: &mut MacroExpansionData) -> Result<(), MacroErr> {
		self.depth += 1;
		if self.depth > MAX_MACRO_DEPTH {
			mbErr!(self, true, "Too many levels of recursion in macro expansion. It is likely caused by recursive macro declaration.");
			self.depth -= 1;
			return Err(MacroErr::MacroDepthExceeded);
		}
		med.tpos = self.tpos;
		med.mtrace = self.mtrace;
		med.etrace = self.etrace;
		Ok(())
	}
	fn fini(&mut self, me: Entry, med: MacroExpansionData) -> Result<()> {
		self.buf = self.buf[..=self.tpos].to_string();
		self.depth -= 1;
		// if is verbose (assume yes for now)
		self.etrace = true;
		self.print_expansion(Some(me), &self.buf[med.tpos..], &self.buf[self.tpos..])?;
		self.mtrace = med.mtrace;
		self.etrace = med.etrace;

		Ok(())
	}
	fn append(&mut self, c: char) {
		assert_eq!(self.tpos, self.buf.len());
		self.buf.push(c);
		self.tpos += 1;
	}
	fn appends(&mut self, s: &str) {
		assert_eq!(self.tpos, self.buf.len());
		self.buf.push_str(s);
		self.tpos += s.len();
	}
	fn do_dnl(&self, me: Entry) {
		todo!()
	}
	fn do_shell_escape(&mut self, cmd: &str) {
		let mut buf = String::new();
		if self.expand_this(cmd, &mut buf) {
			return;
		}
		if let Some(stdout) = popen(&buf) {
			self.appends(stdout.trim_end_matches(|c| c == '\n' || c == '\r'));
		} else {
			mbErr!(
				self,
				true,
				"Failed to open shell expansion pipe for command: {buf}"
			);
			// idk what is %m, can't find refs
		}
	}
	fn do_expression_expansion(&self, expr: &str) {
		// let res = rpmExprStrFlags(expr, )
	}
	/// -> Post-print expanded macro expression.
	/// WARN `t` and `te` should take until EOS
	fn print_expansion(&mut self, me: Option<Entry>, t: &str, te: &str) -> Result<()> {
		let mname = me.map_or("".into(), |m| m.name);
		if te.len() <= t.len() {
			let mut stderr = stderr().lock();
			stderr.write_fmt(format_args!(
				"{:>3}>{} (%{})\n",
				self.depth,
				" ".repeat((2 * self.depth + 1).into()),
				mname
			))?;
			return Ok(());
		}
		// -> Shorten output which contains newlines
		let te = te.trim_end_matches('\n');
		// ^ Assume: te > t
		let t = if self.depth > 0 {
			// Assume no trailing \n
			te.lines().last().unwrap_or(te)
		} else {
			t
		};
		let mut stderr = stderr().lock();
		stderr.write_fmt(format_args!(
			"{:>3}>{} (%{})\n",
			self.depth,
			" ".repeat((2 * self.depth + 1).into()),
			mname
		))?;
		if t.len() > te.len() {
			stderr.write_fmt(format_args!("{}", &t[..t.len() - te.len() - 1]))?;
		}
		Ok(())
	}
	fn expand_this(&mut self, src: &str, target: &mut String) -> bool {
		let mut umb = self.clone();
		umb.buf = "".to_string();
		if let Ok(_) = expand_macro(Some(&umb), src) {
			self.error = true;
		}
		*target = umb.buf;
		umb.error
	}
	pub(crate) fn valid_name(&self, name: &str, action: &str) -> bool {
		let rc = 0;
		let c = name.chars().nth(0).unwrap_or('\0');
		if !(c.is_ascii_alphabetic() || (c == '_' && name.len() > 1)) {
			mbErr!(self, true, "Macro %{name} has illegal name ({action})");
			return false;
		}

		let mep = SaiGaai::new().find_entry(self.mc, name);
		if mep.is_ok() {
			let mep = mep.unwrap();
			if mep.flags & (ME_FUNC | ME_AUTO) != 0 {
				mbErr!(self, true, "Macro %{name} is a built-in ({action})");
				return false;
			}
		}
		true
	}
	pub(crate) fn do_define(
		&mut self, se: &mut str, lvl: i16, expandbody: bool, parsed: usize,
	) -> usize {
		let mut start = se;
		let mut s = se;
		let mut buf = String::new();
		let mut n: &mut str = buf.as_mut();
		let mut ne = n;
		let mut o = "";
		let mut oe = "";
		let (mut b, mut be, mut ebody) = ("", "", "");
		let mut c = '\0';
		let mut oc = ')';
		let mut sbody = "";
		let mut rc = true; // -> assume failure
		copyname!(ne, s, c);

		macro_rules! exit {
			() => {
				if rc {
					self.error = true;
				}
				if parsed != 0 {
					parsed += start.len() - se.len();
				}
				return parsed;
			};
		}

		// -> copy opts (if present)
		let oe = &ne[1..];
		if s.starts_with('(') {
			s = s[1..].as_mut(); // -> skip (
			if s.contains(')') {
				o = oe;
				copyopts!(oe, s, oc);
				s = s[1..].as_mut();
			} else {
				mbErr!(self, true, "Macro %{n} has unterminated opts");
				exit!();
			}
		}
		be = &oe[1..];
		b = be;
		sbody = s;
		s = s.trim_start().as_mut();
		if parsed != 0 {
			b = s;
			be = &b[b.len()..];
			s = s[s.len()..].as_mut();
		} else if c == '{' {
			let _se = matchchar(s, '{', '}');
			if _se == 0 {
				mbErr!(self, true, "Macro %{n} has unterminated body");
				se = s;
				exit!();
			}
			s = s[1..].as_mut();
			b = &s[s.len() - se.len() - 1..];
			be = &be[b.len()..];
			s = se;
		} else {
			let (mut bc, mut pc, mut xc) = (0, 0, 0);
			loop {
				if s.trim().is_empty() {
					break;
				}
				macro_rules! sclone {
					($x:ident) => {{
						sclone!();
						$x += 1;
					}};
					() => {
						(be, be, s) = (s, be[1..].as_mut(), s[1..].as_mut())
					};
				}
				match s.chars().nth(0) {
					Some('\\') => match s.chars().nth(1) {
						None => {}
						_ => s = s[1..].as_mut(),
					},
					Some('%') => match s.chars().nth(1).unwrap_or('\0') {
						'{' => sclone!(bc),
						'(' => sclone!(pc),
						'[' => sclone!(xc),
						'%' => sclone!(),
					},
					Some('{') if bc > 0 => bc += 1,
					Some('}') if bc > 0 => bc -= 1,
					Some('(') if pc > 0 => pc += 1,
					Some(')') if pc > 0 => pc -= 1,
					Some('[') if xc > 0 => xc += 1,
					Some(']') if xc > 0 => xc -= 1,
				}
				be = s;
				sclone!();
			}
			// be = \0
			if bc > 0 || pc > 0 || xc > 0 {
				mbErr!(self, true, "Macro %{n} has unterminated body");
				se = s;
				exit!();
			}
			be = be.trim_end();
		}
		s = s.trim_start_matches(['\n', '\r']).as_mut();
		se = s;
		if !self.valid_name(
			n,
			if expandbody {
				"%global"
			} else {
				"%define"
			},
		) {
			exit!();
		}
		if be.len() - b.len() < 1 {
			mbErr!(self, true, "Macro %{n} has empty body");
			exit!();
		}
		if !sbody.starts_with([' ', '\t'])
			&& !(sbody.starts_with('\\')
				&& ['\n', '\r'].contains(&sbody.chars().nth(1).unwrap_or('\0')))
		{
			mbErr!(self, false, "Macro {n} needs whitespace before body");
		}

		let mut ebody = ebody.to_string();

		if expandbody {
			if self.expand_this(b, &mut ebody) {
				mbErr!(self, true, "Macro %{n} failed to expand");
				exit!();
			}
			b = &ebody;
		}
		push_macro(Some(self.mc), n, o, b, lvl, ME_NONE);
		rc = false;
		exit!();
	}
	fn do_undefine(&mut self, me: Entry, n: &str) {
		if !self.valid_name(n, "%undefine") {
			self.error = true;
		} else {
			pop_macro(Some(self.mc), n);
		}
	}
	fn do_argv_define(&mut self, argv: &[&str], lvl: i16, expand: bool, parsed: usize) {
		let mut se = argv[1];
		let x;
		if matches!(argv.get(2), Some(s) if !s.is_empty()) {
			x = format!("{} {}", argv[1], argv[2]);
			se = &x;
		}
		self.do_define(se.as_mut(), lvl, expand, parsed);
	}
	fn do_def(&mut self, me: Entry, argv: &[&str], parsed: usize) {
		self.do_argv_define(argv, self.level, false, parsed);
	}
	fn do_global(&mut self, me: Entry, argv: &[&str], parsed: usize) {
		self.do_argv_define(argv, RMIL_GLOBAL, true, parsed);
	}
	fn do_dump(&mut self, me: Entry, argv: &[&str], parsed: usize) {
		dumpMacroTable(self.mc, None);
	}
}

pub(crate) struct SaiGaai {
	global_mctx: Context,
	cli_mctx: Context,
	line: usize,
}
impl SaiGaai {
	fn new() -> Self {
		// initLocks()
		Self {
			global_mctx: Context::new(Mutex::new(MacroContext::new())),
			cli_mctx: Context::new(Mutex::new(MacroContext::new())),
			line: 0,
		}
	}
	fn find_entry(&self, mc: Context, name: &str) -> Result<Entry> {
		// original code use binary search
		let ctx = get_ctx(&mc)?;
		Ok(ctx
			.table
			.get(name)
			.ok_or(ParserError::UnknownMacro(self.line, name.to_string()))?
			.clone())
	}
	fn new_entry(&self, mc: Context, key: String, value: Entry) -> Result<()> {
		// no need extend macro table
		// instead get it out of the mutex
		let mut ctx = get_ctx(&mc)?;
		ctx.n += 1;
		if let Some(x) = ctx.table.insert(key, value) {
			// For debugging. Actually it's normal,
			// but dunno why happens itfp.
			debug!("Macro duplicated: {}", x.name);
		}
		Ok(())
	}
}
pub(crate) fn dumpMacroTable(mc: Context, fp: Option<File>) -> io::Result<()> {
	let mc = mc.lock().unwrap();
	let fp = fp.unwrap(); // FIXME should default to stderr
	fp.write(b"========================")?;
	for (name, me) in mc.table {
		let mut s = String::new();
		if !me.opts.is_empty() {
			s += &format!("({})", me.opts);
		}
		if !me.body.is_empty() {
			s += &format!("\t{}", me.body);
		}
		fp.write_fmt(format_args!("{:.3}{} {}{s}", me.level, if me.flags & ME_USED == 0 { ':'} else {'='}, me.name))?;
	}
	fp.write_fmt(format_args!("======================== active {} empty 0", mc.n))?;
	Ok(())

}
pub(crate) fn expand_macro(buf: Option<&MacroBuf>, src: &str) -> Result<()> {
	todo!()
}
// -> rpmExpandMacros(mc, sbuf, obuf, flags)
// => expand_macros(mc, sbuf, flags) -> obuf
pub(crate) fn expand_macros(mc: Context, sbuf: &str, flags: i32) -> Result<String> {
	todo!()
}
fn get_ctx(mc: &Context) -> Result<MutexGuard<MacroContext>> {
	Ok(mc.lock().expect("Can't lock mc"))
}

/// -> fgets(3) analogue that reads \ continuations. Last newline always trimmed.
///
/// in this case, we probably prefer a bufread to throw newlines to us.
/// then we trim and check for \, but also {[( stuff like\n these )]}
/// we don't need the size parameter *I think*...
/// I mean it says it's the *inbut* (yes, inbut) buffer size (bytes).
fn rdcl(mut f: impl BufRead) -> Result<String> {
	let mut buf = String::new();
	let mut bc: u16 = 0; // { }
	let mut pc: u16 = 0; // ( )
	let mut xc: u16 = 9; // [ ]
	loop {
		let mut curbuf = String::new();
		if f.read_line(&mut curbuf)? == 0 {
			break;
		}
		let mut last = '\0';
		let mut esc = false;
		for ch in curbuf.trim_end().chars() {
			if ch == '\\' {
				esc = true;
				continue;
			}
			esc = false;
			if last == '%' && ch == '%' {
				last = '%';
				continue;
			}
			match ch {
				'{' => bc += 1,
				'(' => pc += 1,
				'[' => xc += 1,
				'}' => bc -= 1,
				')' => pc -= 1,
				']' => xc -= 1,
				_ => {}
			}
			last = ch;
		}
		buf += &curbuf;
		if esc {
			continue;
		}
		if bc + pc + xc == 0 {
			break;
		}
	}
	Ok(buf.trim_end().to_string())
}

/// => Return length of text between `pl` and `pr` inclusive.
///
/// -> Return text between `pl` and matching `pr` characters.
///
/// Nyu reinvented the wheel.
/// NOTE: expect `pl` to be first char
fn matchchar(text: &str, pl: char, pr: char) -> usize {
	let mut lvl = 0;
	let mut skip = false;
	for (i, c) in text.chars().enumerate() {
		if skip {
			skip = false;
			continue;
		}
		if c == '\\' {
			skip = true;
			continue;
		}
		if c == pr {
			// why rust nu ++ and -- ???
			lvl -= 1;
			if lvl <= 0 {
				return i + 1;
			}
		} else if c == pl {
			lvl += 1;
		}
	}
	0
}

/// -> Pre-print macro expression to be expanded.
///
/// we use &str instead of ptr
///
/// WARN `t` and `te` should take until EOS
fn print_macro(mb: MacroBuf, mut s: &str, se: &str) -> Result<()> {
	if se.len() >= s.len() {
		let mut stderr = stderr().lock();
		stderr.write_fmt(format_args!(
			"{:>3}>{}(empty)\n",
			mb.depth,
			" ".repeat((2 * mb.depth + 1).into())
		))?;
		return Ok(());
	}

	// it has a s-- check for '{', we don't. skip!

	// -> Print only to first EOF/EOS
	let senl: &str = match se.split_once('\n') {
		Some((a, b)) => a,
		None => se,
	};

	// -> Sub. caret (^) at EO-macro pos.
	let mut stderr = stderr().lock();
	let x = s.to_string();
	stderr.write_fmt(format_args!(
		"{:>3}>{}%{}^",
		mb.depth,
		" ".repeat((2 * mb.depth + 1).into()),
		&x[0..(se.len() - s.len()) - 1]
	))?;
	if se.len() > 1 && (senl.len() - (se.len() + 1)) > 0 {
		// from se+1, with len senl - (se+1)
		stderr.write_fmt(format_args!("{}", &se[1..(senl.len() - (se.len() + 1))]))?;
	}
	stderr.write_all(b"\n")?;
	Ok(())
}
// They kinda have a lot of malloc and stuff
fn rpm_expand<'a>(args: impl AsRef<[&'a str]>) -> String {
	let pe = args.as_ref().join("");
	let mc = MacroContext::new();
	todo!()
}
fn do_expand_macros(mc: MacroContext, src: String, flags: u32) -> Result<(String, u16)> {
	todo!()
}

/// -> Find end of macro call
/// => Find length between
pub(crate) fn find_macro_end(s: &str) -> usize {
	if s.starts_with('(') {
		matchchar(s, '(', ')')
	} else if s.starts_with('{') {
		matchchar(s, '{', '}')
	} else if s.starts_with('[') {
		matchchar(s, '[', ']')
	} else {
		let mut ss = s.trim_start_matches(|p| p == '?' || p == '!');
		if ss.starts_with('-') {
			ss = &ss[1..];
		}
		ss = ss.trim_start_matches(|p: char| p.is_ascii_alphanumeric() || p == '_');
		if ss.starts_with("**") {
			ss = &ss[2..];
		} else if ss.starts_with(|p| p == '*' || p == '#') {
			ss = &ss[1..];
		}
		s.len() - ss.len()
	}
}
pub(crate) fn define_macro(mc: Option<Context>, name: &str, lvl: i16) -> Result<bool> {
	let mc = mc.unwrap_or(_dummy_context());
	let mc = mc.lock().map_err(|e| eyre!(e.to_string()))?;
	let mb = MacroBuf::new(mc.clone(), 0);
	mb.do_define(name.as_mut(), lvl, false, 0);
	Ok(mb.error)
}

pub(crate) fn pop_macro(mc: Option<Context>, name: &str) -> Result<()> {
	let mc = mc.unwrap_or(_dummy_context());
	let mc = mc.lock().map_err(|e| eyre!(e.to_string()))?;
	mc.table.remove(name);

	Ok(())
}

pub(crate) fn macro_is_defined(mc: Option<Context>, name: &str) -> Result<bool> {
	let mc = mc.unwrap_or(_dummy_context());
	let ctx = mc.lock().map_err(|e| eyre!(e.to_string()))?;
	Ok(SaiGaai::new().find_entry(mc, name).is_ok())
}

pub(crate) fn macro_is_parametric(mc: Option<Context>, name: &str) -> Result<bool> {
	let mc = mc.unwrap_or(_dummy_context());
	let ctx = mc.lock().map_err(|e| eyre!(e.to_string()))?;
	let en = SaiGaai::new().find_entry(mc, name);
	if let Ok(en) = en {
		if !en.opts.is_empty() {
			return Ok(true);
		}
	}
	Ok(false)
}

pub(crate) fn load_macro_file(mc: Option<Context>, name: &str) -> Result<i32> {
	let mc_lock = mc.unwrap_or(_dummy_context());
	let ctx = mc_lock.lock().map_err(|e| eyre!(e.to_string()))?;
	let fd = File::open(name);
	if fd.is_err() {
		return Ok(-1);
	}
	let fd = fd.unwrap();
	push_macro(mc, "__file_name", "", name, RMIL_MACROFILES, ME_LITERAL);

	while let Ok(buffer) = rdcl(fd.try_clone()?) {
		let nlines = buffer.lines().count();
		let lineno = 0;

		let mut chars = buffer.chars();
		let c = chars.skip_while(|c| c.is_whitespace()).next().unwrap();
		if c != '%' {
			continue;
		}

		// skip the % character
		chars.next();
	}

	// while ((nlines = rdcl(buf, blen, fd)) > 0) {

	todo!();
}

pub(crate) fn push_macro_any(
	mc: Option<Context>, n: &str, o: &str, b: &str, f: Option<MacroFunc>, nargs: u8, lvl: i16,
	flags: i16,
) {
	let mut me = Entry::default();
	let olen = o.len();
	let blen = b.len();
	let mut p: &str;
	let mc = mc.unwrap_or(_dummy_context());
	let mep = SaiGaai::new().find_entry(mc, n);
	if let Ok(en) = mep {
		// -> entry with shared name
		p = &me.arena;
		me.name = en.name; // -> set name
	} else {
		// -> entry with new name
		let mep = Entry::default();
		p = &me.arena;
		me.name = p.to_string(); // -> copy name
		p = n;
	}
	// -> copy body
	me.body = p.to_string(); // -> copy body
	if blen != 0 {
		p = b;
	} else {
		// !!
		// *p = '\0';
	}
	p = &p[blen + 1..];
	if olen != 0 {
		p = o;
		me.opts = p.to_string();
	} else {
		// me->opts = o ? "" : NULL;
		me.opts = String::new();
	}
	// -> initialize
	me.func = f;
	me.nargs = nargs;
	me.flags = flags;
	me.flags &= !ME_USED;
	me.level = lvl;
	me.prev = mep.ok().map(|a| Box::new(a));
}

#[inline]
pub(crate) fn push_macro(mc: Option<Context>, n: &str, o: &str, b: &str, lvl: i16, flags: i16) {
	push_macro_any(mc, n, o, b, None, 0, lvl, flags);
}

// todo move to rpmlog
enum RPMLogLvl {
	EMERG,
	ALERT,
	CRIT,
	ERR,
	WARN,
	NOTE,
	INFO,
	DEBUG,
}
