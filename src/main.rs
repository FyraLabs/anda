//! Andaman, a package build toolchain for RPM, OCI and Flatpak.
#![allow(clippy::module_name_repetitions)]
#![deny(rust_2018_idioms)]

mod artifacts;
mod builder;
mod cli;
mod flatpak;
mod oci;
mod pt;
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
#[allow(clippy::too_many_lines)]
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
            ref label_filters,
            package,
            ref rpm_opts,
            ref flatpak_opts,
            ref oci_opts,
        } => {
            if !all && !label_filters.is_empty() {
                return Err(eyre!("--filters and --excludes require --all"));
            }
            let target = if all {
                builder::BuildTarget::All(parse_label_filter(label_filters)?)
            } else if let Some(project) = take(project) {
                builder::BuildTarget::Project(project)
            } else {
                let mut app = Cli::command();
                let a = app.find_subcommand_mut("build").unwrap();
                let mut a = take(a).display_name("anda-build").name("anda-build");
                a.print_help()?;
                return Err(eyre!("No project specified, and --all not specified."));
            };
            builder::builder(&cli, target, package, rpm_opts, flatpak_opts, oci_opts).await?;
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
        Command::CI { arches, unique_packages } => {
            let config = anda_config::load_from_file(&cli.config)?;
            let entries = util::configure_build_entries(
                util::fetch_build_entries(&config, &cli.config),
                &arches,
                unique_packages,
            );
            println!("build_matrix={}", serde_json::to_string(&entries)?);
            println!("build_units={}", serde_json::to_string(&util::group_build_entries(entries))?);
        }
        Command::Update { labels, label_filters, project } => {
            let labels = parse_labels(labels.iter().map(std::ops::Deref::deref))
                .ok_or_else(|| eyre!("Cannot parse --labels"))?;
            let filter = parse_label_filter(&label_filters)?;
            let cfg = anda_config::load_from_file(&cli.config)?;
            if let Some(proj) = project {
                update::single_update(&cfg, &proj, &labels, filter.filters)?;
            } else {
                update::update(cfg, &labels, &filter)?;
            }
        }
        Command::Run { scripts, labels } => {
            if scripts.is_empty() {
                return Err(eyre!("No scripts to run"));
            }
            let labels = parse_labels(labels.iter().map(std::ops::Deref::deref))
                .ok_or_else(|| eyre!("Cannot parse --labels"))?;
            update::run_scripts(&scripts, &labels)?;
        }
    }
    Ok(())
}

fn parse_label_filter(args: &cli::LabelFilters) -> Result<anda_config::LabelFilter> {
    Ok(anda_config::LabelFilter {
        filters: anda_config::parse_filters(&args.filters)
            .ok_or_else(|| eyre!("Cannot parse --filters"))?,
        excludes: anda_config::parse_filters(&args.excludes)
            .ok_or_else(|| eyre!("Cannot parse --excludes"))?,
    })
}
