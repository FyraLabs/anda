use crate::io;
use crate::update::tsunagu::ehdl;
use crate::update::{self, re, rpm, tsunagu};
use anda_config::Manifest;
use anyhow::Result;
use log::{debug, error, warn};
use rhai::plugin::*;
use rhai::{Engine, EvalAltResult, Map, NativeCallContext, Scope};
use std::path::PathBuf;
use std::thread;

pub(crate) fn json(ctx: NativeCallContext, a: String) -> Result<Map, Box<EvalAltResult>> {
    ctx.engine().parse_json(a, true)
}

fn gen_en(rpmspec: rpm::RPMSpec) -> (Engine, Scope<'static>) {
    let mut sc = Scope::new();
    sc.push("rpm", rpmspec);
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

pub fn update_pkgs(cfg: Manifest) -> Result<()> {
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
            handlers.push(thread::spawn(move || -> std::io::Result<()> {
                debug!("Running {name}");
                let (en, mut sc) = gen_en(rpmspec);
                match en.run_file_with_scope(&mut sc, PathBuf::from(&scr)) {
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
                        warn!("Fail {name}:\n{e}");
                        Ok(())
                    }
                }
            }));
        }
    }

    // FIXME put me back into the threads!
    let mut errors = vec![];
    for hdl in handlers {
        if let Err(e) = hdl.join() {
            errors.push(e);
        }
    }
    if !errors.is_empty() {
        error!("During andax: Error(s) from rpm.write():");
        for e in errors {
            error!("{:?}", e);
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
        let (en, mut sc) = gen_en(rpmspec);

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
