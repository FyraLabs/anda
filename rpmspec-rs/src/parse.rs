use crate::error::ParserError;
use color_eyre::{eyre::bail, eyre::eyre, Help, Result, SectionExt};
use lazy_static::lazy_static;
use regex::Regex;
use smartstring::alias::String;
use std::{
	collections::HashMap,
	fmt::Write,
	fs::File,
	io::{BufRead, BufReader, Read},
	mem::take,
	process::Command,
};
use tracing::{debug, error, warn};

#[derive(Default, Clone, Copy)]
pub enum PkgQCond {
	#[default]
	Eq, // =
	Le, // <=
	Lt, // <
	Ge, // >=
	Gt, // >
}

impl From<&str> for PkgQCond {
	fn from(value: &str) -> Self {
		match value {
			"=" => PkgQCond::Eq,
			">=" => PkgQCond::Ge,
			">" => PkgQCond::Gt,
			"<=" => PkgQCond::Le,
			"<" => PkgQCond::Lt,
			_ => unreachable!("Regex RE_PKGQCOND matched bad condition `{value}`"),
		}
	}
}

#[derive(Clone, Default)]
pub struct Package {
	pub name: String,
	pub version: Option<String>,
	pub release: Option<String>,
	pub epoch: Option<u32>,
	pub condition: PkgQCond,
}
lazy_static! {
	static ref RE_PKGQCOND: Regex = Regex::new(r"\s+(>=?|<=?|=)\s+(\d+:)?([\w\d.^~]+)-([\w\d.^~]+)(.*)").unwrap();
}

const PKGNAMECHARSET: &str = "_-";

impl Package {
	pub fn new(name: String) -> Self {
		Self { name, ..Self::default() }
	}
	// Simple query: query without the <= and >= and versions and stuff. Only names.
	pub fn add_simple_query(pkgs: &mut Vec<Self>, query: &str) -> Result<()> {
		let query = query.trim();
		let mut last = String::new();
		for ch in query.chars() {
			if ch == ' ' || ch == ',' {
				pkgs.push(Package::new(std::mem::take(&mut last)));
				continue;
			}
			if ch.is_alphanumeric() || PKGNAMECHARSET.contains(ch) {
				return Err(eyre!("Invalid character `{ch}` found in package query.").note(format!("query: `{query}`")));
			}
			last.write_char(ch)?;
		}
		if !last.is_empty() {
			pkgs.push(Package::new(last));
		}
		Ok(())
	}
	pub fn add_query(pkgs: &mut Vec<Self>, query: &str) -> Result<()> {
		let query = query.trim(); // just in case
		if let Some((name, rest)) = query.split_once(|c: char| !c.is_alphanumeric() && !PKGNAMECHARSET.contains(c)) {
			// the part that matches the good name is `name`. Check the rest.
			let mut pkg = Package::new(name.into());
			if let Some(caps) = RE_PKGQCOND.captures(rest) {
				pkg.condition = caps[1].into();
				if let Some(epoch) = caps.get(2) {
					let epoch = epoch.as_str().strip_suffix(':').expect("epoch no `:` by RE_PKGQCOND");
					pkg.epoch = Some(epoch.parse().map_err(|e| eyre!("Cannot parse epoch to u32: `{epoch}`").error(e).suggestion("Epoch can only be positive integers"))?);
				}
				pkg.version = Some(caps[3].into());
				pkg.release = Some(caps[4].into());
				pkgs.push(pkg);
				if let Some(rest) = caps.get(5) {
					return Self::add_query(pkgs, rest.as_str().trim_start_matches(|c| " ,".contains(c)));
				}
				Ok(())
			} else {
				Self::add_query(pkgs, rest.trim_start_matches(|c| " ,".contains(c)))
			}
		} else {
			// check if query matches pkg name
			if query.chars().any(|c| !c.is_alphanumeric() && !PKGNAMECHARSET.contains(c)) {
				return Err(eyre!("Invalid package name `{query}`").suggestion("Use only alphanumerics, underscores and dashes."));
			}
			pkgs.push(Self::new(query.into()));
			Ok(())
		}
	}
}

#[derive(Default)]
pub struct RPMRequires {
	pub none: Vec<Package>,
	pub pre: Vec<Package>,
	pub post: Vec<Package>,
	pub preun: Vec<Package>,
	pub postun: Vec<Package>,
	pub pretrans: Vec<Package>,
	pub posttrans: Vec<Package>,
	pub verify: Vec<Package>,
	pub interp: Vec<Package>,
	pub meta: Vec<Package>,
}

#[derive(Default)]
pub struct Scriptlets {
	pub pre: Option<String>,
	pub post: Option<String>,
	pub preun: Option<String>,
	pub postun: Option<String>,
	pub pretrans: Option<String>,
	pub posttrans: Option<String>,
	pub verify: Option<String>,

	pub triggerprein: Option<String>,
	pub triggerin: Option<String>,
	pub triggerun: Option<String>,
	pub triggerpostun: Option<String>,

	pub filetriggerin: Option<String>,
	pub filetriggerun: Option<String>,
	pub filetriggerpostun: Option<String>,
	pub transfiletriggerin: Option<String>,
	pub transfiletriggerun: Option<String>,
	pub transfiletriggerpostun: Option<String>,
}

pub enum ConfigFileMod {
	None,
	MissingOK,
	NoReplace,
}

pub enum VerifyFileMod {
	FileDigest, // or 'md5'
	Size,
	Link,
	User, // or 'owner'
	Group,
	Mtime,
	Mode,
	Rdev,
	Caps,
}

#[derive(Default)]
pub struct Files {
	// %artifact
	pub artifact: Vec<String>,
	// %ghost
	pub ghost: Vec<String>,
	// %config
	pub config: HashMap<String, ConfigFileMod>,
	// %dir
	pub dir: Vec<String>,
	// %readme (obsolete) = %doc
	// %doc
	pub doc: Vec<String>,
	// %license
	pub license: Vec<String>,
	// %verify
	pub verify: HashMap<String, VerifyFileMod>,
}

pub struct Changelog {
	pub date: String, // ! any other?
	pub version: Option<String>,
	pub maintainer: String,
	pub email: String,
	pub message: String,
}

#[derive(Default)]
pub struct RPMSpec {
	pub globals: HashMap<String, String>,
	pub defines: HashMap<String, String>,

	// %description
	pub description: Option<String>,
	// %prep
	pub prep: Option<String>,
	// %generate_buildrequires
	pub generate_buildrequires: Option<String>,
	// %conf
	pub conf: Option<String>,
	// %build
	pub build: Option<String>,
	// %install
	pub install: Option<String>,
	// %check
	pub check: Option<String>,

	pub scriptlets: Scriptlets,
	pub files: Files,              // %files
	pub changelog: Vec<Changelog>, // %changelog

	//* preamble
	pub name: Option<String>,
	pub version: Option<String>,
	pub release: Option<String>,
	pub epoch: Option<i32>,
	pub license: Option<String>,
	pub sourcelicense: Option<String>,
	pub group: Option<String>,
	pub summary: Option<String>,
	pub sources: HashMap<i16, String>,
	pub patches: HashMap<i16, String>,
	// TODO icon
	// TODO nosource nopatch
	pub url: Option<String>,
	pub bugurl: Option<String>,
	pub modularitylabel: Option<String>,
	pub disttag: Option<String>,
	pub vcs: Option<String>,
	pub distribution: Option<String>,
	pub vendor: Option<String>,
	pub packager: Option<String>,
	// TODO buildroot
	pub autoreqprov: bool,
	pub autoreq: bool,
	pub autoprov: bool,
	pub requires: RPMRequires,
	pub provides: Vec<Package>,
	pub conflicts: Vec<Package>,
	pub obsoletes: Vec<Package>,
	pub recommends: Vec<Package>,
	pub suggests: Vec<Package>,
	pub supplements: Vec<Package>,
	pub enhances: Vec<Package>,
	pub orderwithrequires: Vec<Package>,
	pub buildrequires: Vec<Package>,
	pub buildconflicts: Vec<Package>,
	pub excludearch: Vec<String>,
	pub exclusivearch: Vec<String>,
	pub excludeos: Vec<String>,
	pub exclusiveos: Vec<String>,
	pub buildarch: Vec<String>, // BuildArchitectures BuildArch
	pub prefix: Option<String>, // Prefixes Prefix
	pub docdir: Option<String>,
	pub removepathpostfixes: Vec<String>,
}

impl RPMSpec {
	pub fn new() -> Self {
		Self {
			// buildroot
			autoreqprov: true,
			autoreq: true,
			autoprov: true,
			..Self::default()
		}
	}
}

/// A consumer that yields chars from a mutable String.
/// It is a bit more efficient if characters need to be
/// added into the String for the `.next()` iterations.
/// # Implementation
/// `Consumer` internally has `self.s` (String) storing
/// the output of the `BufReader` temporarily. However,
/// it is actually reversed. This is because operations
/// like `pop()` and `push()` are faster (`O(1)`) while
/// `remove(0)` and `insert(0, ?)` are slower (`O(n)`).
pub struct Consumer<R: std::io::Read = stringreader::StringReader<'static>> {
	s: String,
	r: Option<BufReader<R>>,
}

impl<R: std::io::Read> Consumer<R> {
	pub fn new(s: String, r: Option<BufReader<R>>) -> Self {
		Self { s: s.chars().rev().collect(), r }
	}
	#[inline]
	pub fn push<'a>(&mut self, c: char) {
		self.s.push(c)
	}
	#[inline]
	pub fn len(&self) -> usize {
		self.s.len()
	}
	pub fn read_til_eol(&mut self) -> Option<String> {
		let mut ps = vec![];
		let mut out = String::new();
		macro_rules! close {
			($ch:ident ~ $begin:expr, $end:expr) => {
				if $ch == $end {
					match ps.pop() {
						Some($begin) => continue,
						Some(x) => {
							error!("Found `{}` before closing `{x}`", $end);
							return None;
						}
						None => {
							error!("Unexpected closing char: `{}`", $end);
							return None;
						}
					}
				}
			};
		}
		'main: while let Some(ch) = self.next() {
			if ch == '\n' {
				break;
			}
			if "([{".contains(ch) {
				ps.push(ch);
				continue;
			}
			if ch == '\'' {
				ps.push('\'');
				for ch in self.by_ref() {
					ps.push(ch);
					if ch == '\'' {
						continue 'main;
					}
				}
				error!("Unexpected EOF, `'` not closed");
				return None;
			}
			if ch == '"' {
				ps.push('"');
				for ch in self.by_ref() {
					ps.push(ch);
					if ch == '"' {
						continue 'main;
					}
				}
				error!("Unexpected EOF, `\"` not closed");
				return None;
			}
			close!(ch ~ '(', ')');
			close!(ch ~ '[', ']');
			close!(ch ~ '{', '}');
			out.push(ch);
		}
		if !ps.is_empty() {
			error!("Unclosed: {ps:?}");
			return None;
		}
		Some(out)
	}
}

impl<R: std::io::Read> Iterator for Consumer<R> {
	type Item = char;

	fn next(&mut self) -> Option<Self::Item> {
		if let Some(c) = self.s.pop() {
			return Some(c);
		}
		if let Some(ref mut r) = self.r {
			let mut buf = [0; 64];
			if r.read(&mut buf).ok()? == 0 {
				None // EOF
			} else {
				self.s = match core::str::from_utf8(&buf) {
					Ok(s) => s.chars().rev().collect(),
					Err(e) => {
						error!("cannot parse buffer `{buf:?}`: {e}");
						return None;
					}
				};
				Some(unsafe { self.s.pop().unwrap_unchecked() })
			}
		} else {
			None
		}
	}
}

impl<R: std::io::Read> From<&str> for Consumer<R> {
	fn from(value: &str) -> Self {
		Consumer { s: value.chars().rev().collect(), r: None }
	}
}

#[derive(Default)]
pub struct SpecParser {
	pub rpm: RPMSpec,
	pub errors: Vec<Result<(), ParserError>>,
	pub macros: HashMap<String, String>,
}

pub struct SpecMacroParserIter<'a> {
	reader: &'a mut Consumer,
	parser: &'a mut SpecParser,
	percent: bool,
	buf: String,
}

impl<'a> Iterator for SpecMacroParserIter<'a> {
	type Item = char;
	fn next(&mut self) -> Option<Self::Item> {
		if !self.buf.is_empty() {
			return self.buf.pop();
		}
		if let Some(ch) = self.reader.next() {
			if ch == '%' {
				self.percent = !self.percent;
				if !self.percent {
					return Some('%');
				}
				return self.next();
			}
			if self.percent {
				match self.parser._read_raw_macro_use(self.reader) {
					Ok(s) => {
						self.buf = s.chars().rev().collect();
						return self.buf.pop();
					}
					Err(e) => {
						error!("Fail to parse macro: {e:#?}");
						return None;
					}
				}
			}
			return Some(ch);
		}
		None
	}
}

/// string operations / parsing with consumer
///
/// # Requires
/// - `exit!()`
///
/// # Provides
/// - `exit_chk!()`
/// - `back!()`
/// - `chk_ps!()`
/// - `quote_remain!()`
/// - `next!()`
///
/// FIXME: cannot parse the following:
/// ```sh
/// echo 'hai{'
/// ```
#[rustfmt::skip] // kamo https://github.com/rust-lang/rustfmt/issues/4609
macro_rules! gen_read_helper {
	($reader:ident $pa:ident $pb:ident $pc:ident $sq:ident $dq:ident $ret:expr) => {
		($pa, $pb, $pc) = (usize::default(), usize::default(), usize::default());
		($sq, $dq) = (false, false);
		macro_rules! exit_chk {
			() => {
				if $pa != 0 {
					error!("Unclosed `(` while parsing arguments for parameterized macro ({} time(s))", $pa);
					return $ret;
				}
				if $pb != 0 {
					error!("Unclosed `[` while parsing arguments for parameterized macro ({} time(s))", $pb);
					return $ret;
				}
				if $pc != 0 {
					error!("Unclosed `{{` while parsing arguments for parameterized macro ({} time(s))", $pc);
					return $ret;
				}
				if $sq {
					error!("Unclosed `'` while parsing arguments for parameterized macro ({} time(s))", $sq);
					return $ret;
				}
				if $dq {
					error!("Unclosed `\"` while parsing arguments for parameterized macro ({} time(s))", $dq);
					return $ret;
				}
			};
		}
		macro_rules! back {
			($ch:expr) => {
				match $ch {
					'(' => $pa -= 1,
					')' => $pa += 1,
					'[' => $pb -= 1,
					']' => $pb += 1,
					'{' => $pc -= 1,
					'}' => $pc += 1,
					'\'' => $sq = !$sq,
					'"' => $dq = !$dq,
					_ => {}
				}
				$reader.push($ch);
			};
		}
		macro_rules! chk_ps {
			($ch:ident) => {
				match $ch {
					'(' => $pa += 1,
					')' => $pa -= 1,
					'[' => $pb += 1,
					']' => $pb -= 1,
					'{' => $pc += 1,
					'}' => $pc -= 1,
					'\'' => $sq = !$sq,
					'"' => $dq = !$dq,
					_ => {}
				}
			};
		}
		#[allow(unused_macros)]
		macro_rules! quote_remain {
			() => {
				$pa + $pb + $pc != 0 || $sq || $dq
			};
		}
		#[allow(unused_macros)]
		macro_rules! next {
			($c:expr) => {
				if let Some(ch) = $reader.next() {
					chk_ps!(ch);
					ch
				} else {
					back!($c);
					exit!();
				}
			};
		}
	};
}

impl SpecParser {
	pub fn parse_macro<'a>(&'a mut self, reader: &'a mut Consumer) -> SpecMacroParserIter {
		SpecMacroParserIter { reader, parser: self, percent: false, buf: String::new() }
	}

	pub fn parse_requires(&mut self, sline: &str, ln: usize) -> bool {
		lazy_static! { // move outside FIXME
			static ref RE1: Regex =
				Regex::new(r"(?m)^Requires(?:\(([\w,\s]+)\))?:\s*(.+)$").unwrap();
			static ref RE2: Regex =
				Regex::new(r"(?m)([\w-]+)(?:\s*([>=<]{1,2})\s*([\d._~^]+))?").unwrap();
		}
		if let Some(caps) = RE1.captures(sline) {
			let spkgs = caps[caps.len()].trim();
			let mut pkgs = vec![];
			Package::add_query(&mut pkgs, spkgs).unwrap(); // fixme
			let modifiers = if caps.len() == 2 { &caps[2] } else { "none" };
			for modifier in modifiers.split(',') {
				let modifier = modifier.trim();
				let pkgs = pkgs.to_vec();
				match modifier {
					"none" => self.rpm.requires.none.extend(pkgs),
					"pre" => self.rpm.requires.pre.extend(pkgs),
					"post" => self.rpm.requires.post.extend(pkgs),
					"preun" => self.rpm.requires.preun.extend(pkgs),
					"postun" => self.rpm.requires.postun.extend(pkgs),
					"pretrans" => self.rpm.requires.pretrans.extend(pkgs),
					"posttrans" => self.rpm.requires.posttrans.extend(pkgs),
					"verify" => self.rpm.requires.verify.extend(pkgs),
					"interp" => self.rpm.requires.interp.extend(pkgs),
					"meta" => self.rpm.requires.meta.extend(pkgs),
					// _ => bail!("Unknown Modifier '{}' for Requires", modifier),
					_ => {
						self.errors.push(Err(ParserError::UnknownModifier(ln, modifier.into())));
					}
				}
			}
			return true;
		}
		false
	}
	pub fn arch() -> Result<String> {
		let binding = Command::new("uname").arg("-m").output()?;
		let s = core::str::from_utf8(&binding.stdout)?;
		Ok(s[..s.len() - 1].into()) // remove new line
	}
	// not sure where I've seen the docs, but there was one lying around saying you can define multiple
	// macros with the same name, and when you undefine it the old one recovers (stack?). I don't think
	// it is a good idea to do it like that (it is simply ridiculous and inefficient) but you can try
	pub fn load_macros(&mut self) -> Result<()> {
		// run rpm --showrc | grep "^Macro path"
		let binding = Command::new("sh").args(["-c", "rpm --showrc|grep '^Macro path'|sed 's/Macro path: //'"]).output()?;
		let binding = core::str::from_utf8(&binding.stdout)?;
		let paths = binding.trim().split(':');

		// TODO use Consumer::read_til_EOL() instead
		let re = Regex::new(r"(?m)^%([\w()]+)[\t ]+((\\\n|[^\n])+)$").unwrap();
		for path in paths {
			let path = path.replace("%{_target}", Self::arch()?.as_str());
			debug!(": {path}");
			for path in glob::glob(path.as_str())? {
				let path = path?;
				debug!("{}", path.display());
				let mut buf = vec![];
				let bytes = BufReader::new(File::open(&path)?).read_to_end(&mut buf)?;
				assert_ne!(bytes, 0, "Empty macro definition file '{}'", path.display());
				for cap in re.captures_iter(std::str::from_utf8(&buf)?) {
					if let Some(val) = self.macros.get(&cap[1]) {
						debug!("Macro Definition duplicated: {} : '{val:?}' | '{}'", &cap[1], &cap[2]);
						continue; // FIXME?
					}
					let name = &cap[1];
					if name.ends_with("()") {
						let mut content = String::from(&cap[2]);
						content.push(' '); // yup, we mark it using a space.
						self.macros.insert(unsafe { name.strip_suffix("()").unwrap_unchecked() }.into(), content);
					}
					// we trim() just in case
					self.macros.insert(cap[1].into(), cap[2].trim().into());
				}
			}
		}
		Ok(())
	}
	pub fn parse<R: std::io::Read>(&mut self, bufread: BufReader<R>) -> Result<()> {
		let re_preamble = Regex::new(r"(\w+):\s*(.+)").unwrap();
		let re_dnl = Regex::new(r"^%dnl\b").unwrap();
		let re_digit = Regex::new(r"\d+$").unwrap();
		// FIXME use Consumer::read_til_eol()?
		// TODO proper section handling
		for (line_number, line) in bufread.lines().enumerate() {
			let line = self._expand_macro(&mut Consumer::from(&*line?)).map_err(|e| e.wrap_err(format!("Cannot expand macro on line {line_number}")))?;
			let sline = line.trim();
			if sline.is_empty() || sline.starts_with('#') || re_dnl.is_match(sline) {
				continue;
			}
			// Check for Requires special preamble syntax first
			if self.parse_requires(sline, line_number) {
				continue;
			}
			// only then do we check for other preambles
			for cap in re_preamble.captures_iter(sline) {
				// check for list_preambles
				if let Some(digitcap) = re_digit.captures(&cap[1]) {
					let sdigit = &digitcap[0];
					let digit: i16 = sdigit.parse()?;
					let name = &cap[1][..cap[1].len() - sdigit.len()];
					self.add_list_preamble(name, digit, &cap[2])?;
				} else {
					self.add_preamble(&cap[1], cap[2].into(), line_number)?;
				}
			}
		}
		if !self.errors.is_empty() {
			return take(&mut self.errors).into_iter().map(Result::unwrap_err).fold(Err(eyre!("Cannot parse spec file")), |report, e| report.error(e));
		}
		Ok(())
	}

	pub fn add_list_preamble(&mut self, name: &str, digit: i16, value: &str) -> Result<()> {
		let value = value;
		let rpm = &mut self.rpm;
		macro_rules! no_override_ins {
			($attr:ident) => {{
				if let Some(old) = rpm.$attr.insert(digit, value.into()) {
					error!("Overriding preamble `{name}{digit}` value `{old}` -> `{value}`");
				}
			}};
		}
		match name {
			"Source" => no_override_ins!(sources),
			"Patch" => no_override_ins!(patches),
			_ => return Err(eyre!("Failed to match preamble `{name}{digit}` (value `{value}`)")),
		}
		Ok(())
	}

	pub fn add_preamble(&mut self, name: &str, value: String, ln: usize) -> Result<()> {
		let rpm = &mut self.rpm;

		macro_rules! opt {
			($x:ident $y:ident) => {
				if name == stringify!($x) {
					if let Some(ref old) = rpm.$y {
						warn!(
							"overriding existing {} preamble value `{old}` to `{value}`",
							stringify!($x)
						);
						self.errors.push(Err(ParserError::Duplicate(ln, stringify!($x).into())));
					}
					rpm.name = Some(value);
					return Ok(());
				}
			};
			(~$x:ident $y:ident) => {
				if name == stringify!($x) {
					rpm.name = Some(value.parse()?);
					return Ok(());
				}
			};
			(%$x:ident $y:ident) => {
				if name == stringify!($x) {
					rpm.$y.append(&mut value.split_whitespace().map(|s| s.into()).collect());
					return Ok(());
				}
			};
			($a:ident $b:ident | $($x:ident $y:ident)|+) => {
				opt!($a $b);
				opt!($($x $y)|+);
			}
		}

		opt!(Name name|Version version|Release release|License license|SourceLicense sourcelicense|URL url|BugURL bugurl|ModularityLabel modularitylabel|DistTag disttag|VCS vcs|Distribution distribution|Vendor vendor|Packager packager);
		opt!(Group group); // todo subpackage
		opt!(Summary summary); // todo subpackage
		opt!(~AutoReqProv autoreqprov);
		opt!(~AutoReq autoreq);
		opt!(~AutoProv autoprov);
		opt!(%ExcludeArch excludearch);
		opt!(%ExclusiveArch exclusivearch);
		opt!(%ExcludeOS exclusiveos);
		opt!(%ExclusiveOS exclusiveos);
		opt!(%BuildArch buildarch);
		opt!(%BuildArchitectures buildarch);

		match name {
			"Epoch" => {
				if let Some(old) = rpm.epoch {
					warn!("Overriding existing Epoch preamble value `{old}` to `{value}`");
				}
				rpm.epoch = Some(value.parse().expect("Failed to decode epoch to int"));
			}
			"Provides" => Package::add_query(&mut rpm.provides, &value)?,              // todo subpackage
			"Conflicts" => Package::add_query(&mut rpm.conflicts, &value)?,            // todo subpackage
			"Obsoletes" => Package::add_query(&mut rpm.obsoletes, &value)?,            // todo subpackage
			"Recommends" => Package::add_simple_query(&mut rpm.recommends, &value)?,   // todo subpackage
			"Suggests" => Package::add_simple_query(&mut rpm.suggests, &value)?,       // todo subpackage
			"Supplements" => Package::add_simple_query(&mut rpm.supplements, &value)?, // todo subpackage
			"Enhances" => Package::add_simple_query(&mut rpm.enhances, &value)?,       // todo subpackage
			"BuildRequires" => Package::add_query(&mut rpm.buildrequires, &value)?,
			"OrderWithRequires" => todo!(),
			"BuildConflicts" => todo!(),
			"Prefixes" => todo!(),
			"Prefix" => todo!(),
			"DocDir" => todo!(),
			"RemovePathPostfixes" => todo!(),
			_ => bail!("BUG: failed to match preamble '{name}'"),
		}
		Ok(())
	}

	fn _internal_macro(&mut self, name: &str, reader: &mut Consumer) -> Option<String> {
		match name {
			"define" | "global" => {
				let def = reader.read_til_eol()?;
				if let Some((name, def)) = def.split_once(' ') {
					let name: String = if let Some(x) = name.strip_suffix("()") { format!("{x} ").into() } else { name.into() };
					self.macros.insert(name, def.into());
					Some("".into())
				} else {
					error!("Invalid syntax: `%define {def}`");
					None
				}
			}
			"undefine" => {
				self.macros.remove(name);
				Some("".into())
			}
			"load" => unimplemented!(),
			"expand" => self._expand_macro(reader).ok(),
			"expr" => unimplemented!(),
			"lua" => unimplemented!(),
			"macrobody" => unimplemented!(),
			"quote" => unimplemented!(),
			"gsub" => unimplemented!(),
			"len" => unimplemented!(),
			"lower" => unimplemented!(),
			"rep" => unimplemented!(),
			"reverse" => unimplemented!(),
			"sub" => unimplemented!(),
			"upper" => unimplemented!(),
			"shescape" => unimplemented!(),
			"shrink" => unimplemented!(),
			"basename" => unimplemented!(),
			"dirname" => unimplemented!(),
			"exists" => unimplemented!(),
			"suffix" => unimplemented!(),
			"url2path" => unimplemented!(),
			"uncompress" => unimplemented!(),
			"getncpus" => unimplemented!(),
			"getconfidir" => unimplemented!(),
			"getenv" => unimplemented!(),
			"rpmversion" => unimplemented!(),
			"echo" => unimplemented!(),
			"warn" => unimplemented!(),
			"error" => unimplemented!(),
			"verbose" => unimplemented!(),
			"S" => unimplemented!(),
			"P" => unimplemented!(),
			"trace" => unimplemented!(),
			"dump" => unimplemented!(),
			_ => None,
		}
	}

	/// parses:
	/// ```
	/// %macro_name -a -b hai bai idk \
	///   more args idk
	/// ```
	/// but not:
	/// ```
	/// %{macro_name:hai bai -f -a}
	fn _param_macro_line_args(&mut self, reader: &mut Consumer) -> Option<(String, Vec<String>, Vec<char>)> {
		// we start AFTER %macro_name
		let mut content = String::new();
		let mut flags = vec![];
		let (mut pa, mut pb, mut pc, mut sq, mut dq);
		gen_read_helper!(reader pa pb pc sq dq None);
		macro_rules! exit {
			() => {
				exit_chk!();
				let args = content.split(' ').filter(|x| !x.starts_with('-')).map(|x| x.into()).collect();
				return Some((content, args, flags));
			};
		}
		'main: while let Some(ch) = reader.next() {
			chk_ps!(ch);
			if ch == '%' {
				let ch = next!('%');
				if ch == '%' {
					content.push('%');
					continue;
				}
				back!(ch);
				content.push_str(&self._read_raw_macro_use(reader).ok()?);
				continue;
			}
			if ch == '-' {
				let ch = next!('-');
				if ch.is_alphabetic() {
					let next = next!(ch);
					if "\\ \n".contains(next) {
						back!(next);
						flags.push(ch);
						content.push('-');
						content.push(ch);
						continue;
					} else {
						error!("Found character `{next}` after `-{ch}` in parameterized macro");
						return None;
					}
				} else {
					error!("Argument flag `-{ch}` in parameterized macro is not alphabetic");
					return None;
				}
			}
			if ch == '\\' {
				let mut got_newline = false;
				content = content.trim_end().into();
				content.push(' ');
				while let Some(ch) = reader.next() {
					chk_ps!(ch);
					if ch == '\n' {
						got_newline = true;
					} else if !ch.is_whitespace() {
						if got_newline {
							back!(ch);
							continue 'main;
						} else {
							error!("Got `{ch}` after `\\` before new line");
							return None;
						}
					}
				}
				error!("Unexpected EOF after `\\`");
			}
			if ch == '\n' && !quote_remain!() {
				exit!();
			}
			// compress whitespace to ' '
			if ch.is_whitespace() && content.chars().last().map_or(false, |l| !l.is_whitespace()) {
				content.push(' ');
			} else if !ch.is_whitespace() {
				content.push(ch);
			}
		}
		exit!();
	}

	fn _param_macro(&mut self, name: &str, def: &mut Consumer, reader: &mut Consumer) -> Option<String> {
		let (raw_args, args, flags) = self._param_macro_line_args(reader)?;
		let mut res = String::new();
		let (mut pa, mut pb, mut pc, mut sq, mut dq);
		gen_read_helper!(def pa pb pc sq dq None);
		macro_rules! exit {
			// for gen_read_helper!()
			() => {
				exit_chk!(); // maybe? FIXME
				return None;
			};
		}
		'main: while let Some(ch) = def.next() {
			chk_ps!(ch);
			if ch != '%' {
				res.write_char(ch).unwrap();
				continue;
			}
			let ch = next!('%');
			if ch == '%' {
				res.push('%');
				continue;
			}
			// https://rpm-software-management.github.io/rpm/manual/macros.html
			match ch {
				'*' => {
					let follow = next!('*');
					if follow == '*' {
						res.push_str(&raw_args); // %**
					} else {
						back!(follow);
						res.push_str(&args.join(" ")); // %*
					}
					continue;
				}
				'#' => {
					res.push_str(&args.len().to_string());
					continue;
				}
				'0' => {
					res.push_str(name);
					continue;
				}
				'{' => {
					let req_pc = pc - 1;
					let mut content = String::new();
					for ch in def.by_ref() {
						chk_ps!(ch);
						if req_pc != pc {
							content.push(ch);
							continue;
						}
						// found `}`
						let mut notflag = false;
						if let Some(x) = content.strip_prefix('!') {
							notflag = true;
							content = x.into();
						}
						let expand = {
							let binding = content.clone();
							if let Some((name, e)) = binding.split_once(':') {
								content = name.to_string().into();
								e.into()
							} else {
								binding
							}
						};
						if !content.starts_with('-') {
							// normal stuff
							res.push_str(
								&self._read_raw_macro_use(&mut Consumer::from(&*format!("{{{content}}}"))).ok()?, // FIXME (err hdl?)
							);
						}
						if let Some(content) = content.strip_suffix('*') {
							if content.len() != 2 {
								error!("Invalid macro param flag `%{{{content}}}`");
								return None;
							}
							let mut argv = raw_args.split(' ');
							if !notflag {
								if let Some(n) = argv.clone().enumerate().find_map(|(n, x)| if x == content { Some(n) } else { None }) {
									if let Some(arg) = argv.nth(n + 1) {
										res.push_str(arg);
									}
								}
							}
							// if there are no args after -f, add nothing.
							continue 'main;
						}
						if content.len() != 2 {
							error!("Found `%-{content}` which is not a flag");
							return None;
						}
						let flag = unsafe { content.chars().last().unwrap_unchecked() };
						if !flag.is_alphabetic() {
							error!("Invalid macro name `%-{flag}`");
							return None;
						}
						if flags.contains(&flag) ^ notflag {
							res.push_str(&expand);
						}
						continue 'main;
					}
					error!("Unexpected EOF while parsing `%{{...`");
					return None;
				}
				_ if ch.is_numeric() => {
					let mut macroname = String::new();
					macroname.push(ch);
					// no need chk_ps!(), must be numeric
					while let Some(ch) = def.next() {
						if !ch.is_numeric() {
							def.push(ch);
							break;
						}
						macroname.push(ch);
					}
					let ret = match macroname.parse::<usize>() {
						Ok(n) => args.get(n - 1),
						Err(e) => {
							error!("Cannot parse macro param `%{macroname}`: {e}");
							return None;
						}
					};
					res.push_str(ret.unwrap_or(&String::new()));
				}
				_ => {
					def.push(ch);
					res.push_str(&self._read_raw_macro_use(def).ok()?);
				}
			}
		}
		exit_chk!();
		Some(res)
	}

	fn _expand_macro(&mut self, chars: &mut Consumer) -> Result<String> {
		let mut res = String::new();
		while let Some(ch) = chars.next() {
			if ch == '%' {
				if let Some(ch) = chars.next() {
					if ch == '%' {
						res.push('%');
						continue;
					}
					chars.push(ch);
					res.push_str(&self._read_raw_macro_use(chars)?);
					continue;
				}
			}
			res.push(ch);
		}
		Ok(res)
	}

	fn _rp_macro(&mut self, name: &str, reader: &mut Consumer) -> Option<String> {
		debug!("getting %{name}");
		if let Some(expanded) = self._internal_macro(name, reader) {
			return Some(expanded);
		}
		let def = self.macros.get(name)?;
		// Refactor at your own risk: impossible due to RAII. Fail counter: 2
		if let Some(def) = def.strip_suffix(' ') {
			// parameterized macro
			let out = self._param_macro(name, &mut Consumer::from(def), reader)?;
			self._expand_macro(&mut Consumer::from(&*out)).ok()
		} else {
			// it's just text
			self._expand_macro(&mut Consumer::from(def.as_str())).ok()
		}
	}

	/// parse the stuff after %, and determines `{[()]}`. returns expanded macro.
	/// Assumption: `chars` is a str Consumer (no reader).
	/// FIXME please REFACTOR me!!
	fn _read_raw_macro_use(&mut self, chars: &mut Consumer) -> Result<String> {
		debug!("reading macro");
		let (mut notflag, mut question) = (false, false);
		let mut content = String::new();
		let l = chars.len();
		let (mut pa, mut pb, mut pc, mut sq, mut dq);
		gen_read_helper!(chars pa pb pc sq dq Err(eyre!("Unmatched quotes")));
		macro_rules! flagmacrohdl {
			($name:expr, $consumer:expr) => {
				exit_chk!();
				let out = self._rp_macro($name, &mut Consumer::from($consumer));
				if notflag {
					if question {
						return Ok("".into());
					}
					// when %a is undefined, %{!a} expands to %{!a}, but %!a expands to %a
					return Ok(out.unwrap_or_else(|| if content.is_empty() { format!("%{}", $name) } else { format!("%{{!{}}}", $name) }.into()));
				}
				return Ok(out.unwrap_or_default());
			};
			($name:expr) => {
				flagmacrohdl!($name, "")
			};
		}
		while let Some(ch) = chars.next() {
			chk_ps!(ch);
			// we read until we encounter '}' or ':' or the end
			match ch {
				'!' => notflag = !notflag,
				'?' => {
					if question {
						warn!("Seeing double `?` flag in macro use. Ignoring.");
					}
					question = true;
				}
				'{' => {
					let req_pc = pc - 1;
					if chars.len() + 1 != l {
						back!('{');
						break;
					}
					let mut name = String::new();
					while let Some(ch) = chars.next() {
						chk_ps!(ch);
						if pc == req_pc {
							flagmacrohdl!(&name);
						}
						if ch == ':' {
							let mut content = String::new();
							for ch in chars.by_ref() {
								chk_ps!(ch);
								if pc == req_pc {
									if question {
										return Ok(if self.macros.contains_key(&name) ^ notflag { self.parse_macro(&mut Consumer::from(&*content)).collect() } else { "".into() });
									}
									flagmacrohdl!(&name, &*content);
								}
								content.push(ch);
							}
							return Err(eyre!("EOF but `%{{...:...` is not ended with `}}`"));
						}
						if ch == '?' {
							if question {
								warn!("Seeing double `?` flag in macro use. Ignoring.");
							}
							question = true;
							continue;
						}
						if ch == '!' {
							notflag = !notflag;
							continue;
						}
						name.push(ch);
					}
					return Err(eyre!("EOF while parsing `%{{...`"));
				}
				'(' => {
					if !content.is_empty() {
						back!('(');
						break;
					}
					if notflag || question {
						warn!("flags (! and ?) are not supported for %().");
					}
					let mut shellcmd = std::string::String::new();
					let req_pa = pa - 1;
					for ch in chars.by_ref() {
						chk_ps!(ch);
						if pa == req_pa {
							return match Command::new("sh").arg("-c").arg(&shellcmd).output() {
								Ok(out) => {
									if out.status.success() {
										Ok(core::str::from_utf8(&out.stdout)?.trim_end_matches('\n').into())
									} else {
										Err(eyre!("Shell expansion command did not succeed")
											.note(out.status.code().map_or("No status code".into(), |c| format!("Status code: {c}")))
											.section(core::str::from_utf8(&out.stdout)?.to_string().header("Stdout:"))
											.section(core::str::from_utf8(&out.stderr)?.to_string().header("Stderr:")))
									}
								}
								Err(e) => Err(eyre!(e).wrap_err("Shell expansion failed").note(shellcmd)),
							};
						}
						shellcmd.push(ch);
					}
					return Err(eyre!("Unexpected end of shell expansion command: `%({shellcmd}`"));
				}
				'[' => {
					todo!("what does %[] mean? www")
				}
				_ if ch.is_alphanumeric() || ch == '_' => content.push(ch),
				_ => {
					back!(ch);
					break;
				}
			}
		}
		flagmacrohdl!(&content);
	}

	pub fn new() -> Self {
		Self { rpm: RPMSpec::new(), errors: vec![], macros: HashMap::new() }
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use std::fs::File;

	#[test]
	fn parse_spec() -> Result<()> {
		let f = File::open("../tests/test.spec")?;
		let f = BufReader::new(f);

		let mut sp = SpecParser::new();
		sp.parse(f)?;
		println!("{}", sp.rpm.name.unwrap_or_default());
		println!("{}", sp.rpm.summary.unwrap_or_default());
		Ok(())
	}
	#[test]
	fn test_load_macros() -> Result<()> {
		println!("{}", SpecParser::arch()?);
		let mut sp = SpecParser::new();
		sp.load_macros()?;
		println!("{:#?}", sp.macros);
		Ok(())
	}
	#[test]
	fn simple_macro_expand() -> Result<()> {
		let mut parser = super::SpecParser::new();
		parser.macros.insert("macrohai".into(), "hai hai".into());
		assert_eq!(parser._read_raw_macro_use(&mut ("macrohai".into()))?, "hai hai");
		Ok(())
	}
	#[test]
	fn text_recursive_macro_expand() -> Result<()> {
		let mut parser = super::SpecParser::new();
		parser.macros.insert("mhai".into(), "hai hai".into());
		parser.macros.insert("quadhai".into(), "%mhai %{mhai}".into());
		assert_eq!(parser._read_raw_macro_use(&mut ("quadhai".into()))?, "hai hai hai hai");
		Ok(())
	}
	#[test]
	fn text_quoting_recursive_macro_expand() -> Result<()> {
		let mut parser = super::SpecParser::new();
		parser.macros.insert("mhai".into(), "hai hai".into());
		parser.macros.insert("idk".into(), "%!?mhai %?!mhai %{mhai}".into());
		parser.macros.insert("idk2".into(), "%{?mhai} %{!mhai} %{!?mhai} %{?!mhai}".into());
		parser.macros.insert("aaa".into(), "%idk %idk2".into());
		assert_eq!(parser._read_raw_macro_use(&mut ("aaa".into()))?, "  hai hai hai hai hai hai  ");
		Ok(())
	}
	#[test]
	fn shell_macro_expand() -> Result<()> {
		let mut parser = super::SpecParser::new();
		parser.macros.insert("x".into(), "%(echo haai | sed 's/a/aa/g')".into());
		assert_eq!(parser._read_raw_macro_use(&mut ("x".into()))?, "haaaai");
		Ok(())
	}
	#[test]
	fn presence_macro_expand() -> Result<()> {
		let mut parser = super::SpecParser::new();
		parser.macros.insert("x".into(), "%{?not_exist:hai}%{!?not_exist:bai}".into());
		assert_eq!(parser._read_raw_macro_use(&mut ("x".into()))?, "bai");
		parser.macros.insert("not_exist".into(), "wha".into());
		assert_eq!(parser._read_raw_macro_use(&mut ("x".into()))?, "hai");
		Ok(())
	}
	#[test]
	fn param_macro_args_parsing() {
		let mut parser = super::SpecParser::new();
		assert_eq!(
			parser._param_macro_line_args(&mut Consumer::from("-a hai -b asdfsdklj \\  \n abcd\ne")),
			Some(("-a hai -b asdfsdklj abcd".into(), vec!["hai".into(), "asdfsdklj".into(), "abcd".into()], vec!['a', 'b']))
		);
	}
}
// BUG: %{hai:{}} <- this breaks because two `}`
