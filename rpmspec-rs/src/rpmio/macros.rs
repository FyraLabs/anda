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
    spec::Macro,
};
use anyhow::{anyhow, bail, Ok, Result};
use log::{debug, info, warn};
use std::{
    collections::HashMap,
    io::{stderr, BufRead, BufReader, Write},
    sync::{Arc, Mutex, MutexGuard},
};

type Func = fn(MacroBuf, Entry, Vec<String>, &usize);
#[derive(Clone)]
struct Entry {
    prev: Option<Box<Self>>, // Macro entry stack
    name: String,            // Macro name
    opts: String,            // Macro parameters
    body: String,            // Macro body
    func: Func,              // Macro function (builtin macros)
    nargs: u8,               // No. required args
    flags: i16,              // Macro state bits
    level: u16,              // Scoping level
    arena: String,           // String arena
}
struct MacroContext {
    // in the original C code, Entry[] was used.
    // we switched to HashMap to improve find_entry() perf.
    table: HashMap<String, Entry>,
    n: u16,     // No. macros
    depth: u8,  // Depth tracking when recursing from Lua
    level: u16, // Scope level tracking when recursing from Lua
}
impl MacroContext {
    fn new() -> Self {
        Self {
            table: HashMap::new(),
            n: 0,
            depth: 0,
            level: 0,
        }
    }
}
type Context = Arc<Mutex<MacroContext>>;
#[derive(Clone)]
struct MacroBuf {
    buf: String,       // Expansion buffer
    tops: usize,       // Current position in buf
    nb: usize,         // No. bytes remaining in buf
    depth: u8,         // Current expansion depth
    level: u16,        // Current scoping level
    error: bool,        // Errors encountered during expansion?
    mtrace: bool,       // Pre-print macro to expand?  (macro_trace)
    etrace: bool,       // Post-print macro expansion? (expand_trace)
    flags: i16,        // Flags to control behaviour
    me: Option<Entry>, // Current macro
    args: Vec<String>, // Current macro arguments
    mc: Context,
}
struct MacroExpansionData {
    tpos: usize,
    mtrace: i16,
    etrace: i16,
}
const MAX_MACRO_DEPTH: u8 = 64;
const PRINT_MACRO_TRACE: bool = false;
const PRINT_EXPAND_TRACE: bool = false;

impl MacroBuf {
    fn new(mc: MacroContext, flags: i16) -> Self {
        Self {
            buf: "".to_string(),
            depth: mc.depth,
            level: mc.level,
            mtrace: PRINT_MACRO_TRACE,
            etrace: PRINT_EXPAND_TRACE,
            mc: Arc::from(Mutex::from(mc)),
            flags,
            tops: 0,
            args: vec![],
            error: false,
            me: None,
            nb: 0,
        }
    }
}


struct SaiGaai {
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
    fn expand_macro(src: &str) -> Result<()> {
        Ok(())
    }
    fn get_ctx(mc: &Context) -> Result<MutexGuard<MacroContext>> {
        Ok(mc.lock().expect("Can't lock mc"))
    }
    fn find_entry(&self, mc: Context, name: String) -> Result<Entry> {
        // original code use binary search
        let ctx = Self::get_ctx(&mc)?;
        Ok(ctx
            .table
            .get(&name)
            .ok_or(ParserError::UnknownMacro(self.line, name))?
            .clone())
    }
    fn new_entry(&self, mc: Context, key: String, value: Entry) -> Result<()> {
        // no need extend macro table
        // instead get it out of the mutex
        let mut ctx = Self::get_ctx(&mc)?;
        ctx.n += 1;
        if let Some(x) = ctx.table.insert(key, value) {
            // For debugging. Actually it's normal,
            // but dunno why happens itfp.
            debug!("Macro duplicated: {}", x.name);
        }
        Ok(())
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
    fn printMacro(mb: MacroBuf, mut s: &str, se: &str) -> Result<()> {
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
    /// -> Post-print expanded macro expression.
    /// WARN `t` and `te` should take until EOS
    fn printExpansion(mb: MacroBuf, me: Option<Entry>, t: &str, te: &str) -> Result<()> {
        let mname = me.map_or("".into(), |m| m.name);
        if te.len() <= t.len() {
            let mut stderr = stderr().lock();
            stderr.write_fmt(format_args!(
                "{:>3}>{} (%{})\n",
                mb.depth,
                " ".repeat((2 * mb.depth + 1).into()),
                mname
            ))?;
            return Ok(());
        }
        // -> Shorten output which contains newlines
        let te = te.trim_end_matches('\n');
        // ^ Assume: te > t
        let t = if mb.depth > 0 {
            // Assume no trailing \n
            te.lines().last().unwrap_or(te)
        } else { t };
        let mut stderr = stderr().lock();
        stderr.write_fmt(format_args!(
            "{:>3}>{} (%{})\n",
            mb.depth,
            " ".repeat((2 * mb.depth + 1).into()),
            mname
        ))?;
        if t.len() > te.len() {
            stderr.write_fmt(format_args!("{}", &t[..t.len()-te.len()-1]))?;
        }
        Ok(())
    }
    fn expandThis(mb: MacroBuf, src: &str) -> (String, bool) {
        let mut umb = mb;
        umb.buf = "".to_string();
        let err = Self::expandMacro(&umb, src) != 0;

        (umb.buf, err)
    }
    // They kinda have a lot of malloc and stuff
    fn rpmExpand(args: Vec<String>) -> String {
        let pe = args.join("");
        let mc = MacroContext::new();
        todo!()
    }
    fn doExpandMacros(mc: MacroContext, src: String, flags: u32) -> Result<(String, u16)> {
        todo!()
    }
    fn expandMacro(mb: &MacroBuf, src: &str) -> usize {
        todo!()
    }
}
macro_rules! mbErr {
    ($mb:expr, $error:expr, $fmt:expr, $($ap:tt)*) => {{
        let emsg = format!($fmt, $ap);
        let pfx = SaiGaai::rpmExpand(
            "%{?__file_name:%{__file_name}: }",
            "%{?__file_lineno:line %{__file_lineno}: }",
        );
        // I have no idea why original C code incl'd NULL in args
        rpmlog(
            if $error {
                RPMLogLvl::ERR
            } else {
                RPMLogLvl::WARN
            },
            "{}{}",
            pfx,
            emsg,
        );
        if $error {
            $mb.error = error;
        }
    }};
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
