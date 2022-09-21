#![deny(rust_2018_idioms)]
mod build_type;
mod rpm_spec;
use anyhow::{anyhow, Result};

use clap::{AppSettings, ArgEnum, Parser, Subcommand, ValueEnum};
use std::{path::PathBuf, str::FromStr};
use self::build_type::PackageType;

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
        #[clap(short, long, default_value = "false")]
        all: bool,

        /// Project to build
        #[clap()]
        project: Option<String>,

        /// Builds a specific artifact format (default: all)
        /// possible values: rpm, docker, podman, flatpak, rpm-ostree
        #[clap(short, long)]
        package: Option<PackageType>,
    }
}


fn main() {
    //println!("Hello, world!");
    let cli = Cli::parse();

    println!("{:?}", cli);
}