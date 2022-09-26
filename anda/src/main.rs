#![deny(rust_2018_idioms)]
mod artifacts;
mod builder;
mod flatpak;
mod oci;
mod rpm_spec;

use anyhow::Result;

use clap::{AppSettings, Parser, Subcommand, ArgAction};
use std::path::PathBuf;

use self::artifacts::PackageType;


/// Andaman is a package building toolchain that can automate building packages in various formats,
/// such as RPM, Flatpak, Docker, etc.
///
/// It is designed to be used in a CI/CD pipeline, but can also be used locally.
/// To use Andaman, you need a project manifest file in the root of your repository.
/// The file in question is a HCL (Hashicorp Configuration Language) file, and it is called `anda.hcl`.
/// The file is used to configure the build process, and it is used to define the build steps.
///
#[derive(Parser, Debug)]
#[clap(about, version)]
#[clap(global_setting = AppSettings::DeriveDisplayOrder)]
pub struct Cli {
    #[clap(subcommand)]
    command: Command,

    /// Path to Andaman configuration file
    #[clap(default_value = "anda.hcl", short, long, env = "ANDA_CONFIG")]
    config: PathBuf,

    /// Output directory for built packages
    #[clap(short, long, env = "TARGET_DIR", default_value = "anda-build")]
    target_dir: PathBuf,
}


#[derive(Parser, Debug, Clone)]
pub struct FlatpakOpts {
    /// Flatpak: Extra source directory
    /// can be defined multiple times
    #[clap(long)]
    flatpak_extra_sources: Vec<String>,

    /// Flatpak: Extra source URL
    /// can be defined multiple times
    #[clap(long)]
    flatpak_extra_sources_url: Vec<String>,

    /// Flatpak: Do not delete the build directory
    #[clap(long, action)]
    flatpak_dont_delete_build_dir: bool,
}

#[derive(Parser, Debug, Clone)]
pub struct OciOpts {
    /// OCI: Labels to add to the image
    /// can be defined multiple times
    #[clap(long)]
    label: Vec<String>,

    /// OCI: Build Arguments to pass to the build
    /// can be defined multiple times
    #[clap(long)]
    build_arg: Vec<String>,

    /// OCI: compress the context with gzip
    #[clap(long, action)]
    gzip: bool,
}

#[derive(Parser, Debug, Clone)]
pub struct RpmOpts {
    /// Mock: Do not mirror repositories.
    ///
    /// This flag sets the `mirror` config opt in your mock config to `false`, which most mock configs use to enable
    /// usage of the test repo in Fedora.
    /// The test repo is usually an internal Koji artifact repository used in its build tags.
    /// This is useful for quickly building from test repositories
    /// without having to wait for the compose to finish.
    ///
    /// This argument is ignored if the build is not RPM Mock.
    #[clap(long, action)]
    no_mirrors: bool,

    /// RPM: Builder backend
    /// possible values: rpmbuild, mock
    /// default: mock
    #[clap(long, arg_enum, default_value = "mock")]
    rpm_builder: rpm_spec::RPMBuilder,

    /// Mock: Mock configuration
    #[clap(long, short = 'c')]
    mock_config: Option<String>,
}

#[derive(Subcommand, Debug, Clone)]
enum Command {
    /// Build a project
    ///
    /// This is the main entrypoint of Andaman.
    /// This command optionally accepts a project name to build, or an `--all` flag to build all projects in the manifest.
    /// If no project name is specified, and the `--all` flag is not specified, the program will exit with an error.
    Build {
        /// Builds all projects in the current directory
        #[clap(short, long, action)]
        all: bool,

        /// Project to build
        #[clap()]
        project: Option<String>,

        /// Builds a specific artifact format
        #[clap(short, long, arg_enum, default_value = "all")]
        package: PackageType,

        /// Options for RPM builds
        #[clap(flatten)]
        rpm_opts: RpmOpts,

        /// Options for Flatpak builds
        #[clap(flatten)]
        flatpak_opts: FlatpakOpts,

        /// Options for OCI builds
        #[clap(flatten)]
        oci_opts: OciOpts,
    },
    /// Cleans up the build directory
    Clean,
}

fn main() -> Result<()> {
    //println!("Hello, world!");
    let cli = Cli::parse();

    // println!("{:?}", &cli);

    match cli.command.clone() {
        Command::Build {
            all,
            project,
            package,
            rpm_opts,
            flatpak_opts,
            oci_opts,
        } => {

            if project.is_none() && !all {
                // print help
                return Err(anyhow::anyhow!("No project specified, and --all not specified. Please run `anda build --help` for more information. This program will now exit."));
            }

            eprintln!("{:?}", &all);
            builder::builder(
                &cli,
                rpm_opts,
                all,
                project,
                package,
            )?;
        }
        Command::Clean => {
            println!("Cleaning up build directory");
            let clean = std::fs::remove_dir_all(&cli.target_dir);
            if clean.is_err() {
                // match the errors
                match clean.err().unwrap().kind() {
                    std::io::ErrorKind::NotFound => {}
                    e => {
                        println!("Error cleaning up build directory: {:?}", e);
                    }
                }
            }
        }
    }
    Ok(())
}
