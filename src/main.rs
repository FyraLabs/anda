//! Andaman, a package build toolchain for RPM, OCI and Flatpak.
#![allow(clippy::module_name_repetitions)]
#![deny(rust_2018_idioms)]

mod artifacts;
mod builder;
mod cli;
mod flatpak;
mod oci;
mod rpm_spec;
mod update;
mod util;
use anda_config::parse_labels;
use clap::{CommandFactory, Parser};
use clap_complete::generate;
use cli::{Cli, Command};
use color_eyre::{eyre::eyre, Result};
use std::{io, mem::take};
use tracing::{debug, trace};

#[allow(clippy::unwrap_in_result)]
#[allow(clippy::missing_errors_doc)]
#[allow(clippy::missing_panics_doc)]
#[tokio::main]
async fn main() -> Result<()> {
    color_eyre::install()?;
    let mut cli = Cli::parse();
    let mut app = Cli::command();
    app.build();

    tracing_log::LogTracer::init()?;
    let subscriber = tracing_subscriber::FmtSubscriber::builder()
        .with_max_level(util::convert_filter(cli.verbose.log_level_filter()))
        .event_format(tracing_subscriber::fmt::format().pretty())
        .finish();
    tracing::subscriber::set_global_default(subscriber).expect("setting default subscriber failed");

    trace!("Matching subcommand");
    match cli.command {
        Command::Build {
            all,
            ref mut project,
            ref mut package,
            ref mut rpm_opts,
            ref mut flatpak_opts,
            ref mut oci_opts,
        } => {
            if project.is_none() && !all {
                // print help
                let mut app = Cli::command();
                let a = app.find_subcommand_mut("build").unwrap();
                let mut a = take(a).display_name("anda-build").name("anda-build");
                a.print_help()?;
                return Err(eyre!("No project specified, and --all not specified."));
            }

            let project = take(project);
            let package = std::mem::replace(package, cli::PackageType::Rpm);
            let flatpak_opts = take(flatpak_opts);
            let oci_opts = take(oci_opts);
            let rpm_opts = take(rpm_opts);
            debug!("{all:?}");
            builder::builder(&cli, rpm_opts, all, project, package, flatpak_opts, oci_opts).await?;
        }
        Command::Clean => {
            println!("Cleaning up build directory");
            let clean = std::fs::remove_dir_all(&cli.target_dir);
            if let Err(e) = clean {
                // match the errors
                match e.kind() {
                    std::io::ErrorKind::NotFound => {}
                    e => println!("Error cleaning up build directory: {e:?}"),
                }
            }
        }

        Command::List => {
            let config = anda_config::load_from_file(&cli.config)?;

            for (project_name, project) in &config.project {
                let project_alias = project
                    .alias
                    .as_ref()
                    .map_or_else(String::new, |alias| format!(" ({})", alias.join(", ")));

                println!("{project_name}{project_alias}");
            }

            trace!("{config:#?}");
        }
        Command::Init { path, yes } => {
            // create a new project
            debug!("Creating new project in {}", path.display());
            util::init(path.as_path(), yes)?;
        }
        Command::Completion { shell } => {
            generate(shell, &mut cli::Cli::command(), "anda", &mut io::stdout());
        }
        Command::CI => {
            let config = anda_config::load_from_file(&cli.config)?;
            let entries = util::fetch_build_entries(config);

            println!("build_matrix={}", serde_json::to_string(&entries)?);
        }
        Command::Update { labels, filters, excludes, project } => {
            let labels = parse_labels(labels.iter().map(std::ops::Deref::deref))
                .ok_or_else(|| eyre!("Cannot parse --labels"))?;
            let filters = anda_config::parse_filters(&filters)
                .ok_or_else(|| eyre!("Cannot parse --filters"))?;
            let excludes = anda_config::parse_filters(&excludes)
                .ok_or_else(|| eyre!("Cannot parse --excludes"))?;
            let cfg = anda_config::load_from_file(&cli.config)?;
            if let Some(proj) = project {
                update::single_update(&cfg, &proj, &labels, filters)?;
            } else {
                update::update(cfg, labels, filters, excludes)?;
            }
        }
        Command::Run { scripts, labels } => {
            if scripts.is_empty() {
                return Err(eyre!("No scripts to run"));
            }
            let labels = parse_labels(labels.iter().map(std::ops::Deref::deref))
                .ok_or_else(|| eyre!("Cannot parse --labels"))?;
            update::run_scripts(&scripts, labels)?;
        }
    }
    Ok(())
}
