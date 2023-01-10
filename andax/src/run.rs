use crate::{
    error::{AndaxError, AndaxRes},
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
                ctx.fn_name().to_string(),
                ctx.source().unwrap_or("").to_string(),
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

#[instrument(name = "traceback")]
pub fn _tb(
    proj: &String,
    scr: &PathBuf,
    err: EvalAltResult,
    pos: Position,
    rhai_fn: &str,
    fn_src: &str,
    oerr: Option<Rc<color_eyre::Report>>,
    arb: Option<Rc<dyn std::error::Error>>,
) {
    let line = pos.line();
    let col = pos.position().unwrap_or(0);
    if let Some(line) = line {
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
                    let lns = " ".repeat(line.to_string().len());
                    let mut code = format!(
                        " {lns} ┌{}\n {line} │ {sl}\n {lns} │ {}{}",
                        "─".repeat(col),
                        " ".repeat(col - 1),
                        "─".repeat(m)
                    );
                    if !rhai_fn.is_empty() {
                        code += &*format!("\n {lns} └─═ When invoking: {rhai_fn}()");
                    }
                    if !fn_src.is_empty() {
                        code += &*format!("\n {lns} └─═ Function source: {fn_src}");
                    }
                    if let Some(o) = oerr {
                        code += &*format!("\n {lns} └─═ From: {o}");
                    }
                    if let Some(o) = arb {
                        code += &*format!("\n {lns} └─═ From: {o}");
                    }
                    let c = code.matches('└').count();
                    if c > 0 {
                        code = code.replacen('└', "├", c - 1);
                    }
                    error!(
                        proj,
                        script = format!("{}:{line}:{col}", scr.display()),
                        "{err}\n{code}"
                    );
                    return;
                }
            }
            Err(e) => error!("{proj}: Cannot open `{}`: {e}", scr.display()),
        }
    } else {
        error!("{proj}: {} (no position data)\n{err}", scr.display());
    }
}

pub fn traceback(name: &String, scr: &PathBuf, err: EvalAltResult) {
    trace!("{name}: Generating traceback");
    let pos = err.position();
    if let EvalAltResult::ErrorRuntime(ref run_err, pos) = err {
        if let Some(AndaxError::RustReport(rhai_fn, fn_src, oerr)) =
            run_err.clone().try_cast::<AndaxError>()
        {
            _tb(
                name,
                scr,
                err,
                pos,
                rhai_fn.as_str(),
                fn_src.as_str(),
                Some(oerr),
                None,
            );
            return;
        }
        if let Some(AndaxError::RustError(rhai_fn, fn_src, oerr)) =
            run_err.clone().try_cast::<AndaxError>()
        {
            _tb(
                name,
                scr,
                err,
                pos,
                rhai_fn.as_str(),
                fn_src.as_str(),
                None,
                Some(oerr),
            );
            return;
        }
    }
    _tb(name, scr, err, pos, "", "", None, None);
}

pub fn run<'a>(
    name: &'a String,
    scr: &'a PathBuf,
    f: impl FnOnce(&mut Scope<'a>),
) -> Option<Scope<'a>> {
    let (en, mut sc) = gen_en();
    f(&mut sc);
    debug!("Running {name}");
    match en.run_file_with_scope(&mut sc, scr.clone()) {
        Ok(()) => Some(sc.to_owned()),
        Err(err) => {
            traceback(name, scr, *err);
            None
        }
    }
}
