use std::{sync::{Arc, Mutex}, collections::HashMap};
use anyhow::{anyhow, bail, Ok, Result};

use crate::error::{self, ParserError};

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
type Context = Arc<Mutex<Option<MacroContext>>>;
struct MacroBuf {
    buf: String,       // Expansion buffer
    tops: usize,       // Current position in buf
    nb: usize,         // No. bytes remaining in buf
    depth: u8,        // Current expansion depth
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
}
impl SaiGaai {
    fn new() -> Self {
        // initLocks()
        Self {
            global_mctx: Context::new(Mutex::new(None)),
            cli_mctx: Context::new(Mutex::new(None))
        }

    }
    fn expand_macro(src: &str) -> Result<()> {
        Ok(())
    }
    fn find_entry(mc: Context, name: String, line: usize) -> Result<Entry> {
        let a = mc.clone().lock()?;
        let a = a.take().unwrap();
        // let a = mc.get_mut()?.expect("No MacroContext");
        Ok(a.table.get(&name).ok_or(ParserError::UnknownMacro(line, name))?.clone())
    }
}
