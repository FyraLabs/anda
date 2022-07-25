use anyhow::{anyhow, Result};
use clap::{AppSettings, ArgEnum, Parser, Subcommand};
use log::{debug, error, info, trace};
use std::fs;
use std::path::PathBuf;

mod api;
mod backend;
mod build;
mod config;
mod util;
mod error;

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
    /// Install a package
    Install {
        /// Packages to be installed
        #[clap(required = true)]
        packages: Vec<String>,
    },

    /// Remove a package
    Remove {
        /// Packages to be removed
        #[clap(required = true)]
        packages: Vec<String>,
    },

    /// Build an Andaman project
    Build {
        /// Path to the project
        /// If not specified, the current directory is used
        #[clap(value_name = "ANDA_PROJECT_PATH", default_value = ".")]
        path: PathBuf,

        /// Working directory for the build
        /// If not specified, the current directory is used
        #[clap(short,long,value_name = "ANDA_WORKDIR")]
        workdir: Option<PathBuf>,
    },
    /// Subcommand for interacting with the build system
    Backend {
        /// Subcommand to run
        #[clap(subcommand)]
        command: BackendCommand,
    },
    Pack {
        /// Path to the project.
        /// If not specified, the current directory is used
        #[clap(value_name = "ANDA_PROJECT_PATH", default_value = ".")]
        path: PathBuf,

        /// optional name of the package to pack
        #[clap(short, long, value_name = "ANDA_PACK_OUTPUT")]
        output: Option<String>,
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    // if RUST_LOG is not set, set it to "info"
    if std::env::var("RUST_LOG").is_err() {
        std::env::set_var("RUST_LOG", "info,anda=debug");
    }

    pretty_env_logger::init();

    let cli = Cli::parse();

    match cli.command {
        Command::Install { packages } => {
            println!("Installing {}", packages.join(", "));
        }

        Command::Remove { packages } => {
            println!("Removing {}", packages.join(", "));
        }

        Command::Build { path, workdir } => {

            // check if path is file
            if path.is_file() {
                info!("path is a file, calling builder");

                if path.file_name().unwrap().to_str().unwrap().ends_with(".andasrc.zip") {
                    debug!("path is an andasrc tarball package, calling unpacker");
                    ProjectPacker::unpack_and_build(&path, workdir).await.map_err(|e| {
                        error!("{:?}", e);
                        anyhow!("{:?}", e)
                    })?;
                }



                else {
                    // error and exit
                    //error!("path is not a valid build source! Please either use an andasrc tarball or a valid anda project directory");
                    anyhow::bail!("path is not a valid build source! Please either use an andasrc tarball or a valid anda project directory.");
                }
            }
            println!(
                "Building from {}",
                fs::canonicalize(path.clone()).unwrap().display()
            );
        }

        Command::Backend { command } => {
            backend::match_subcmd(&command).await?;
        }
        Command::Pack { path, output } => {
            println!(
                "Packing from {}",
                fs::canonicalize(path.clone()).map_err(|e| {
                    error!("{}", e);
                    e
                })?.display()
            );
            //build::start_build(&path)?;
            let p = util::ProjectPacker::pack(&path, output).await.unwrap();

            println!("Packed to {}", p.display());
        }
    };

    Ok(())
}

mod tests {
    #[test]
    fn test_() {}
}
