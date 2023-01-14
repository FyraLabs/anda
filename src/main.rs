#![deny(rust_2018_idioms)]

mod artifacts;
mod builder;
mod cli;
mod flatpak;
mod oci;
mod rpm_spec;
mod update;
mod util;
use anda_config::parse_map;
use clap::{CommandFactory, Parser};
use clap_complete::generate;
use cli::{Cli, Command};
use color_eyre::{eyre::eyre, Result};
use std::io;
use tracing::debug;

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let mut app = Cli::command();
    app.build();

    let subscriber = tracing_subscriber::FmtSubscriber::builder()
        .with_max_level(util::convert_filter(cli.verbose.log_level_filter()))
        .event_format(tracing_subscriber::fmt::format().pretty())
        .finish();
    tracing::subscriber::set_global_default(subscriber).expect("setting default subscriber failed");

    let cmd = cli.command.to_owned();

    match cmd {
        Command::Build { all, project, package, rpm_opts, flatpak_opts, oci_opts } => {
            if project.is_none() && !all {
                // print help
                let mut app = Cli::command();
                let mut a = app
                    .find_subcommand_mut("build")
                    .unwrap()
                    .clone()
                    .display_name("anda-build")
                    .name("anda-build");
                a.print_help().unwrap();
                return Err(eyre!("No project specified, and --all not specified."));
            }

            debug!("{all:?}");
            builder::builder(&cli, rpm_opts, all, project, package, flatpak_opts, oci_opts).await?;
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

        Command::List => {
            let config = anda_config::load_from_file(&cli.config).unwrap();

            for (project_name, project) in config.project.iter() {
                let project_alias = if let Some(alias) = &project.alias {
                    format!(" ({})", alias.join(", "))
                } else {
                    "".to_string()
                };

                println!("{project_name}{project_alias}");
            }

            debug!("{config:#?}");
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
            let config = anda_config::load_from_file(&cli.config).unwrap();
            let entries = util::fetch_build_entries(config)?;

            println!("build_matrix={}", serde_json::to_string(&entries)?);
        }
        Command::Update { labels, filters } => {
            let labels = parse_map(&labels.unwrap_or_default());
            let filters = parse_map(&filters.unwrap_or_default());
            update::update_rpms(
                anda_config::load_from_file(&cli.config).unwrap(),
                labels.ok_or_else(|| eyre!("Cannot parse --labels"))?,
                filters.ok_or_else(|| eyre!("Cannot parse --labels"))?,
            )?;
        }
        Command::Run { scripts, labels } => {
            if scripts.is_empty() {
                return Err(eyre!("No scripts to run"));
            }
            let labels = parse_map(&labels.unwrap_or_default());
            update::run_scripts(&scripts, labels.ok_or_else(|| eyre!("Cannot parse --labels"))?)?;
        }
    }
    Ok(())
}
