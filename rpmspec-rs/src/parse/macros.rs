use std::{sync::{Arc, Mutex, MutexGuard}, collections::HashMap, io::{BufRead, BufReader}};
use anyhow::{anyhow, bail, Ok, Result};
use crate::{error::{self, ParserError}, spec::Macro};
use log::{warn, info, debug};

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
    n: u16,            // No. macros
    depth: u8,        // Depth tracking when recursing from Lua
    level: u16,        // Scope level tracking when recursing from Lua
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
struct MacroBuf {
    buf: String,       // Expansion buffer
    tops: usize,       // Current position in buf
    nb: usize,         // No. bytes remaining in buf
    depth: u8,         // Current expansion depth
    level: u16,        // Current scoping level
    error: u16,        // Errors encountered during expansion?
    mtrace: i16,       // Pre-print macro to expand?  (macro_trace)
    etrace: i16,       // Post-print macro expansion? (expand_trace)
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
        // original code using binary search
        let ctx = Self::get_ctx(&mc)?;
        Ok(ctx.table.get(&name).ok_or(ParserError::UnknownMacro(self.line, name))?.clone())
    }
    fn new_entry(&self, mc: Context, key: String, value: Entry) -> Result<()> {
        // we don't have to extend our macro table,
        // instead we just get it out of the mutex
        let mut ctx = Self::get_ctx(&mc)?;
        ctx.n += 1;
        if let Some(x) = ctx.table.insert(key, value) {
            warn!("Macro duplicated: {}", x.name);
        }
        Ok(())
    }

    // * fgets(3) analogue that reads \ continuations. Last newline always trimmed.
    // in this case, we probably prefer a bufread to throw newlines to us.
    // then we trim and check for \, but also {[( stuff like\n these )]}
    // we don't need the size parameter *I think*...
    // I mean it says it's the *inbut* (yes, inbut) buffer size (bytes).
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
                    _ => {},
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

}
