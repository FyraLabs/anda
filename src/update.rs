use anda_config::Manifest;
use andax::{run, RPMSpec};
use color_eyre::Result;
use std::{
    collections::BTreeMap,
    thread::{self, Builder},
};
use tracing::{debug, error, instrument, trace};

#[instrument(skip(cfg))]
pub fn update(
    cfg: Manifest,
    global_lbls: BTreeMap<String, String>,
    fls: BTreeMap<String, String>,
) -> Result<()> {
    let mut handlers = vec![];
    'p: for (name, mut proj) in cfg.project {
        if let Some(scr) = &proj.update {
            trace!(name, scr = scr.to_str(), "Th start");
            let mut lbls = std::mem::take(&mut proj.labels);
            lbls.extend(global_lbls.clone());
            for (k, v) in &fls {
                if let Some(val) = lbls.get(k) {
                    if val == v {
                        continue;
                    }
                }
                continue 'p; // for any filters !match labels in proj (strict)
            }
            let fls = fls.clone();
            handlers.push(Builder::new().name(name).spawn(move || {
                let th = thread::current();
                let name = th.name().expect("No name for andax thread??");
                let scr = proj.update.expect("No update script? How did I get here??");
                let sc = run(name, &scr, lbls, |sc| {
                    // we have to do it here as `Dynamic` in andax::Map nu Sync impl
                    let mut filters = andax::Map::new();
                    for (k, v) in fls {
                        filters.insert(k.into(), v.into());
                    }
                    sc.push("filters", filters);
                    if let Some(rpm) = &proj.rpm {
                        sc.push("rpm", RPMSpec::new(name.to_string(), &scr, &rpm.spec));
                    }
                });
                if let Some(sc) = sc {
                    let rpm: RPMSpec = sc.get_value("rpm").expect("No rpm object in rhai scope");
                    if let Err(e) = rpm.write() {
                        error!("{name}: Failed to write RPM: {e}");
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
        let labels = labels.clone();
        handlers.push(Builder::new().name(scr.to_string()).spawn(move || {
            let th = thread::current();
            let name = th.name().expect("No name for andax thread??");
            run(name, &std::path::PathBuf::from(name), labels, |_| {});
        })?);
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
