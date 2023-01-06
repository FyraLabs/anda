use crate::io;
use crate::update::tsunagu::ehdl;
use crate::update::{self, re, rpm, tsunagu};
use anda_config::Manifest;
use anyhow::Result;
use log::{debug, error, warn};
use regex::Regex;
use rhai::plugin::*;
use rhai::{Engine, EvalAltResult, Map, NativeCallContext, Scope};
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::thread;

pub(crate) fn json(ctx: NativeCallContext, a: String) -> Result<Map, Box<EvalAltResult>> {
    ctx.engine().parse_json(a, true)
}

fn gen_en() -> (Engine, Scope<'static>) {
    let mut sc = Scope::new();
    sc.push("USER_AGENT", tsunagu::USER_AGENT);
    sc.push("IS_WIN32", cfg!(windows));
    let mut en = Engine::new();
    en.register_fn("json", json)
        .register_fn("find", |a: &str, b: &str, c: i64| ehdl(re::find(a, b, c)))
        .register_fn("sub", |a: &str, b: &str, c: &str| ehdl(re::sub(a, b, c)))
        .register_global_module(exported_module!(io::anda_rhai).into())
        .register_global_module(exported_module!(update::tsunagu::anda_rhai).into())
        .build_type::<rpm::RPMSpec>();
    (en, sc)
}

pub fn traceback(name: &String, scr: &PathBuf, err: EvalAltResult) {
    let pos = err.position();
    let line = pos.line();
    let col = pos.position().unwrap_or(0);
    if let Some(line) = line {
        // Print code
        warn!("{name}: {}:{line}:{col}", scr.display());
        match File::open(scr) {
            Ok(f) => {
                let f = BufReader::new(f);
                for (n, sl) in f.lines().enumerate() {
                    if n != line {
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
                    warn!(" {line} | {sl}");
                    warn!(
                        " {} | {}{}",
                        " ".repeat(line.to_string().len()),
                        " ".repeat(col - 1),
                        "^".repeat(m.range().len())
                    );
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

pub fn run<'a, F>(name: &'a String, scr: &'a PathBuf, f: F) -> Option<Scope<'a>>
where
    F: FnOnce(&mut Scope<'a>),
{
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

pub fn update_rpms(cfg: Manifest) -> Result<()> {
    let mut handlers = vec![];
    for (name, proj) in cfg.project.iter() {
        if let Some(rpm) = &proj.rpm {
            let spec = &rpm.spec;
            if rpm.update.is_none() {
                continue;
            }
            let scr = rpm.update.to_owned().unwrap();
            let rpmspec = rpm::RPMSpec::new(name.clone(), &scr, spec)?;
            let name = name.to_owned();
            handlers.push(thread::spawn(move || {
                let sc = run(&name, &scr, |sc| { sc.push("rpm", rpmspec); });
                if let Some(sc) = sc {
                    let rpm = sc
                        .get_value::<rpm::RPMSpec>("rpm")
                        .expect("No rpm object in rhai scope");
                    if rpm.changed {
                        if let Err(e) = rpm.write() {
                            error!("{name}: Failed to write RPM:");
                            error!("{name}: {e}");
                        }
                    }
                }
            }));
        }
    }

    for hdl in handlers {
        if let Err(e) = hdl.join() {
            error!("Cannot join thread: {e:?}");
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::anyhow;

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
