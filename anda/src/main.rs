#![deny(rust_2018_idioms)]
mod artifacts;
mod builder;
mod flatpak;
mod oci;
mod rpm_spec;

use anyhow::Result;

use clap::{AppSettings, CommandFactory, Parser, Subcommand, Args};
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
#[clap(help_heading = "Global Options")]
pub struct Cli {
    #[clap(subcommand)]
    command: Command,

    /// Path to Andaman configuration file
    #[clap(default_value = "anda.hcl", short, long, env = "ANDA_CONFIG")]
    config: PathBuf,

    #[clap(flatten)]
    verbose: clap_verbosity_flag::Verbosity,

    /// Output directory for built packages
    #[clap(short, long, env = "TARGET_DIR", default_value = "anda-build")]
    target_dir: PathBuf,
}

#[derive(Args, Debug, Clone)]
#[clap(help_heading = "Flatpak Builder Options")]
pub struct FlatpakOpts {
    /// Flatpak: Extra source directory
    /// can be defined multiple times
    #[clap(long, group = "extra-source")]
    flatpak_extra_sources: Vec<String>,

    /// Flatpak: Extra source URL
    /// can be defined multiple times
    #[clap(long)]
    flatpak_extra_sources_url: Vec<String>,

    /// Flatpak: Do not delete the build directory
    #[clap(long, action)]
    flatpak_dont_delete_build_dir: bool,
}

#[derive(Args, Debug, Clone)]
#[clap(help_heading = "OCI Builder Options")]
pub struct OciOpts {
    /// OCI: Labels to add to the image
    #[clap(long)]
    label: Vec<String>,

    /// OCI: Build Arguments to pass to the build
    #[clap(long)]
    build_arg: Vec<String>,

    /// OCI: compress the context with gzip
    #[clap(long, action)]
    compress: bool,
}

#[derive(Args, Debug, Clone)]
#[clap(help_heading = "RPM Options")]
pub struct RpmOpts {
    /// RPM: Do not mirror repositories.
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


    /// RPM: Define a custom macro
    /// can be defined multiple times
    #[clap(short = 'D', long)]
    rpm_macro: Vec<String>,

    /// RPM: Mock configuration
    #[clap(long, short = 'c')]
    mock_config: Option<String>,
}

#[derive(Parser, Debug, Clone)]
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

    let mut app = Cli::command();

    app.build();

    let sub = app.get_subcommands();

    // for s in sub {
    //     println!("{:?}", s);
    // }

    // let app = Command::command().find_subcommand("build").unwrap().clone();
    // clap_mangen::Man::new(app).render(&mut std::io::stdout()).unwrap();
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
                let mut app = Cli::command();


                app.find_subcommand_mut("build").unwrap().print_help().unwrap();
                // print help for build subcommand
                return Err(anyhow::anyhow!(
                    "No project specified, and --all not specified."
                ));
            }

            eprintln!("{:?}", &all);
            builder::builder(
                &cli,
                rpm_opts,
                all,
                project,
                package,
                flatpak_opts,
                oci_opts,
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
