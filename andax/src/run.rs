use crate::{
    error::AndaxError,
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
pub(crate) fn ehdl<A, B>(
    ctx: &CallCtx,
    o: Result<A, impl std::error::Error + 'static>,
) -> Result<A, Box<EvalAltResult>> {
    o.map_err(|err| {
        Box::new(EvalAltResult::ErrorRuntime(
            Dynamic::from(AndaxError::RustError(
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
        .register_fn("find", |ctx: CallCtx, a, b, c| rf(ctx, re::find(a, b, c)))
        .register_fn("sub", |ctx: CallCtx, a, b, c| rf(ctx, re::sub(a, b, c)))
        .register_global_module(exported_module!(io::anda_rhai).into())
        .register_global_module(exported_module!(update::tsunagu::anda_rhai).into())
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
    arb: Option<Rc<dyn std::error::Error>>
) {
    let line = pos.line();
    let col = pos.position().unwrap_or(0);
    // let stdout = stdout();
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
                    let re = Regex::new(r"[\w_][\w_\d]+?").unwrap();
                    let m = re.find_at(sl.as_str(), col + 1);
                    let m = if let Some(x) = m {
                        x.range().len()
                    } else {
                        1
                    };
                    // let lock = stdout.lock();
                    let lns = " ".repeat(line.to_string().len());
                    let src = if fn_src.is_empty() {
                        "".to_string()
                    } else {
                        format!(" {lns} = Function source: {fn_src}")
                    };
                    let soerr = if let Some(oerr) = oerr {
                        format!(" {lns} = From this error: {oerr}")
                    } else if let Some(oerr) = arb {
                        format!(" {lns} = From this error: {oerr}")
                    } else {
                        "".to_string()
                    };
                    let func = if !rhai_fn.is_empty() {
                        format!("{rhai_fn}()")
                    } else {
                        "unknown".into()
                    };
                    let code = format!(
                        " {lns} |\n {line} | {sl}\n {lns} | {}{}\n {lns} + When invoking: {func}\n{src}\n{soerr}",
                        " ".repeat(col - 1),
                        "^".repeat(m)
                    );
                    warn!(
                        proj,
                        script = format!("{}:{line}:{col}", scr.display()),
                        func,
                        "{err}\n{code}"
                    );
                    return;
                }
            }
            Err(e) => error!("{proj}: Cannot open `{}`: {e}", scr.display()),
        }
    } else {
        warn!("{proj}: {} (no position data)\n{err}", scr.display());
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

#[cfg(test)]
mod tests {
    use super::*;
    use color_eyre::{Report, Result};

    fn run_update(rpmspec: rpm::RPMSpec) -> Result<()> {
        // FIXME can we avoid clone()
        let name = rpmspec.name.clone();
        let scr = rpmspec.chkupdate.clone();
        let (en, mut sc) = gen_en();
        sc.push("rpm", rpmspec);

        match en.run_file_with_scope(&mut sc, scr) {
            Ok(()) => {
                let rpm = sc
                    .get_value::<rpm::RPMSpec>("rpm")
                    .expect("No rpm object in rhai scope");
                if rpm.changed {
                    rpm.write()?;
                }
                Ok(())
            }
            Err(err) => {
                let e = *err;
                warn!("Fail {}:\n{e}", name);
                Err(Report::msg(e.to_string()))
            }
        }
    }

    #[test]
    fn run_rhai() -> Result<()> {
        run_update(rpm::RPMSpec::new(
            "umpkg".into(),
            "tests/test.rhai",
            "tests/umpkg.spec",
        )?)?;
        Ok(())
    }
}
