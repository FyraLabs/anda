use anda_config::Manifest;
use andax::{run, RPMSpec};
use color_eyre::Result;
use std::io::Write;
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
    let project_count = cfg.project.len();
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
            let alias = proj.alias.into_iter().flatten().next().clone().unwrap_or(name);
            handlers.push(Builder::new().name(alias).spawn(move || {
                let th = thread::current();
                let name = th.name().expect("No name for andax thread??");
                let scr = proj.update.expect("No update script? How did I get here??");
                let start = std::time::SystemTime::now();
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
                let duration = std::time::SystemTime::now().duration_since(start).unwrap();
                if let Some(sc) = sc {
                    let rpm: RPMSpec = sc.get_value("rpm").expect("No rpm object in rhai scope");
                    if let Err(e) = rpm.write() {
                        error!("{name}: Failed to write RPM: {e}");
                    }
                }
                duration
            })?);
        }
    }

    debug!("Joining {} threads", handlers.len());
    let mut tasks = vec![];
    let handlers_count = handlers.len();

    for hdl in handlers {
        let th = hdl.thread();
        let name = th.name().expect("No name for andax thread??").to_string();
        match hdl.join() {
            Ok(duration) => tasks.push((name, duration)),
            Err(e) => error!("Panic @ `{name}` : {e:#?}"),
        }
    }

    tasks.sort_unstable_by(|(_, duration0), (_, duration1)| duration1.cmp(duration0));
    let mut stdout = std::io::stdout();

    writeln!(
        stdout,
        "\nFinished running {}/{project_count} tasks, {} failed fatally.",
        tasks.len(),
        handlers_count - tasks.len()
    )
    .unwrap();
    writeln!(stdout, "Here is a list of unfiltered tasks:\n").unwrap();
    writeln!(stdout, "      Time (ms)   Project").unwrap();
    writeln!(stdout, "No.   ══════════╤═════════").unwrap();

    for (n, (name, duration)) in tasks.into_iter().enumerate() {
        writeln!(stdout, "{:<5} {:>9} │ {name}", n + 1, duration.as_millis()).unwrap();
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
