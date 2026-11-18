use crate::update_report::{
    failure_outcome, panic_message, ErrorKind, ErrorReport, Event, ProjectReport, Report, Status,
};
use anda_config::Manifest;
use andax::{run, try_run, RPMSpec};
use color_eyre::eyre::eyre;
use color_eyre::{Result, Section};
use itertools::Itertools;
use std::io::Write;
use std::{
    collections::BTreeMap,
    path::Path,
    thread::{self, Builder},
};
use tracing::{debug, error, instrument, trace};

/// Return true only if the project `lbls` does not have the key or the value does not match.
fn filter_project(lbls: &BTreeMap<String, String>) -> impl Fn(&(String, String)) -> bool + '_ {
    |(k, v)| lbls.get(k) != Some(v)
}

/// Return true only if `lbls` have the key and the value matches.
fn exclude_project(lbls: &BTreeMap<String, String>) -> impl Fn(&(String, String)) -> bool + '_ {
    |(k, v)| lbls.get(k) == Some(v)
}

struct Outcome {
    status: Status,
    spec_changed: bool,
    error: Option<ErrorReport>,
    events: Vec<Event>,
}

fn run_update_script<K, V>(
    name: &str,
    scr: &Path,
    spec: Option<&Path>,
    lbls: impl Iterator<Item = (K, V)>,
    fls: Vec<Vec<(String, String)>>,
) -> Outcome
where
    K: Into<andax::rhai::Identifier>,
    V: Into<andax::rhai::Dynamic>,
{
    let result = try_run(name, scr, lbls, |sc| {
        // we have to do it here as `Dynamic` in andax::Map nu Sync impl
        let filters = fls
            .into_iter()
            .flat_map(|fls| fls.into_iter().map(|(k, v)| (k.into(), v.into())))
            .collect::<andax::Map>();
        sc.push("filters", filters);
        if let Some(spec) = spec {
            sc.push("rpm", RPMSpec::new(name.to_owned(), scr, spec));
        }
    });
    let sc = match result {
        Ok(sc) => sc,
        Err(failure) => {
            let (status, error) = failure_outcome(failure);
            return Outcome { status, spec_changed: false, error, events: vec![] };
        }
    };
    let Some(mut rpm) = sc.get_value::<RPMSpec>("rpm") else {
        return Outcome { status: Status::Ok, spec_changed: false, error: None, events: vec![] };
    };
    let events = std::mem::take(&mut rpm.submodule_changes).into_iter().map(Event::from).collect();
    let spec_changed = rpm.changed();
    match rpm.write() {
        Ok(()) => Outcome { status: Status::Ok, spec_changed, error: None, events },
        Err(e) => {
            error!("{name}: Failed to write RPM: {e}");
            Outcome {
                status: Status::Failed,
                spec_changed: false,
                error: Some(ErrorReport::plain(ErrorKind::Write, e.to_string())),
                events,
            }
        }
    }
}

#[instrument(skip(cfg))]
pub fn single_update(
    cfg: &Manifest,
    proj: &str,
    lbls: &[(String, String)],
    fls: Vec<Vec<(String, String)>>,
    report: Option<&Path>,
) -> Result<()> {
    let Some((key, project)) = cfg.get_project_entry(proj) else {
        return Err(eyre!("Project not found: {proj}"));
    };
    let Some(scr) = &project.update else {
        return Err(eyre!("no update field in project"));
    };
    let spec = project.rpm.as_ref().map(|rpm| rpm.spec.as_path());
    let lbls = project.labels.iter().chain(lbls.iter().map(|(k, v)| (k, v)));
    let start = std::time::Instant::now();
    let outcome = run_update_script(proj, scr, spec, lbls, fls);
    if let Some(path) = report {
        Report::new(vec![ProjectReport {
            project: key.clone(),
            alias: project.alias.iter().flatten().next().cloned(),
            script: scr.clone(),
            spec: spec.map(Path::to_path_buf),
            status: outcome.status,
            spec_changed: outcome.spec_changed,
            duration_ms: Some(start.elapsed().as_millis()),
            error: outcome.error,
            events: outcome.events,
        }])
        .write_to(path)?;
    }
    Ok(())
}

#[allow(clippy::arithmetic_side_effects)]
#[instrument(skip(cfg))]
pub fn update(
    cfg: Manifest,
    global_lbls: &[(String, String)],
    fls: &[Vec<(String, String)>],
    excls: &[Vec<(String, String)>],
    report: Option<&Path>,
) -> Result<()> {
    let mut handlers = vec![];
    let proj_len = cfg.project.len();
    let mut scr_len = 0;
    for (name, mut proj) in cfg.project {
        let Some(scr) = proj.update else { continue };
        scr_len += 1;
        let mut lbls = std::mem::take(&mut proj.labels);
        lbls.extend(global_lbls.to_owned());
        if !fls.is_empty() && fls.iter().all(|fls| fls.iter().any(filter_project(&lbls))) {
            continue;
        }
        if excls.iter().any(|excls| excls.iter().all(exclude_project(&lbls))) {
            continue;
        }
        trace!(name, scr = scr.to_str(), "Th start");
        let fls = fls.to_owned();
        let alias = proj.alias.into_iter().flatten().next();
        let pending = ProjectReport {
            project: name.clone(),
            alias: alias.clone(),
            script: scr.clone(),
            spec: proj.rpm.map(|rpm| rpm.spec),
            status: Status::Panicked,
            spec_changed: false,
            duration_ms: None,
            error: None,
            events: vec![],
        };
        let spec = pending.spec.clone();
        let handle = Builder::new().name(alias.unwrap_or(name)).spawn(move || {
            let th = thread::current();
            let name = th.name().expect("No name for andax thread??");
            let start = std::time::Instant::now();
            let outcome = run_update_script(name, &scr, spec.as_deref(), lbls.iter(), fls);
            (start.elapsed().as_millis(), outcome)
        })?;
        handlers.push((handle, pending));
    }

    let hdl_len = handlers.len();
    if hdl_len == 0 {
        tracing::info!("No tasks were run.");
        return Ok(());
    }
    debug!("Joining {hdl_len} threads");
    let mut panicked = Vec::new();

    let mut reports = Vec::with_capacity(hdl_len);

    let tasks = handlers
        .into_iter()
        .filter_map(|(hdl, mut pending)| {
            let name = hdl.thread().name().expect("No name for andax thread??").to_owned();
            match hdl.join() {
                Ok((duration, outcome)) => {
                    pending.status = outcome.status;
                    pending.spec_changed = outcome.spec_changed;
                    pending.duration_ms = Some(duration);
                    pending.error = outcome.error;
                    pending.events = outcome.events;
                    reports.push(pending);
                    Some((name, duration))
                }
                Err(payload) => {
                    error!("Thread `{name}` panicked. This is most likely a bug.");
                    pending.error =
                        Some(ErrorReport::plain(ErrorKind::Panic, panic_message(payload.as_ref())));
                    reports.push(pending);
                    panicked.push(name);
                    None
                }
            }
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
    )?;
    writeln!(stdout, "Here is a list of unfiltered tasks:\n")?;
    writeln!(stdout, "No.    Time/ms Project/alias")?;
    writeln!(stdout, "═════╤════════╤═{}", "═".repeat(pname_len.max(13)))?;

    for (n, (name, duration)) in tasks.enumerate() {
        let sep = if n % 2 == 0 { '┃' } else { '│' };
        writeln!(stdout, "{:<5}{sep}{:>7} {sep} {name}", n + 1, duration)?;
    }

    if let Some(path) = report {
        Report::new(reports).write_to(path)?;
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
pub fn run_scripts(scripts: &[String], labels: &[(String, String)]) -> Result<()> {
    let mut handlers = vec![];
    for scr in scripts {
        trace!(scr, "Th start");
        let labels = labels.to_owned();
        handlers.push(Builder::new().name(scr.to_owned()).spawn(move || {
            let th = thread::current();
            let name = th.name().expect("No name for andax thread??");
            run(name, &std::path::PathBuf::from(name), labels.into_iter(), |_| {});
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
        let test1 = filter_project(&lbls);
        for (k, v) in transform(&[("nightly", "0"), ("hai", "bai"), ("large", "1")]) {
            assert!(test1(&(k, v)));
        }
        for (k, v) in transform(&[("nightly", "1")]) {
            assert!(!test1(&(k, v)));
        }
    }
}
