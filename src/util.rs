//! Utility functions and types

use std::{collections::BTreeMap, fs::read_to_string, path::Path};

use anda_config::{Docker, DockerImage, Manifest, Project, RpmBuild};
use anyhow::Result;
use async_trait::async_trait;
use console::style;
use lazy_static::lazy_static;
use log::{debug, info};
use nix::sys::signal;
use nix::unistd::Pid;
use regex::Regex;
use serde::{Deserialize, Serialize};
use tokio::io::AsyncBufReadExt;
use tokio::process::Command;

lazy_static! {
    static ref ARCH_REGEX: Regex = Regex::new("(BuildArch|ExclusiveArch):\\s(.+)").unwrap();
}

enum ConsoleOut {
    Stdout,
    Stderr,
}
// Build entry for GHA
#[derive(Debug, Clone, Serialize, Deserialize, Ord, Eq, PartialEq, PartialOrd)]
pub struct BuildEntry {
    pub pkg: String,
    pub arch: String,
}

pub fn fetch_build_entries(config: Manifest) -> Result<Vec<BuildEntry>> {
    let changed_files = get_changed_files(Path::new(".")).unwrap_or_default();

    let default_arches = vec!["x86_64".to_string(), "aarch64".to_string()];

    let mut entries = Vec::new();

    let regex = Regex::new("(BuildArch|ExclusiveArch):\\s+(.+)")?;

    for (name, project) in config.project {
        if !changed_files
            .iter()
            .filter_map(|file| Path::new(file).parent())
            .any(|file| name.starts_with(file.to_str().unwrap()))
        {
            continue;
        }

        if let Some(rpm) = project.rpm {
            let mut arches: Vec<String> = Vec::new();
            let spec = rpm.spec;
            let spec_contents = read_to_string(spec)?;
            for cap in regex.captures_iter(spec_contents.as_str()) {
                arches.append(
                    &mut cap[2]
                        .split(' ')
                        .map(|arch| arch.to_string())
                        .collect::<Vec<String>>(),
                );
            }

            if arches.is_empty()
                || arches
                    .iter()
                    .any(|arch| arch == "noarch" || arch.starts_with('%'))
            {
                arches = default_arches.clone();
            }

            for arch in arches {
                entries.push(BuildEntry {
                    pkg: name.clone(),
                    arch,
                });
            }
        }
    }

    Ok(entries)
}

// #[test]
// fn test_entries() {
//     let config = anda_config::load_from_file(&PathBuf::from("anda.hcl"));

//     fetch_build_entries(config.unwrap());
// }

/// Command Logging
///
/// This trait implements custom logging for commands in a format of `{command} | {line}`
/// It also implements Ctrl-C handling for the command, and will send a SIGINT to the command
#[async_trait]
pub trait CommandLog {
    async fn log(&mut self) -> Result<()>;
}
#[async_trait]
impl CommandLog for Command {
    async fn log(&mut self) -> Result<()> {
        // let cmd_name = self;
        // make process name a constant string that we can reuse every time we call print_log
        let process = self
            .as_std()
            .get_program()
            .into_string()
            .unwrap();
        let args = self
            .as_std()
            .get_args()
            .map(|a| a.to_str().unwrap())
            .collect::<Vec<&str>>()
            .join(" ");
        debug!("Running command: {process} {args}",);
        let c = self
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());

        // copy self

        let mut output = c.spawn().unwrap();

        fn print_log(process: &str, output: String, out: ConsoleOut) {
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

            let formatter = format!("{}\t| {}", process, output);
            println!("{}", formatter);
        }

        // handles so we can run both at the same time

        let mut tasks = vec![];
        // stream stdout

        let stdout = output.stdout.take().unwrap();
        let stdout_reader = tokio::io::BufReader::new(stdout);
        let mut stdout_lines = stdout_reader.lines();

        // HACK: Rust ownership is very fun.
        let t = process.clone();
        let stdout_handle = tokio::spawn(async move {
            while let Some(line) = stdout_lines.next_line().await.unwrap() {
                print_log(&t, line, ConsoleOut::Stdout);
            }
            Ok(())
        });

        tasks.push(stdout_handle);

        // stream stderr

        debug!("Streaming stderr");
        let stderr = output.stderr.take().unwrap();
        let stderr_reader = tokio::io::BufReader::new(stderr).lines();
        let mut stderr_lines = stderr_reader;

        debug!("stderr: {:?}", stderr_lines);

        let stderr_handle = tokio::spawn(async move {
            while let Some(line) = stderr_lines.next_line().await.unwrap() {
                print_log(&process, line, ConsoleOut::Stderr);
            }
            Ok(())
        });

        // send sigint to child process when we ctrl-c
        tasks.push(stderr_handle);

        // sigint handle

        let sigint_handle = tokio::spawn(async move {
            // wait for ctrl-c or child process to finish

            tokio::select! {
                _ = tokio::signal::ctrl_c() => {
                    info!("Received ctrl-c, sending sigint to child process");
                    signal::kill(Pid::from_raw(output.id().unwrap() as i32), signal::Signal::SIGINT).unwrap();

                    // exit program
                    eprintln!("Received ctrl-c, exiting");
                    // std::process::exit(127);
                    Err(anyhow::anyhow!("Received ctrl-c, exiting"))
                }
                w = output.wait() => {

                    // check exit status
                    let status = w.unwrap();
                    if status.success() {
                        info!("Command exited successfully");
                        Ok(())
                    } else {
                        info!("Command exited with status: {}", status);
                        Err(anyhow::anyhow!("Command exited with status: {}", status))
                    }
                    // info!("Child process finished");
                }
            }
        });

        tasks.push(sigint_handle);

        for task in tasks {
            task.await??;
        }

        Ok(())

        // output.wait().await.unwrap();
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
            changed_files.push(
                delta
                    .new_file()
                    .path()
                    .unwrap()
                    .to_str()
                    .unwrap()
                    .to_string(),
            );
            true
        },
        None,
        None,
        None,
    )
    .ok()?;
    Some(changed_files)
}

#[test]
fn test_head() {
    println!("{:?}", get_changed_files(Path::new(".")));
}

/// Formats the current time in the format of YYYYMMDD
use chrono::prelude::*;
pub fn get_date() -> String {
    let now: DateTime<Utc> = Utc::now();
    now.format("%Y%m%d").to_string()
}

use promptly::prompt_default;

/// Initializes a new anda project
pub fn init(path: &Path, yes: bool) -> Result<()> {
    // create the directory if not exists
    if !path.exists() {
        std::fs::create_dir(path)?;
    }

    let mut config = Manifest {
        project: BTreeMap::new(),
        config: Default::default(),
    };

    // use ignore to scan for files
    let walk = ignore::WalkBuilder::new(path).build();

    for entry in walk {
        let entry = entry?;
        let path = entry.path().strip_prefix("./").unwrap();

        if path.is_file() {
            if path.extension().unwrap_or_default().eq("spec") {
                {
                    debug!("Found spec file: {}", path.display());
                    // ask if we want to add spec to project
                    let add_spec: bool = {
                        if yes {
                            true
                        } else {
                            prompt_default(
                                &format!("Add spec file `{}` to manifest?", path.display()),
                                true,
                            )?
                        }
                    };

                    if add_spec {
                        let project_name = path.file_stem().unwrap().to_str().unwrap();
                        let project = Project {
                            rpm: Some(RpmBuild {
                                spec: path.to_path_buf(),
                                ..Default::default()
                            }),
                            ..Default::default()
                        };
                        config.project.insert(project_name.to_string(), project);
                    }
                }
            }

            let mut counter = 0;
            if path.extension().unwrap_or_default().eq("dockerfile")
                || path
                    .file_name()
                    .unwrap_or_default()
                    .to_str()
                    .unwrap()
                    .eq("Dockerfile")
            {
                let add_oci: bool = {
                    if yes {
                        true
                    } else {
                        prompt_default(
                            &format!("Add Dockerfile `{}` to manifest?", path.display()),
                            true,
                        )?
                    }
                };

                if add_oci {
                    // create a new project called docker

                    let mut docker = Docker {
                        ..Default::default()
                    };

                    let image = DockerImage {
                        dockerfile: Some(path.display().to_string()),
                        ..Default::default()
                    };
                    counter += 1;
                    let image_name = format!("docker-{}", counter);
                    docker.image.insert(image_name, image);

                    let project = Project {
                        docker: Some(docker),
                        ..Default::default()
                    };

                    // increment counter
                    config.project.insert("docker".to_string(), project);
                }
            }
        }
    }
    println!("{}", anda_config::config::to_string(config)?);

    Ok(())
}
