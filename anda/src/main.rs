#![deny(rust_2018_idioms)]
mod artifacts;
mod cmd;
mod rpm_spec;
mod builder;

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

        //TODO: Move this to an argument group (clap 4.0 feature(?))
        /// Mock: Do not mirror repositories
        /// This is useful for quickly building test repositories
        /// without having to wait for the mirror to finish
        /// This argument is ignored if the build is not RPM Mock
        #[clap(long, action)]
        no_mirrors: bool,

        /// RPM: Builder backend
        /// possible values: rpmbuild, mock
        /// default: mock
        #[clap(long, arg_enum, default_value = "mock")]
        rpm_builder: rpm_spec::RPMBuilder,
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
            no_mirrors,
            rpm_builder,
        } => {
            println!("Build command");
            println!("all: {}", all);
            println!("project: {:?}", project);
            println!("package: {:?}", package);

            let cwd = std::env::current_dir().unwrap();

            match package {
                PackageType::Rpm => {
                    let opts =
                        rpm_spec::RPMOptions::new(None, cwd, cli.target_dir.clone());
                    //let result = backend.build(&cwd.join("tests/umpkg.spec")).unwrap();

                    let result = rpm_builder.build(&PathBuf::from("tests/umpkg.spec"), &opts).unwrap();

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
