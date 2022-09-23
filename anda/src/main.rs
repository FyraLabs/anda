#![deny(rust_2018_idioms)]
mod artifacts;
mod cmd;
mod rpm_spec;

use anyhow::{anyhow, Result};

use crate::rpm_spec::RPMSpecBackend;
use clap::{AppSettings, ArgEnum, Parser, Subcommand, ValueEnum};
use std::{path::PathBuf, str::FromStr};

use self::artifacts::PackageType;

#[derive(Parser, Debug)]
#[clap(about, version)]
#[clap(global_setting = AppSettings::DeriveDisplayOrder)]
struct Cli {
    #[clap(subcommand)]
    command: Command,

    /// Path to Andaman configuration file
    #[clap(default_value = "anda.hcl", short, long, env = "ANDA_CONFIG")]
    config: PathBuf,

    /// Output directory for built packages
    #[clap(short, long, env = "TARGET_DIR", default_value = "anda-build")]
    target_dir: PathBuf,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Build a project
    Build {
        /// Builds all projects in the current directory
        #[clap(short, long, action)]
        all: bool,

        /// Project to build
        #[clap()]
        project: Option<String>,

        /// Builds a specific artifact format (default: all)
        /// possible values: rpm, docker, podman, flatpak, rpm-ostree
        #[clap(short, long, arg_enum, default_value = "all")]
        package: PackageType,
    },
}

fn main() {
    //println!("Hello, world!");
    let cli = Cli::parse();

    println!("{:?}", cli);

    match cli.command {
        Command::Build {
            all,
            project,
            package,
        } => {
            println!("Build command");
            println!("all: {}", all);
            println!("project: {:?}", project);
            println!("package: {:?}", package);

            let cwd = std::env::current_dir().unwrap();

            match package {
                PackageType::Rpm => {
                    let backend =
                        rpm_spec::MockBackend::new(None, cwd.clone(), cli.target_dir.clone());
                    let result = backend.build(&cwd.join("tests/umpkg.spec")).unwrap();

                    for path in result {
                        println!("Built: {}", path.display());
                    }
                }
                PackageType::Docker => todo!(),
                PackageType::Podman => todo!(),
                PackageType::Flatpak => todo!(),
                PackageType::RpmOstree => todo!(),
                PackageType::All => todo!(),
            }
        }
    }
}
