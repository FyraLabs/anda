use crate::{
    error::{AndaxError, AndaxRes, TbErr},
    io,
    update::{self, re, rpm, tsunagu},
};
use regex::Regex;
use rhai::{plugin::*, Engine, EvalAltResult, NativeCallContext as CallCtx, Scope};
use std::{
    fs::File,
    io::{BufRead, BufReader},
    path::PathBuf,
    rc::Rc,
};
use tracing::{debug, error, instrument, trace, warn};

fn json(ctx: CallCtx, a: String) -> Result<rhai::Map, Box<EvalAltResult>> {
    ctx.engine().parse_json(a, true)
}
fn json_arr(ctx: CallCtx, a: String) -> Result<rhai::Array, Box<EvalAltResult>> {
    serde_json::from_str(&a).ehdl(&ctx)
}

pub(crate) fn rf<T>(ctx: CallCtx, res: color_eyre::Result<T>) -> Result<T, Box<EvalAltResult>>
where
    T: rhai::Variant + Clone,
{
    res.map_err(|err| {
        Box::new(EvalAltResult::ErrorRuntime(
            Dynamic::from(AndaxError::RustReport(
                ctx.fn_name().into(),
                ctx.source().unwrap_or("").into(),
                Rc::from(err),
            )),
            ctx.position(),
        ))
    })
}

fn gen_en() -> (Engine, Scope<'static>) {
    let mut sc = Scope::new();
    sc.push("USER_AGENT", tsunagu::USER_AGENT);
    sc.push("IS_WIN32", cfg!(windows));
    let mut en = Engine::new();
    en.register_fn("json", json)
        .register_fn("json_arr", json_arr)
        .register_fn("find", |ctx: CallCtx, a, b, c| rf(ctx, re::find(a, b, c)))
        .register_fn("sub", |ctx: CallCtx, a, b, c| rf(ctx, re::sub(a, b, c)))
        .register_global_module(exported_module!(io::anda_rhai).into())
        .register_global_module(exported_module!(update::tsunagu::anda_rhai).into())
        .build_type::<update::tsunagu::Req>()
        .build_type::<rpm::RPMSpec>();
    (en, sc)
}

/// Generates Error description from nanitozo \
/// used in `_tb()`
fn _gemsg(nanitozo: &TbErr) -> String {
    match nanitozo {
        TbErr::Report(o) => format!("From: {o}"),
        TbErr::Arb(o) => format!("From: {o}"),
        TbErr::Rhai(o) => format!("Rhai: {o}"),
    }
}

fn _gpos(p: Position) -> Option<(usize, usize)> {
    p.line().map(|l| (l, p.position().unwrap_or(0)))
}

#[instrument(name = "traceback")]
pub fn _tb(
    proj: &str,
    scr: &PathBuf,
    nanitozo: TbErr,
    pos: Position,
    rhai_fn: &str,
    fn_src: &str,
) {
    if let Some((line, col)) = _gpos(pos) {
        // Print code
        match File::open(scr) {
            Ok(f) => {
                let f = BufReader::new(f);
                for (n, sl) in f.lines().enumerate() {
                    if n != line - 1 {
                        continue;
                    }
                    if let Err(e) = sl {
                        error!("{proj}: Cannot read line: {e}");
                        break;
                    }
                    let sl = sl.unwrap();
                    let re = Regex::new(r"[A-Za-z_][A-Za-z0-9_]*").unwrap();
                    let m = if let Some(x) = re.find_at(sl.as_str(), col - 1) {
                        if x.range().start != col - 1 {
                            1
                        } else {
                            x.range().len()
                        }
                    } else {
                        1
                    };
                    let ln = line.to_string().len();
                    let lns = " ".repeat(ln);
                    let _l = "─".repeat(ln);
                    let _r = "─".repeat(sl.len()+2);
                    let mut code = format!(
                        "─{_l}─┬{_r}\n {lns} │ {}:{line}:{col}\n─{_l}─┼{_r}\n {line} │ {sl}\n {lns} │ {}{}",
                        scr.display(),
                        " ".repeat(col - 1),
                        "─".repeat(m)
                    );
                    if !rhai_fn.is_empty() {
                        code += &*format!("\n {lns} └─═ When invoking: {rhai_fn}()");
                    }
                    if !fn_src.is_empty() {
                        code += &*format!("\n {lns} └─═ Function source: {fn_src}");
                    }
                    code += &*format!("\n {lns} └─═ {}", _gemsg(&nanitozo));
                    let c = code.matches('└').count();
                    if c > 0 {
                        code = code.replacen('└', "├", c - 1);
                    }
                    error!("Script Exception —— {proj}\n{code}");
                    return;
                }
            }
            Err(e) => error!("{proj}: Cannot open `{}`: {e}", scr.display()),
        }
    } else {
        let err = _gemsg(&nanitozo);
        error!("{proj}: {} (no position data)\n{err}", scr.display());
    }
}

pub fn traceback(name: &str, scr: &PathBuf, err: EvalAltResult) {
    trace!("{name}: Generating traceback");
    let pos = err.position();
    if let EvalAltResult::ErrorRuntime(ref run_err, pos) = err {
        if let Some(AndaxError::RustReport(rhai_fn, fn_src, oerr)) =
            run_err.clone().try_cast::<AndaxError>()
        {
            _tb(
                name,
                scr,
                TbErr::Report(oerr),
                pos,
                rhai_fn.as_str(),
                fn_src.as_str(),
            );
            return;
        }
        if let Some(AndaxError::RustError(rhai_fn, fn_src, oerr)) =
            run_err.clone().try_cast::<AndaxError>()
        {
            _tb(
                name,
                scr,
                TbErr::Arb(oerr),
                pos,
                rhai_fn.as_str(),
                fn_src.as_str(),
            );
            return;
        }
    }
    _tb(name, scr, TbErr::Rhai(err), pos, "", "");
}

pub fn run<'a>(
    name: &'a str,
    scr: &'a PathBuf,
    f: impl FnOnce(&mut Scope<'a>),
) -> Option<Scope<'a>> {
    let (en, mut sc) = gen_en();
    f(&mut sc);
    debug!("Running {name}");
    match en.run_file_with_scope(&mut sc, scr.clone()) {
        Ok(()) => Some(sc),
        Err(err) => {
            traceback(name, scr, *err);
            None
        }
    }
}
