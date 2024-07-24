use anda_config::Manifest;
use andax::{run, RPMSpec};
use color_eyre::{Result, Section};
use itertools::Itertools;
use std::io::Write;
use std::{
    collections::BTreeMap,
    thread::{self, Builder},
};
use tracing::{debug, error, instrument, trace};

/// Return true only if the project `lbls` does not have the key or the value does not match.
fn filter_project(lbls: &BTreeMap<String, String>) -> impl FnMut((&String, &String)) -> bool + '_ {
    |(k, v)| lbls.get(k).map_or(true, |val| val != v)
}

#[allow(clippy::arithmetic_side_effects)]
#[instrument(skip(cfg))]
pub fn update(
    cfg: Manifest,
    global_lbls: BTreeMap<String, String>,
    fls: BTreeMap<String, String>,
) -> Result<()> {
    let mut handlers = vec![];
    let proj_len = cfg.project.len();
    let mut scr_len = 0;
    for (name, mut proj) in cfg.project {
        let Some(scr) = proj.update else { continue };
        scr_len += 1;
        let mut lbls = std::mem::take(&mut proj.labels);
        lbls.extend(global_lbls.clone());
        if fls.iter().any(filter_project(&lbls)) {
            continue;
        }
        trace!(name, scr = scr.to_str(), "Th start");
        let fls = fls.clone();
        let alias = proj.alias.into_iter().flatten().next().clone().unwrap_or(name);
        handlers.push(Builder::new().name(alias).spawn(move || {
            let th = thread::current();
            let name = th.name().expect("No name for andax thread??");
            let start = std::time::Instant::now();
            let sc = run(name, &scr, lbls, |sc| {
                // we have to do it here as `Dynamic` in andax::Map nu Sync impl
                let filters =
                    fls.into_iter().map(|(k, v)| (k.into(), v.into())).collect::<andax::Map>();
                sc.push("filters", filters);
                if let Some(rpm) = &proj.rpm {
                    sc.push("rpm", RPMSpec::new(name.to_owned(), &scr, &rpm.spec));
                }
            });
            let duration = start.elapsed().as_millis();
            if let Some(sc) = sc {
                let rpm: RPMSpec = sc.get_value("rpm").expect("No rpm object in rhai scope");
                if let Err(e) = rpm.write() {
                    error!("{name}: Failed to write RPM: {e}");
                }
            }
            duration
        })?);
    }

    let hdl_len = handlers.len();
    if hdl_len == 0 {
        tracing::info!("No tasks were run.");
        return Ok(());
    }
    debug!("Joining {hdl_len} threads");
    let mut panicked = Vec::with_capacity(0);

    let tasks = handlers
        .into_iter()
        .filter_map(|hdl| {
            let th = hdl.thread();
            let name = th.name().expect("No name for andax thread??").to_owned();
            hdl.join()
                .inspect_err(|_| error!("Thread `{name}` panicked. This is most likely a bug."))
                .inspect_err(|_| panicked.push(name.clone()))
                .ok()
                .map(|duration| (name, duration))
        })
        .sorted_unstable_by(|(_, duration0), (_, duration1)| duration1.cmp(duration0));
    let task_len = tasks.len();
    let pname_len = tasks
        .clone()
        .max_by(|(name0, _), (name1, _)| name0.len().cmp(&name1.len()))
        .map_or(13, |(name, _)| name.len());
    let mut stdout = std::io::stdout();

    writeln!(
        stdout,
        "\nFinished running {task_len}/{scr_len} scripts out of {proj_len} projects, {} failed fatally.",
        hdl_len - task_len
    )
    .unwrap();
    writeln!(stdout, "Here is a list of unfiltered tasks:\n").unwrap();
    writeln!(stdout, "No.    Time/ms Project/alias").unwrap();
    writeln!(stdout, "═════╤════════╤═{}", "═".repeat(pname_len.max(13))).unwrap();

    for (n, (name, duration)) in tasks.enumerate() {
        let sep = if n % 2 == 0 { '┃' } else { '│' };
        writeln!(stdout, "{:<5}{sep}{:>7} {sep} {name}", n + 1, duration).unwrap();
    }

    if !panicked.is_empty() {
        return Err(panicked.into_iter().fold(
            color_eyre::Report::msg("One of the threads panicked while running the update script")
                .suggestion("Bug report: https://github.com/FyraLabs/anda/issues"),
            |err, name| err.warning(format!("Project/alias: {name}")),
        ));
    }

    Ok(())
}

#[instrument]
pub fn run_scripts(scripts: &[String], labels: BTreeMap<String, String>) -> Result<()> {
    let mut handlers = vec![];
    for scr in scripts {
        trace!(scr, "Th start");
        let labels = labels.clone();
        handlers.push(Builder::new().name(scr.to_owned()).spawn(move || {
            let th = thread::current();
            let name = th.name().expect("No name for andax thread??");
            run(name, &std::path::PathBuf::from(name), labels, |_| {});
        })?);
    }

    debug!("Joining {} threads", handlers.len());

    for hdl in handlers {
        let th = hdl.thread();
        let name = th.name().expect("No name for andax thread??").to_owned();
        if let Err(e) = hdl.join() {
            error!("Panic @ `{name}` : {e:#?}");
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_filter() {
        let transform = |arr: &[(&str, &str)]| {
            arr.iter()
                .map(|(l, r)| ((*l).to_owned(), (*r).to_owned()))
                .collect::<BTreeMap<String, String>>()
        };
        // update only nightly packages
        let lbls = std::iter::once(("nightly", "1")).map(|(l, r)| (l.into(), r.into())).collect();
        let mut test1 = filter_project(&lbls);
        for (k, v) in transform(&[("nightly", "0"), ("hai", "bai"), ("large", "1")]) {
            assert!(test1((&k, &v)));
        }
        for (k, v) in transform(&[("nightly", "1")]) {
            assert!(!test1((&k, &v)));
        }
    }
}
