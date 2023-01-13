use anda_config::Manifest;
use andax::{run, update::rpm::RPMSpec};
use color_eyre::Result;
use std::{collections::BTreeMap, thread};
use tracing::{debug, error, instrument, trace};

#[instrument]
pub fn update_rpms(cfg: Manifest, lbls: BTreeMap<String, String>) -> Result<()> {
    let mut handlers = vec![];
    for (name, proj) in cfg.project.iter() {
        if let Some(rpm) = &proj.rpm {
            let spec = &rpm.spec;
            let scr = if let Some(scr) = &rpm.update {
                scr.to_owned()
            } else {
                continue;
            };
            trace!(name, scr = scr.to_str(), "Th start");
            let rpmspec = RPMSpec::new(name.to_owned(), &scr, spec)?;
            let lbls = lbls.clone();
            handlers.push(thread::Builder::new().name(name.clone()).spawn(move || {
                let th = thread::current();
                let name = th.name().expect("No name for andax thread??");
                let sc = run(name, &scr, lbls, |sc| {
                    sc.push("rpm", rpmspec);
                });
                if let Some(sc) = sc {
                    let rpm = sc
                        .get_value::<RPMSpec>("rpm")
                        .expect("No rpm object in rhai scope");
                    if rpm.changed {
                        if let Err(e) = rpm.write() {
                            error!("{name}: Failed to write RPM: {e}");
                        }
                    }
                }
            })?);
        }
    }

    debug!("Joining {} threads", handlers.len());

    for hdl in handlers {
        let th = hdl.thread();
        let name = th.name().expect("No name for andax thread??").to_string();
        if let Err(e) = hdl.join() {
            error!("Panic @ `{name}` : {e:#?}");
        }
    }

    Ok(())
}

#[instrument]
pub fn run_scripts(scripts: &[String], labels: BTreeMap<String, String>) -> Result<()> {
    let mut handlers = vec![];
    for scr in scripts {
        trace!(scr, "Th start");
        let lbls = labels.clone();
        handlers.push(
            thread::Builder::new()
                .name(scr.to_string())
                .spawn(move || {
                    let th = thread::current();
                    let name = th.name().expect("No name for andax thread??");
                    run(name, &std::path::PathBuf::from(name), lbls, |_| {});
                })?,
        );
    }

    debug!("Joining {} threads", handlers.len());

    for hdl in handlers {
        let th = hdl.thread();
        let name = th.name().expect("No name for andax thread??").to_string();
        if let Err(e) = hdl.join() {
            error!("Panic @ `{name}` : {e:#?}");
        }
    }

    Ok(())
}
