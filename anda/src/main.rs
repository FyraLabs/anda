use std::path::PathBuf;
use clap::{AppSettings, Parser, Subcommand, ArgEnum};
use log::{debug, error, info, trace};
use anyhow::{anyhow, Result};
use std::fs;

mod build;
mod config;
mod backend;
mod api;

use backend::BackendCommand;

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
        #[clap(value_name = "PROJECT_PATH", default_value = ".")]
        path: PathBuf,
    },
    /// Subcommand for interacting with the build system
    Backend {
        /// Subcommand to run
        #[clap(subcommand)]
        command: BackendCommand,
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    // if RUST_LOG is not set, set it to "info"
    if std::env::var("RUST_LOG").is_err() {
        std::env::set_var("RUST_LOG", "debug");
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

        Command::Build { path } => {
            println!("Building from {}", fs::canonicalize(path.clone()).unwrap().display());
            //build::start_build(&path)?;
            build::ProjectBuilder::new(path).build().await?;
        }

        Command::Backend { command } => {
            backend::match_subcmd(&command).await?;
        }
    };

    Ok(())
}

mod tests {
    #[test]
    fn test_() {}
}
