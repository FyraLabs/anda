//! Utility functions and types
use anda_config::{Docker, DockerImage, Manifest, Project, RpmBuild};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use clap_verbosity_flag::log::LevelFilter;
use color_eyre::Result;

use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    path::Path,
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

    let mut first_per_package = BTreeMap::new();
    for entry in entries {
        first_per_package.entry(entry.pkg.clone()).or_insert(entry);
    }

    if arches.is_empty() {
        return first_per_package.into_values().collect();
    }

    first_per_package
        .into_values()
        .flat_map(|entry| {
            arches.iter().map(move |arch| BuildEntry { arch: arch.clone(), ..entry.clone() })
        })
        .collect()
}

pub fn group_build_entries(entries: Vec<BuildEntry>) -> Vec<BuildUnit> {
    let mut units: BTreeMap<String, Vec<BuildEntry>> = BTreeMap::new();
    for entry in entries {
        let name = match entry.labels.get("group").filter(|group| !group.is_empty()) {
            Some(group) => format!("group-{}", URL_SAFE_NO_PAD.encode(group)),
            None => format!("package-{}", URL_SAFE_NO_PAD.encode(&entry.pkg)),
        };
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
    let projects = if changed_files.iter().any(|file| is_root_manifest(file, config_path)) {
        config.project.keys().map(String::as_str).collect()
    } else {
        with_group_members(config, changed_projects(config, changed_files))
    };

    projects
        .into_iter()
        .flat_map(|name| {
            let project = &config.project[name];
            project_arches(project).into_iter().map(|arch| BuildEntry {
                pkg: name.to_owned(),
                arch,
                labels: project.labels.clone(),
            })
        })
        .collect()
}

fn is_root_manifest(file: &str, config_path: &Path) -> bool {
    let cwd = std::env::current_dir().unwrap_or_default();
    Path::new(file) == config_path.strip_prefix(cwd).unwrap_or(config_path)
}

fn changed_projects<'a>(config: &'a Manifest, changed_files: &[String]) -> BTreeSet<&'a str> {
    let suffix = config.config.strip_suffix.as_deref().unwrap_or_default();
    let mut projects_by_dir: HashMap<&Path, Vec<&str>> = HashMap::new();
    for name in config.project.keys() {
        projects_by_dir.entry(Path::new(name.trim_end_matches(suffix))).or_default().push(name);
    }

    changed_files
        .iter()
        .flat_map(|file| Path::new(file).ancestors())
        .filter_map(|dir| projects_by_dir.get(dir))
        .flatten()
        .copied()
        .collect()
}

fn with_group_members<'a>(
    config: &'a Manifest,
    mut projects: BTreeSet<&'a str>,
) -> BTreeSet<&'a str> {
    let groups: BTreeSet<&str> =
        projects.iter().filter_map(|name| config.project[*name].build_group()).collect();
    projects.extend(
        config
            .project
            .iter()
            .filter(|(_, project)| {
                project.build_group().is_some_and(|group| groups.contains(group))
            })
            .map(|(name, _)| name.as_str()),
    );
    projects
}

fn project_arches(project: &Project) -> Vec<String> {
    let scm = project.rpm.as_ref().is_some_and(|rpm| rpm.enable_scm.unwrap_or(false));
    match &project.arches {
        Some(arches) if !scm => arches.clone(),
        _ => DEFAULT_ARCHES.to_vec(),
    }
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

    fn project(labels: &[(&str, &str)], arches: &[&str]) -> Project {
        Project {
            labels: labels.iter().map(|(k, v)| ((*k).to_owned(), (*v).to_owned())).collect(),
            arches: (!arches.is_empty())
                .then(|| arches.iter().map(|&arch| arch.to_owned()).collect()),
            ..Default::default()
        }
    }

    fn manifest(projects: Vec<(&str, Project)>) -> Manifest {
        Manifest {
            project: projects.into_iter().map(|(name, p)| (name.to_owned(), p)).collect(),
            config: anda_config::Config::default(),
        }
    }

    fn entry(pkg: &str, arch: &str, group: Option<&str>) -> BuildEntry {
        BuildEntry {
            pkg: pkg.to_owned(),
            arch: arch.to_owned(),
            labels: group.into_iter().map(|g| ("group".to_owned(), g.to_owned())).collect(),
        }
    }

    fn changes(config: &Manifest, files: &[&str]) -> Vec<(String, String)> {
        let files: Vec<String> = files.iter().map(|&f| f.to_owned()).collect();
        build_entries_for_changes(config, &files, Path::new("anda.hcl"))
            .into_iter()
            .map(|entry| (entry.pkg, entry.arch))
            .collect()
    }

    fn pairs(expected: &[(&str, &str)]) -> Vec<(String, String)> {
        expected.iter().map(|&(pkg, arch)| (pkg.to_owned(), arch.to_owned())).collect()
    }

    #[test]
    fn changed_package_pulls_in_its_group() {
        let config = manifest(vec![
            ("nvidia/driver", project(&[("group", "nvidia")], &["x86_64"])),
            ("nvidia/utils", project(&[("group", "nvidia")], &["aarch64"])),
            ("intel/driver", project(&[("group", "intel")], &["x86_64"])),
        ]);

        assert_eq!(
            changes(&config, &["nvidia/driver/anda.hcl"]),
            pairs(&[("nvidia/driver", "x86_64"), ("nvidia/utils", "aarch64")])
        );
    }

    #[test]
    fn subrepo_label_is_not_a_group() {
        let config = manifest(vec![
            ("pkg-a", project(&[("subrepo", "nvidia")], &["x86_64"])),
            ("pkg-b", project(&[("subrepo", "nvidia")], &["x86_64"])),
        ]);

        assert_eq!(changes(&config, &["pkg-a/pkg-a.spec"]), pairs(&[("pkg-a", "x86_64")]));
    }

    #[test]
    fn root_manifest_change_rebuilds_everything() {
        let config = manifest(vec![("pkg-a", Project::default()), ("pkg-b", Project::default())]);

        assert_eq!(
            changes(&config, &["anda.hcl"]),
            pairs(&[
                ("pkg-a", "x86_64"),
                ("pkg-a", "aarch64"),
                ("pkg-b", "x86_64"),
                ("pkg-b", "aarch64"),
            ])
        );
    }

    #[test]
    fn scm_projects_use_default_arches() {
        let scm = Project {
            rpm: Some(RpmBuild { enable_scm: Some(true), ..Default::default() }),
            ..project(&[], &["riscv64"])
        };
        let config = manifest(vec![("pkg", scm)]);

        assert_eq!(
            changes(&config, &["pkg/SPECS/pkg.spec"]),
            pairs(&[("pkg", "x86_64"), ("pkg", "aarch64")])
        );
    }

    #[test]
    fn units_split_by_group_or_package() {
        let units = group_build_entries(vec![
            entry("nvidia/a", "x86_64", Some("nvidia")),
            entry("nvidia/b", "aarch64", Some("nvidia")),
            entry("discord", "x86_64", None),
            entry("discord", "aarch64", None),
        ]);

        let summary: Vec<_> = units.into_iter().map(|u| (u.name, u.packages.len())).collect();
        assert_eq!(
            summary,
            [
                (format!("group-{}", URL_SAFE_NO_PAD.encode("nvidia")), 2),
                (format!("package-{}", URL_SAFE_NO_PAD.encode("discord")), 2),
            ]
        );
    }

    #[test]
    fn arch_override_and_unique_packages() {
        let entries = vec![entry("pkg", "x86_64", Some("g")), entry("pkg", "aarch64", Some("g"))];

        let overridden = configure_build_entries(entries.clone(), &["riscv64".to_owned()], false);
        assert_eq!(overridden, [entry("pkg", "riscv64", Some("g"))]);

        let unique = configure_build_entries(entries, &[], true);
        assert_eq!(unique, [entry("pkg", "x86_64", Some("g"))]);
    }
}
