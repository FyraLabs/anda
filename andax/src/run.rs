use crate::{
    error::AndaxError,
    io,
    update::{self, re, rpm, tsunagu},
};
use log::{debug, error, trace, warn};
use regex::Regex;
use rhai::{plugin::*, Engine, EvalAltResult, NativeCallContext as CallCtx, Scope};
use std::{
    fs::File,
    io::{BufRead, BufReader},
    path::PathBuf,
    rc::Rc,
};

fn json(ctx: CallCtx, a: String) -> Result<rhai::Map, Box<EvalAltResult>> {
    ctx.engine().parse_json(a, true)
}


pub(crate) fn rf<T>(ctx: CallCtx, res: anyhow::Result<T>) -> Result<T, Box<EvalAltResult>>
where
    T: rhai::Variant + Clone,
{
    res.map_err(|err| {
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
        .register_fn("find", |ctx, a, b, c| rf(ctx, re::find(a, b, c)))
        .register_fn("sub", |ctx, a, b, c| rf(ctx, re::sub(a, b, c)))
        .register_global_module(exported_module!(io::anda_rhai).into())
        .register_global_module(exported_module!(update::tsunagu::anda_rhai).into())
        .build_type::<rpm::RPMSpec>();
    (en, sc)
}

pub fn _tb(
    name: &String,
    scr: &PathBuf,
    err: EvalAltResult,
    pos: Position,
    rhai_fn: &str,
    fn_src: &str,
    oerr: Option<Rc<anyhow::Error>>,
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
                        error!("{name}: Cannot read line: {e}");
                        break;
                    }
                    let sl = sl.unwrap();
                    let re = Regex::new(r"\b.+?\b").unwrap();
                    let m = re
                        .find_at(sl.as_str(), col + 1)
                        .expect("Can't match code with regex");
                    // let lock = stdout.lock();
                    warn!(
                        "{name}: {}:{line}:{col} {}",
                        scr.display(),
                        if !rhai_fn.is_empty() {
                            format!("at `{rhai_fn}()`")
                        } else {
                            "".into()
                        }
                    );
                    let lns = " ".repeat(line.to_string().len());
                    warn!(" {line} | {sl}");
                    warn!(
                        " {lns} | {}{}",
                        " ".repeat(col - 1),
                        "^".repeat(m.range().len())
                    );
                    if !fn_src.is_empty() {
                        warn!(" {lns} = Function source: {fn_src}");
                    }
                    if let Some(oerr) = oerr {
                        warn!(" {lns} = From this error: {oerr}");
                    }
                    break;
                }
            }
            Err(e) => error!("{name}: Cannot open `{}`: {e}", scr.display()),
        }
    } else {
        warn!("{name}: {} (no position data)", scr.display());
    }
    warn!("{name}: {err}");
}

pub fn traceback(name: &String, scr: &PathBuf, err: EvalAltResult) {
    trace!("{name}: Generating traceback");
    let pos = err.position();
    if let EvalAltResult::ErrorRuntime(ref run_err, pos) = err {
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
                Some(oerr),
            );
            return;
        }
    }
    _tb(name, scr, err, pos, "", "", None);
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
    use anyhow::{anyhow, Result};

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
                Err(anyhow!(e.to_string()))
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
