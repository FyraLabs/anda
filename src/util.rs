//! Utility functions and types
use anda_config::{Docker, DockerImage, Manifest, Project, RpmBuild};
use clap_verbosity_flag::log::LevelFilter;
use color_eyre::{eyre::eyre, Result, Section};
use console::style;
use itertools::Itertools;
use nix::{sys::signal, unistd::Pid};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, io::Write, path::Path};
use tokio::{io::AsyncBufReadExt, process::Command};
use tracing::{debug, info};

lazy_static::lazy_static! {
    static ref BUILDARCH_REGEX: Regex = Regex::new("BuildArch:\\s*(.+)").unwrap();
    static ref EXCLUSIVEARCH_REGEX: Regex = Regex::new("ExclusiveArch:\\s*(.+)").unwrap();
    static ref DEFAULT_ARCHES: [String; 2] = ["x86_64".to_owned(), "aarch64".to_owned()];
}

#[derive(Copy, Clone)]
enum ConsoleOut {
    Stdout,
    Stderr,
}
// Build entry for GHA
#[derive(Debug, Clone, Serialize, Deserialize, Ord, Eq, PartialEq, PartialOrd)]
pub struct BuildEntry {
    pub pkg: String,
    pub arch: String,
    pub labels: BTreeMap<String, String>,
}

pub fn fetch_build_entries(config: Manifest) -> Vec<BuildEntry> {
    let changed_files = get_changed_files(Path::new(".")).unwrap_or_default();
    let changed_dirs: std::collections::HashSet<_> = changed_files
        .iter()
        .map(|f| f.trim_end_matches(|x| x != '/').trim_end_matches('/'))
        .collect();
    let suffix = config.config.strip_suffix.clone().unwrap_or_default();

    let mut entries = Vec::new();
    for (mut name, project) in config.project {
        let dir = name.trim_end_matches(&suffix);
        if !changed_dirs.contains(dir) {
            continue;
        }

        if let Some(rpm) = project.rpm {
            if rpm.enable_scm.unwrap_or(false) {
                entries.extend(DEFAULT_ARCHES.iter().map(|arch| BuildEntry {
                    pkg: std::mem::take(&mut name),
                    arch: arch.clone(),
                    labels: project.labels.clone(),
                }));
                continue;
            }
        }
        entries.extend(project.arches.unwrap_or_else(|| DEFAULT_ARCHES.to_vec()).into_iter().map(|arch| BuildEntry {
            pkg: name.clone(),
            arch,
            labels: project.labels.clone(),
        }));
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
    async fn log(&mut self) -> Result<()> {
        fn print_log(process: &str, output: &[u8], out: ConsoleOut) {
            // check if no_color is set
            let no_color = std::env::var("NO_COLOR").is_ok();

            let process = {
                if no_color {
                    style(process)
                } else {
                    match out {
                        ConsoleOut::Stdout => style(process).cyan(),
                        ConsoleOut::Stderr => style(process).yellow(),
                    }
                }
            };
            let mut output2 = Vec::with_capacity(output.len() + 10);
            output2.extend_from_slice(format!("{process} │ ").as_bytes());
            for &c in output {
                if c == b'\r' {
                    output2.extend_from_slice(format!("\r{process} │ ").as_bytes());
                } else {
                    output2.push(c);
                }
            }
            std::io::stdout().write_all(&output2).unwrap();
            std::io::stdout().write_all(b"\n").unwrap();
        }

        // make process name a constant string that we can reuse every time we call print_log
        let process = self.as_std().get_program().to_owned().into_string().unwrap();
        let args = self
            .as_std()
            .get_args()
            .map(shell_quote::Sh::quote_vec)
            .map(|s| String::from_utf8(s).unwrap())
            .join(" ");
        debug!("Running command: {process} {args}",);

        // Wrap the command in `script` to force it to give it a TTY
        let mut c = Self::new("script");

        c.arg("-e")
            .arg("-f")
            .arg("/dev/null")
            .arg("-q")
            .arg("-c")
            .arg(format!("{process} {args}"))
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());

        // c.stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::piped());

        trace!(?c, "Running command");

        let mut output = c.spawn().map_err(|e| {
            eyre!("Cannot run command")
                .wrap_err(e)
                .note(format!("Process: {process}"))
                .note("Args: {args}")
                .suggestion(format!("You might need to install `{process}` via a package manager."))
        })?;

        // HACK: Rust ownership is very fun.
        let t = process.clone();

        let stdout = output.stdout.take().unwrap();
        let mut stdout_lines = tokio::io::BufReader::new(stdout).split(b'\n');

        let stderr = output.stderr.take().unwrap();
        let mut stderr_lines = tokio::io::BufReader::new(stderr).split(b'\n');

        // handles so we can run both at the same time
        for task in [
            tokio::spawn(async move {
                while let Some(line) = stdout_lines.next_segment().await.unwrap() {
                    print_log(&t, &line, ConsoleOut::Stdout);
                }
                Ok(())
            }),
            tokio::spawn(async move {
                while let Some(line) = stderr_lines.next_segment().await.unwrap() {
                    print_log(&process, &line, ConsoleOut::Stderr);
                }
                Ok(())
            }),
            tokio::spawn(async move {
                tokio::select! {
                    _ = tokio::signal::ctrl_c() => {
                        info!("Received ctrl-c, sending sigint to child process");
                        #[allow(clippy::cast_possible_wrap)]
                        signal::kill(Pid::from_raw(output.id().unwrap() as i32), signal::Signal::SIGINT).unwrap();

                        // exit program
                        eprintln!("Received ctrl-c, exiting");
                        // std::process::exit(127);
                        Err(eyre!("Received ctrl-c, exiting"))
                    }
                    w = output.wait() => {

                        // check exit status
                        let status = w.unwrap();
                        if status.success() {
                            info!("Command exited successfully");
                            Ok(())
                        } else {
                            info!("Command exited with status: {status}");
                            Err(eyre!("Command exited with status: {status}"))
                        }
                        // info!("Child process finished");
                    }
                }
            }),
        ] {
            task.await??;
        }

        Ok(())
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
        let path = entry.path().strip_prefix("./").unwrap();

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
    };
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
            .warning(lazy_format::lazy_format!("Status code: {rc}"))
            .with_note(|| format!("Command: `{cmd_str}`"))
            .note("Status: {status}"),
        _ => color_eyre::Report::msg("Script terminated unexpectedly")
            .note(lazy_format::lazy_format!("Status: {status}")),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_head() {
        println!("{:?}", get_changed_files(Path::new(".")));
    }
    #[test]
    fn test_entries() {
        let config = anda_config::load_from_file(&PathBuf::from("anda.hcl"));

        fetch_build_entries(config.unwrap());
    }
}
