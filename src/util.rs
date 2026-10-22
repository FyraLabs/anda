//! Utility functions and types
use anda_config::{Docker, DockerImage, Manifest, Project, RpmBuild};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use clap_verbosity_flag::log::LevelFilter;
use color_eyre::Result;

use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    path::{Path, PathBuf},
    sync::LazyLock,
};
use tokio::process::Command;
use tracing::debug;

static DEFAULT_ARCHES: LazyLock<[String; 2]> =
    LazyLock::new(|| ["x86_64".to_owned(), "aarch64".to_owned()]);

// Build entry for GHA
#[derive(Debug, Clone, Serialize, Deserialize, Ord, Eq, PartialEq, PartialOrd)]
pub struct BuildEntry {
    pub pkg: String,
    pub arch: String,
    pub labels: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BuildUnit {
    pub name: String,
    pub packages: Vec<BuildEntry>,
}

pub fn configure_build_entries(
    entries: Vec<BuildEntry>,
    arches: &[String],
    unique_packages: bool,
) -> Vec<BuildEntry> {
    if arches.is_empty() && !unique_packages {
        return entries;
    }

    let mut by_package = BTreeMap::new();
    for entry in entries {
        by_package.entry(entry.pkg.clone()).or_insert(entry);
    }

    if arches.is_empty() {
        return by_package.into_values().collect();
    }

    by_package
        .into_values()
        .flat_map(|entry| {
            arches.iter().map(move |arch| BuildEntry {
                pkg: entry.pkg.clone(),
                arch: arch.clone(),
                labels: entry.labels.clone(),
            })
        })
        .collect()
}

pub fn group_build_entries(entries: Vec<BuildEntry>) -> Vec<BuildUnit> {
    let mut units: BTreeMap<String, Vec<BuildEntry>> = BTreeMap::new();
    for entry in entries {
        let (kind, value) = entry
            .labels
            .get("group")
            .filter(|group| !group.is_empty())
            .map_or(("package", entry.pkg.as_str()), |group| ("group", group.as_str()));
        let name = format!("{kind}-{}", URL_SAFE_NO_PAD.encode(value.as_bytes()));
        units.entry(name).or_default().push(entry);
    }

    units.into_iter().map(|(name, packages)| BuildUnit { name, packages }).collect()
}

pub fn fetch_build_entries(config: &Manifest, config_path: &Path) -> Vec<BuildEntry> {
    let changed_files = get_changed_files(Path::new(".")).unwrap_or_default();
    build_entries_for_changes(config, &changed_files, config_path)
}

fn build_entries_for_changes(
    config: &Manifest,
    changed_files: &[String],
    config_path: &Path,
) -> Vec<BuildEntry> {
    let suffix = config.config.strip_suffix.clone().unwrap_or_default();
    // A change to the root manifest can change project membership, groups, or architecture
    // settings globally. Nested manifests are mapped to their own packages below.
    let root_config_path = config_path
        .strip_prefix(std::env::current_dir().unwrap_or_default())
        .unwrap_or(config_path)
        .components()
        .collect::<PathBuf>()
        .to_string_lossy()
        .replace('\\', "/");
    let manifest_changed = changed_files.iter().any(|file| file == &root_config_path);

    // Manifest loading recursively merges nested anda.hcl files into this project map. Index
    // package paths and group memberships once, avoiding a full project scan for every changed
    // package. Ordered sets keep the emitted matrix deterministic while deduplicating group members.
    let mut projects_by_dir: HashMap<String, Vec<String>> =
        HashMap::with_capacity(config.project.len());
    let mut projects_by_group: HashMap<String, Vec<String>> =
        HashMap::with_capacity(config.project.len());
    let mut group_by_project: HashMap<String, String> =
        HashMap::with_capacity(config.project.len());

    for (name, project) in &config.project {
        let dir = name.trim_end_matches(&suffix).to_owned();
        projects_by_dir.entry(dir).or_default().push(name.clone());

        if let Some(group) = project.build_group() {
            let group = group.to_owned();
            projects_by_group.entry(group.clone()).or_default().push(name.clone());
            group_by_project.insert(name.clone(), group);
        }
    }

    let mut changed_projects = BTreeSet::new();
    if manifest_changed {
        changed_projects.extend(config.project.keys().cloned());
    } else {
        for file in changed_files {
            let mut dir = String::new();
            for component in file.split('/') {
                if !dir.is_empty() {
                    dir.push('/');
                }
                dir.push_str(component);
                if let Some(projects) = projects_by_dir.get(&dir) {
                    changed_projects.extend(projects.iter().cloned());
                }
            }
        }
    }

    // A change to any package in a build group invalidates all packages in that group. Keep
    // emitting one matrix entry per package/architecture; the CI workflow coordinates publishing.
    let mut included_projects = changed_projects.clone();
    if !manifest_changed {
        let changed_groups: BTreeSet<String> = changed_projects
            .iter()
            .filter_map(|name| group_by_project.get(name).cloned())
            .collect();
        for group in changed_groups {
            if let Some(members) = projects_by_group.get(&group) {
                included_projects.extend(members.iter().cloned());
            }
        }
    }

    let mut entries = Vec::new();
    for name in included_projects {
        let Some(project) = config.project.get(&name) else {
            continue;
        };

        if project.rpm.as_ref().is_some_and(|rpm| rpm.enable_scm.unwrap_or(false)) {
            entries.extend(DEFAULT_ARCHES.iter().map(|arch| BuildEntry {
                pkg: name.clone(),
                arch: arch.clone(),
                labels: project.labels.clone(),
            }));
            continue;
        }

        entries.extend(
            project
                .arches
                .clone()
                .unwrap_or_else(|| DEFAULT_ARCHES.to_vec())
                .into_iter()
                .map(|arch| BuildEntry { pkg: name.clone(), arch, labels: project.labels.clone() }),
        );
    }

    entries
}

/// Command Logging
///
/// This trait implements custom logging for commands in a format of `{command} | {line}`
/// It also implements Ctrl-C handling for the command, and will send a SIGINT to the command
#[async_trait::async_trait]
pub trait CommandLog {
    async fn log(&mut self) -> Result<()>;
}

#[async_trait::async_trait]
impl CommandLog for Command {
    #[allow(clippy::arithmetic_side_effects)]
    // we are force-casting anyway
    // #[allow(clippy::cast_possible_truncation)] // we are force-casting anyway
    #[tracing::instrument]
    async fn log(&mut self) -> Result<()> {
        debug!("running command");
        crate::pt::PseudoTerminalCtl::new(self.as_std().get_program()).run(self).await
    }
}

// utility functions for spec templating

use git2::Repository;
/// Get the current commit id from the current git repository (cwd)
pub fn get_commit_id_cwd() -> Option<String> {
    let repo = Repository::open(".").ok()?;
    let head = repo.head().ok()?;
    let commit = head.peel_to_commit().ok()?;
    let id = commit.id();
    Some(id.to_string())
}

/// Get the current commit id from a git repository
pub fn _get_commit_id(path: &str) -> Option<String> {
    let repo = Repository::open(path).ok()?;
    let head = repo.head().ok()?;
    let commit = head.peel_to_commit().ok()?;
    let id = commit.id();
    Some(id.to_string())
}

// git diff --name-only HEAD^
pub fn get_changed_files(path: &Path) -> Option<Vec<String>> {
    let repo = Repository::open(path).ok()?;
    let head = repo.head().ok()?;
    let commit = head.peel_to_commit().ok()?;
    let parent = commit.parent(0).ok()?;
    let diff = repo
        .diff_tree_to_tree(Some(&parent.tree().ok()?), Some(&commit.tree().ok()?), None)
        .ok()?;
    let mut changed_files = vec![];
    diff.foreach(
        &mut |delta, _| {
            changed_files.push(delta.new_file().path().unwrap().to_str().unwrap().to_owned());
            true
        },
        None,
        None,
        None,
    )
    .ok()?;
    trace!("changed files: {changed_files:?}");
    Some(changed_files)
}

/// Formats the current time in the format of YYYYMMDD
pub fn get_date() -> String {
    let now: chrono::DateTime<chrono::Utc> = chrono::Utc::now();
    now.format("%Y%m%d").to_string()
}

use promptly::prompt_default;
use tracing::trace;

/// Initializes a new anda project
pub fn init(path: &Path, yes: bool) -> Result<()> {
    // create the directory if not exists
    if !path.exists() {
        std::fs::create_dir_all(path)?;
    }

    let mut config = Manifest { project: BTreeMap::new(), config: anda_config::Config::default() };

    // use ignore to scan for files
    let walk = ignore::WalkBuilder::new(path).build();

    for entry in walk {
        let entry = entry?;
        let path = entry.path().strip_prefix("./")?;

        if !path.is_file() {
            continue;
        }

        match path.extension().unwrap_or_default().as_encoded_bytes() {
            b"spec" => {
                debug!("Found spec file: {}", path.display());
                if yes
                    || prompt_default(
                        format!("Add spec file `{}` to manifest?", path.display()),
                        true,
                    )?
                {
                    let project_name = path.file_stem().unwrap().to_str().unwrap();
                    let project = Project {
                        rpm: Some(RpmBuild { spec: path.to_path_buf(), ..Default::default() }),
                        ..Default::default()
                    };
                    config.project.insert(project_name.to_owned(), project);
                }
            }
            b"dockerfile" => add_dockerfile_to_manifest(yes, path, &mut config)?,
            _ if path.file_name().is_some_and(|f| f.eq("Dockerfile")) => {
                add_dockerfile_to_manifest(yes, path, &mut config)?;
            }
            _ => {}
        }
    }
    println!("{}", anda_config::config::to_string(&config)?);

    Ok(())
}

fn add_dockerfile_to_manifest(
    yes: bool,
    path: &Path,
    config: &mut Manifest,
) -> Result<(), color_eyre::eyre::Error> {
    let add_oci =
        yes || prompt_default(format!("Add Dockerfile `{}` to manifest?", path.display()), true)?;
    if add_oci {
        // create a new project called docker

        let mut docker = Docker::default();

        let image =
            DockerImage { dockerfile: Some(path.display().to_string()), ..Default::default() };
        let image_name = "docker-1".to_owned();
        docker.image.insert(image_name, image);

        let project = Project { docker: Some(docker), ..Default::default() };

        // increment counter
        config.project.insert("docker".to_owned(), project);
    }
    Ok(())
}

pub const fn convert_filter(filter: LevelFilter) -> tracing_subscriber::filter::LevelFilter {
    match filter {
        LevelFilter::Off => tracing_subscriber::filter::LevelFilter::OFF,
        LevelFilter::Error => tracing_subscriber::filter::LevelFilter::ERROR,
        LevelFilter::Warn => tracing_subscriber::filter::LevelFilter::WARN,
        LevelFilter::Info => tracing_subscriber::filter::LevelFilter::INFO,
        LevelFilter::Debug => tracing_subscriber::filter::LevelFilter::DEBUG,
        LevelFilter::Trace => tracing_subscriber::filter::LevelFilter::TRACE,
    }
}

#[macro_export]
macro_rules! cmd {
    (@ $cmd:ident [[$expr:expr]]) => { $cmd.args($expr); };
    (@ $cmd:ident $tt:tt) => { $cmd.arg(cmd!(# $tt)); };
    (# [$expr:literal $($arg:expr),*]) => { format!($expr, $($arg),*) };
    (# {{$expr:expr}}) => { format!("{}", $expr) };
    (# $expr:expr) => { &$expr };
    (# $expr:literal) => { $expr };

    (stdout $cmd:literal $($t:tt)+) => {{
        #[allow(unused_braces)]
        let cmd = cmd!($cmd $($t)+).output()?;
        String::from_utf8_lossy(&cmd.stdout)
    }};
    ($cmd:literal $($t:tt)*) => {{
        #[allow(unused_braces)]
        let mut cmd = std::process::Command::new($cmd);
        $(
            cmd!(@ cmd $t);
        )*
        cmd
    }};
    ($cmd:block $($t:tt)*) => {{
        #[allow(unused_braces)]
        let mut cmd = std::process::Command::new(cmd!(# $cmd));
        $(
            cmd!(@ cmd $t);
        )*
        cmd
    }};
    (?$cmd:tt $($t:tt)*) => {{
        #[allow(unused_braces)]
        $crate::util::cmd(cmd!($cmd $($t)*), &[Box::new($cmd), $(Box::new(cmd!(# $t))),*])
    }};
}

/// Run a command and perform logging.
///
/// # Errors
/// This function transform command failures into better error messages.
#[inline]
pub fn cmd<const N: usize>(
    mut cmd: std::process::Command,
    cmd_arr: &[Box<dyn std::fmt::Display>; N],
) -> color_eyre::Result<()> {
    use color_eyre::Help;
    use itertools::Itertools;
    let cmd_str = cmd_arr.iter().join(" ");
    tracing::trace!("Running command: `{cmd_str}`");
    let status = cmd.status()?;
    Err(match (status, status.code()) {
        _ if status.success() => return Ok(()),
        (_, Some(rc)) => color_eyre::Report::msg("Command exited")
            .warning(lazy_format::lazy_format!("Status code: {}", rc))
            .with_note(|| format!("Command: `{cmd_str}`"))
            .note(lazy_format::lazy_format!("Status: {}", status)),
        _ => color_eyre::Report::msg("Script terminated unexpectedly")
            .note(lazy_format::lazy_format!("Status: {}", status)),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn changed_package_expands_to_every_package_in_its_group() {
        let grouped = |label: &str, group: &str, arches: &[&str]| Project {
            labels: [(label.to_owned(), group.to_owned())].into(),
            arches: Some(arches.iter().map(|arch| (*arch).to_owned()).collect()),
            ..Default::default()
        };
        let config = Manifest {
            project: [
                ("nvidia/driver".to_owned(), grouped("group", "nvidia", &["x86_64"])),
                ("nvidia/utils".to_owned(), grouped("subrepo", "nvidia", &["aarch64"])),
                ("intel/driver".to_owned(), grouped("group", "intel", &["x86_64"])),
            ]
            .into(),
            config: anda_config::Config::default(),
        };

        let entries = build_entries_for_changes(
            &config,
            &["nvidia/driver/anda.hcl".to_owned(), "nvidia/utils/utils.spec".to_owned()],
            Path::new("anda.hcl"),
        );

        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].pkg, "nvidia/driver");
        assert_eq!(entries[0].arch, "x86_64");
        assert_eq!(entries[1].pkg, "nvidia/utils");
        assert_eq!(entries[1].arch, "aarch64");
    }

    #[test]
    fn packages_without_group_label_remain_standalone() {
        let standalone = |subrepo: &str| Project {
            labels: [("subrepo".to_owned(), subrepo.to_owned())].into(),
            arches: Some(vec!["x86_64".to_owned()]),
            ..Default::default()
        };
        let config = Manifest {
            project: [
                ("pkg-a".to_owned(), standalone("nvidia")),
                ("pkg-b".to_owned(), standalone("nvidia")),
            ]
            .into(),
            config: anda_config::Config::default(),
        };

        let entries = build_entries_for_changes(
            &config,
            &["pkg-a/pkg-a.spec".to_owned()],
            Path::new("anda.hcl"),
        );

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].pkg, "pkg-a");
    }

    #[test]
    fn only_group_label_defines_build_group() {
        let group_only = Project {
            labels: [("group".to_owned(), "explicit".to_owned())].into(),
            ..Default::default()
        };
        let group_and_subrepo = Project {
            labels: [
                ("group".to_owned(), "explicit".to_owned()),
                ("subrepo".to_owned(), "fallback".to_owned()),
            ]
            .into(),
            ..Default::default()
        };
        let subrepo_only = Project {
            labels: [("subrepo".to_owned(), "fallback".to_owned())].into(),
            ..Default::default()
        };
        let empty_group =
            Project { labels: [("group".to_owned(), String::new())].into(), ..Default::default() };

        assert_eq!(group_only.build_group(), Some("explicit"));
        assert_eq!(group_and_subrepo.build_group(), Some("explicit"));
        assert_eq!(subrepo_only.build_group(), None);
        assert_eq!(empty_group.build_group(), None);
    }

    #[test]
    fn manifest_change_includes_all_projects() {
        let config = Manifest {
            project: [
                ("pkg-a".to_owned(), Project::default()),
                ("pkg-b".to_owned(), Project::default()),
            ]
            .into(),
            config: anda_config::Config::default(),
        };

        let entries =
            build_entries_for_changes(&config, &["anda.hcl".to_owned()], Path::new("anda.hcl"));

        assert_eq!(entries.len(), 4);
        assert_eq!(
            entries.iter().map(|entry| entry.pkg.as_str()).collect::<Vec<_>>(),
            ["pkg-a", "pkg-a", "pkg-b", "pkg-b"]
        );
    }

    #[test]
    fn build_units_group_label_members_but_keep_standalone_arches_together() {
        let entry = |pkg: &str, arch: &str, group: Option<&str>| BuildEntry {
            pkg: pkg.to_owned(),
            arch: arch.to_owned(),
            labels: group.map_or_else(BTreeMap::new, |group| {
                [("group".to_owned(), group.to_owned())].into()
            }),
        };
        let units = group_build_entries(vec![
            entry("nvidia/a", "x86_64", Some("nvidia")),
            entry("nvidia/b", "aarch64", Some("nvidia")),
            entry("discord", "x86_64", None),
            entry("discord", "aarch64", None),
        ]);

        assert_eq!(units.len(), 2);
        assert_eq!(units[0].name, format!("group-{}", URL_SAFE_NO_PAD.encode("nvidia")));
        assert_eq!(units[0].packages.len(), 2);
        assert_eq!(units[1].name, format!("package-{}", URL_SAFE_NO_PAD.encode("discord")));
        assert_eq!(units[1].packages.len(), 2);
    }

    #[test]
    fn ci_architecture_overrides_and_unique_package_option() {
        let entries = vec![
            BuildEntry {
                pkg: "pkg".to_owned(),
                arch: "x86_64".to_owned(),
                labels: [("group".to_owned(), "g".to_owned())].into(),
            },
            BuildEntry {
                pkg: "pkg".to_owned(),
                arch: "aarch64".to_owned(),
                labels: [("group".to_owned(), "g".to_owned())].into(),
            },
        ];

        let arches = ["x86_64".to_owned(), "aarch64".to_owned()];
        let overridden = configure_build_entries(entries.clone(), &arches, false);
        assert_eq!(
            overridden.iter().map(|entry| entry.arch.as_str()).collect::<Vec<_>>(),
            ["x86_64", "aarch64"]
        );

        let unique = configure_build_entries(entries, &[], true);
        assert_eq!(unique.len(), 1);
        assert_eq!(unique[0].arch, "x86_64");
    }

    #[test]
    fn scm_entries_preserve_package_name() {
        let project = Project {
            labels: [("group".to_owned(), "scm".to_owned())].into(),
            rpm: Some(RpmBuild { enable_scm: Some(true), ..Default::default() }),
            ..Default::default()
        };
        let config = Manifest {
            project: [("pkg".to_owned(), project)].into(),
            config: anda_config::Config::default(),
        };

        let entries = build_entries_for_changes(
            &config,
            &["pkg/SPECS/pkg.spec".to_owned()],
            Path::new("anda.hcl"),
        );

        assert_eq!(entries.len(), DEFAULT_ARCHES.len());
        assert!(entries.iter().all(|entry| entry.pkg == "pkg"));
    }
}
