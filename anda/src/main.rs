#![deny(rust_2018_idioms)]

use anyhow::{anyhow, Result};
use buildkit_llb::prelude::{MultiBorrowedOutput, Terminal};
use clap::{AppSettings, ArgEnum, Parser, Subcommand};
use log::{debug, error, info};
use std::path::PathBuf;
use std::{collections::HashMap, fs, io::stdout};

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
}

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, ArgEnum)]
enum BuildBackend {
    System,
    Mock,
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
    Buildx,
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
        } => {
            if let Ok(url) = reqwest::Url::parse(&path) {
                info!("path is a URL, calling downloader");
                ProjectPacker::download_and_call_unpack_build(url.as_str(), workdir)
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
                    ProjectPacker::unpack_and_build(&path, workdir)
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
                        .build_in_scope(&scope)
                        .await
                        .map_err(|e| {
                            error!("{}", e);
                            anyhow!("{}", e)
                        })?;
                    // cargo run --bin anda build -s anda::
                } else {
                    build::ProjectBuilder::new(path)
                        .build(projects)
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
        Command::Buildx => {
            let hash = HashMap::from([("FOO".to_string(), "BAR".to_string())]);

            let opts = container::BuildkitOptions {
                env: Some(hash),
                ..Default::default()
            };
            let mut b = container::Buildkit::new(Some(opts)).image("alpine:latest");
            //b.command("sudo dnf install -y git");
            b.command("echo 'hello world' > /builddir/file0");
            //b.command("ls -la /src");
            b.command("echo 'hello world' > /builddir/file1 && cat /builddir/file0");
            b.command("echo 'hello world' > /builddir/file2 && cat /builddir/file1");
            b.command("echo $FOO");

            //Terminal::with(b.build_graph()).write_definition(std::io::stdout());

            b.execute()?;
        }
    };

    Ok(())
}

mod tests {
    #[test]
    fn test_() {}
}
