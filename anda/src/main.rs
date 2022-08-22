#![deny(rust_2018_idioms)]

use anyhow::{anyhow, Result};

use clap::{AppSettings, ArgEnum, Parser, Subcommand};
use log::{debug, error, info};
use std::fs;
use std::{path::PathBuf, str::FromStr};

mod api;
mod backend;
mod build;
mod config;
mod container;
mod error;
mod util;

use backend::BackendCommand;

use crate::util::ProjectPacker;

#[derive(Parser)]
#[clap(about, version)]
#[clap(global_setting = AppSettings::DeriveDisplayOrder)]
struct Cli {
    /// Path to the package
    #[clap(value_name = "FILE", default_value = ".")]
    path: PathBuf,

    #[clap(subcommand)]
    command: Command,

    /// Path to the config file
    #[clap(default_value = "anda.hcl", short, long)]
    config: PathBuf,
}

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, ArgEnum)]
enum BuildBackend {
    System,
    Mock,
}

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, ArgEnum)]
pub enum BuildkitLog {
    Auto,
    Tty,
    Plain,
}

impl FromStr for BuildkitLog {
    fn from_str(s: &str) -> Result<Self, anyhow::Error> {
        match s {
            "auto" => Ok(BuildkitLog::Auto),
            "tty" => Ok(BuildkitLog::Tty),
            "plain" => Ok(BuildkitLog::Plain),
            _ => Err(anyhow!("Invalid buildkit log level: {}", s)),
        }
    }

    type Err = anyhow::Error;
}

impl Default for BuildkitLog {
    fn default() -> Self {
        BuildkitLog::Auto
    }
}

#[derive(Subcommand)]
enum Command {
    /// Build an Andaman project
    Build {
        /// Path to the project
        /// If not specified, the current directory is used
        #[clap(value_name = "PROJECT_PATH", default_value = ".")]
        path: String,

        /// Working directory for the build
        /// If not specified, the current directory is used
        #[clap(short, long, value_name = "WORKDIR")]
        workdir: Option<PathBuf>,

        /// List of projects to be built.
        /// if not specified, all projects are built.
        /// Can be specified multiple times.
        #[clap(short, long, value_name = "PROJECT")]
        projects: Vec<String>,

        /// Scope of the project to be run.
        #[clap(short, long, value_name = "SCOPE")]
        scope: Option<String>,

        /// Output LLB to stdout
        #[clap(short, long, action, default_value = "false")]
        llb: bool,

        /// Log format
        #[clap(short, long, value_name = "FORMAT")]
        buildkit_log: Option<BuildkitLog>,
    },
    /// Subcommand for interacting with the build system
    Backend {
        /// Subcommand to run
        #[clap(subcommand)]
        command: BackendCommand,
    },
    /// Packs up the project into a distributable .andasrc.zip file
    Pack {
        /// Path to the project.
        /// If not specified, the current directory is used
        #[clap(value_name = "ANDA_PROJECT_PATH", default_value = ".")]
        path: PathBuf,

        /// optional name of the package to pack
        #[clap(short, long, value_name = "ANDA_PACK_OUTPUT")]
        output: Option<String>,
    },

    /// Pushes the project into the registry
    Push {
        /// Path to the project.
        /// If not specified, the current directory is used
        #[clap(value_name = "ANDA_PROJECT_PATH", default_value = ".")]
        path: PathBuf,

        /// Target to build to
        #[clap(short, long, value_name = "TARGET")]
        target: String,

        /// Optional project scope to push with
        #[clap(short, long, value_name = "SCOPE")]
        scope: Option<String>,
    },

    /// Shows build info
    BuildInfo {
        /// The build ID to show info for
        id: String,
    },

    /// Sets up buildkit using docker
    Setup,
}

#[tokio::main]
async fn main() -> Result<()> {
    // if RUST_LOG is not set, set it to "info"
    if std::env::var("RUST_LOG").is_err() {
        #[cfg(debug_assertions)]
        std::env::set_var("RUST_LOG", "info,anda=debug");
    }

    pretty_env_logger::init();

    let cli = Cli::parse();

    match cli.command {
        Command::Build {
            path,
            workdir,
            projects,
            scope,
            llb,
            buildkit_log,
        } => {
            // Build Options

            let opts = build::BuilderOptions {
                display_llb: llb,
                config_location: cli.config,
                buildkit_log: buildkit_log.unwrap_or_default(),
            };

            if let Ok(url) = reqwest::Url::parse(&path) {
                info!("path is a URL, calling downloader");
                ProjectPacker::download_and_call_unpack_build(url.as_str(), workdir, &opts, projects)
                    .await
                    .map_err(|e| {
                        error!("{}", e);
                        anyhow!("{}", e)
                    })?;
                return Ok(());
            }

            let path = PathBuf::from(path);

            // check if path is file
            if path.is_file() {
                info!("path is a file, calling builder");

                if path
                    .file_name()
                    .unwrap()
                    .to_str()
                    .unwrap()
                    .ends_with(".andasrc.zip")
                {
                    debug!("path is an andasrc tarball package, calling unpacker");
                    ProjectPacker::unpack_and_build(&path, workdir, &opts, projects)
                        .await
                        .map_err(|e| {
                            error!("{}", e);
                            anyhow!("{}", e)
                        })?;
                } else {
                    // error and exit
                    //error!("path is not a valid build source! Please either use an andasrc tarball or a valid anda project directory");
                    anyhow::bail!("path is not a valid build source! Please either use an andasrc tarball or a valid anda project directory.");
                }
            } else if path.is_dir() {
                if let Some(scope) = scope {
                    build::ProjectBuilder::new(path)
                        .build_in_scope(&scope, &opts)
                        .await
                        .map_err(|e| {
                            error!("{}", e);
                            anyhow!("{}", e)
                        })?;
                    // cargo run --bin anda build -s anda::
                } else {
                    build::ProjectBuilder::new(path)
                        .build(projects, &opts)
                        .await
                        .map_err(|e| {
                            error!("{}", e);
                            anyhow!("{}", e)
                        })?;
                }
            }
        }

        Command::Backend { command } => {
            backend::match_subcmd(&command).await?;
        }
        Command::Pack { path, output } => {
            // check if path is a git url

            let path_str = path.to_str().unwrap();

            if path_str.starts_with("http") && path_str.ends_with(".git")
                || path_str.starts_with("git://") && path_str.ends_with(".git")
                || path_str.starts_with("ssh") && path_str.ends_with(".git")
                || path_str.starts_with("git@") && path_str.ends_with(".git")
            {
                info!("path is a git url, calling packer");
                ProjectPacker::pack_git(path_str).await.map_err(|e| {
                    error!("{}", e);
                    anyhow!("{}", e)
                })?;
            } else {
                println!(
                    "Packing from {}",
                    fs::canonicalize(path.clone())
                        .map_err(|e| {
                            error!("{}", e);
                            e
                        })?
                        .display()
                );
                //build::start_build(&path)?;
                let p = ProjectPacker::pack(&path, output).await.map_err(|e| {
                    error!("{}", e);
                    anyhow!("{}", e)
                })?;

                println!("Packed to {}", p.display());
            }
        }
        Command::Push { path, target, scope } => {
            // pack the project, then push to backend

            let p = ProjectPacker::pack(&path, None).await.map_err(|e| {
                error!("{}", e);
                anyhow!("{}", e)
            })?;

            // pushin p
            let backend = api::AndaBackend::new(None);
            // get target by name
            let target = backend.get_target_by_name(&target).await.map_err(|e| {
                error!("{}", e);
                anyhow!("{}", e)
            })?;

            //let target_id_test = uuid::Uuid::parse_str("ad84b005-a147-4235-a339-eea78157ec0c").unwrap();

            // push da p
            let b = backend.upload_build(target.id, &p, scope).await.map_err(|e| {
                error!("{}", e);
                anyhow!("{}", e)
            })?;
            println!("{:?}", b);
        }
        Command::BuildInfo { id } => {
            // try and parse the build id as uuid
            if let Ok(uuid) = uuid::Uuid::parse_str(&id) {
                crate::backend::buildinfo(uuid).await?;
            } else {
                anyhow::bail!("invalid build id");
            }
        }
        Command::Setup => {
            // run docker
            let c = std::process::Command::new("docker")
                .arg("run")
                .arg("-d")
                .arg("--name")
                .arg("anda-buildkitd")
                .arg("--privileged")
                .arg("--restart")
                .arg("always")
                .arg("moby/buildkit:latest")
                .status()?;

            if !c.success() {
                anyhow::bail!("failed to start buildkitd docker service");
            }

        }
    };

    Ok(())
}

mod tests {
    #[test]
    fn test_() {}
}
