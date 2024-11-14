// This module is included in the build.rs file so we can generate some CLI completions/man pages
// If you want to add a crate in here, also add it to build-dependencies

use clap::{Args, Parser, Subcommand, ValueEnum};
use clap_complete::Shell;
use clap_verbosity_flag::InfoLevel;
use std::{path::PathBuf, str::FromStr};

#[derive(ValueEnum, Debug, Clone, Copy, Default)]
pub enum RPMBuilder {
    #[default]
    Mock,
    Rpmbuild,
}

#[derive(Copy, Clone, ValueEnum, Debug)]
pub enum PackageType {
    Rpm,
    Docker,
    Podman,
    Flatpak,
    // RpmOstree,
    All,
}

impl FromStr for PackageType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "rpm" => Ok(Self::Rpm),
            "docker" => Ok(Self::Docker),
            "podman" => Ok(Self::Podman),
            "flatpak" => Ok(Self::Flatpak),
            // "rpm-ostree" => Ok(Self::RpmOstree),
            "all" => Ok(Self::All),
            _ => Err(format!("Invalid package type: {s}")),
        }
    }
}

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
// #[clap(global_setting = AppSettings::DeriveDisplayOrder)]
pub struct Cli {
    #[clap(subcommand)]
    pub command: Command,

    /// Path to Andaman configuration file
    #[clap(default_value = "anda.hcl", short, long, env = "ANDA_CONFIG")]
    pub config: PathBuf,

    #[clap(flatten)]
    // #[clap(default_value = "info")]
    pub verbose: clap_verbosity_flag::Verbosity<InfoLevel>,

    /// Output directory for built packages
    #[clap(short, long, env = "TARGET_DIR", default_value = "anda-build")]
    pub target_dir: PathBuf,
}

#[derive(Args, Debug, Clone, Default)]
pub struct FlatpakOpts {
    /// Flatpak: Extra source directory
    /// can be defined multiple times
    #[clap(long, group = "extra-source")]
    pub extra_sources: Vec<String>,

    /// Flatpak: Extra source URL
    /// can be defined multiple times
    #[clap(long)]
    pub extra_sources_url: Vec<String>,

    /// Flatpak: Do not delete the build directory
    #[clap(long, action)]
    pub dont_delete_build_dir: bool,
}

#[derive(Args, Debug, Clone, Default)]
pub struct OciOpts {
    /// OCI: Labels to add to the image
    #[clap(long)]
    pub label: Vec<String>,

    /// OCI: Build Arguments to pass to the build
    #[clap(long)]
    pub build_arg: Vec<String>,

    /// OCI: compress the context with gzip
    #[clap(long, action)]
    pub compress: bool,
}

#[derive(Args, Debug, Clone, Default)]
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
    pub no_mirrors: bool,

    /// RPM: Builder backend
    #[clap(long, short, value_enum, default_value = "mock")]
    pub rpm_builder: RPMBuilder,

    /// RPM: Define a custom macro
    /// can be defined multiple times
    #[clap(short = 'D', long)]
    pub rpm_macro: Vec<String>,

    /// RPM: A target to pass to rpmbuild/mock, useful for cross compilation
    #[clap(long)]
    pub rpm_target: Option<String>,

    /// RPM: Mock configuration
    #[clap(long, short = 'c')]
    pub mock_config: Option<String>,

    /// RPM: Extra repositories to pass to mock
    #[clap(long, short = 'R')]
    pub extra_repos: Vec<String>,
}

#[derive(Subcommand, Debug, Clone)]
#[allow(clippy::large_enum_variant)]
pub enum Command {
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
        #[clap(short, long, value_enum, default_value = "all")]
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

    /// Lists all projects in the manifest
    List,

    /// Initializes a new project manifest
    Init {
        /// Path to the project manifest
        #[clap(default_value = ".")]
        path: PathBuf,

        /// Assume yes to all questions
        #[clap(short, long, action)]
        yes: bool,
    },
    /// Generate shell completions
    Completion {
        /// Shell to generate completions for
        #[clap(value_enum)]
        shell: Shell,
    },
    /// Get CI output for Github Actions
    CI,

    /// Update all projects
    Update {
        /// Labels to pass to the scripts
        #[clap(short, long)]
        labels: Vec<String>,
        /// Only run update scripts in project with the specified labels
        ///
        /// This should be a comma-separated list of filters.
        /// Each time `--filters=...` is specified, the comma-separated list of key-values will be
        /// checked against a project. If missing or different, the project will be ignored.
        /// However, specifying `--filters` multiple times will create an "or" effect --- the
        /// project will not be ignored if it satisfies one of the list of `--filters`. For
        /// example, `-f a=1,b=2 -f c=3` means the project needs to satisfy either "a=1" and "b=2",
        /// or only "c=3".
        #[clap(short, long)]
        filters: Vec<String>,
        /// Exclude update scripts in project with the specified labels
        ///
        /// This should be a comma-separated list of excludes.
        /// Each time `--exclude=...` is specified, the comma-separated list of key-values will be
        /// checked against the labels of a project, and it will be ignored if all the key-values
        /// are present. In addition, specifying `--exclude` multiple times will create an "or"
        /// effect --- a project will be excluded if it satisfies one of the list of `--filters`.
        /// For example, `-e a=1,b=2 -e c=3` means projects with "a=1" and "b=2" at the same time,
        /// or "c=3", are excluded. Projects with only "a=1" or "b=2" are not excluded.
        ///
        /// This will always override `--filters`.
        #[clap(short, long)]
        excludes: Vec<String>,
    },

    /// Run .rhai scripts
    Run {
        scripts: Vec<String>,
        #[clap(short, long)]
        labels: Option<String>,
    },
}
