use anda_config::Manifest;
use andax::{run, update::rpm::RPMSpec};
use color_eyre::Result;
use std::thread;
use tracing::{debug, error, instrument, trace};

#[instrument]
pub fn update_rpms(cfg: Manifest) -> Result<()> {
    let mut handlers = vec![];
    for (name, proj) in cfg.project.iter() {
        if let Some(rpm) = &proj.rpm {
            let spec = &rpm.spec;
            let scr = if rpm.update.is_none() {
                let s = rpm.spec.display().to_string();
                let paths: Vec<&str> = s.split('/').collect();
                let path = &paths[..paths.len() - 1].join("/");
                let scr = format!("{path}/update.rhai");
                if !std::path::Path::new(&scr).exists() {
                    continue;
                }
                debug!("Found {scr}");
                std::path::PathBuf::from(scr)
            } else {
                rpm.update.to_owned().unwrap()
            };
            let rpmspec = RPMSpec::new(name.clone(), &scr, spec)?;
            let name = name.to_owned();
            trace!(name, scr = scr.display().to_string(), "Th start");
            handlers.push(thread::Builder::new().name(name).spawn(move || {
                let name = thread::current()
                    .name()
                    .expect("No name for andax thread??")
                    .to_string();
                let sc = run(&name, &scr, |sc| {
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
            error!("Panic @ `{name}`");
        }
    }

    Ok(())
}
