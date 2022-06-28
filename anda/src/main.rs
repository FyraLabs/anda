use std::path::PathBuf;

use clap::{AppSettings, Parser, Subcommand};
use log::{debug, error, info, trace};
use log4rs::*;

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
}

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Command::Install { packages } => {
            println!("Installing {}", packages.join(", "));
        }

        Command::Remove { packages } => {
            println!("Removing {}", packages.join(", "));
        }
    }
}

mod tests {
    #[test]
    fn test_() {}
}
