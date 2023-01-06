#![deny(rust_2018_idioms)]

mod artifacts;
mod builder;
mod cli;
mod flatpak;
mod oci;
mod rpm_spec;
mod update;
mod util;

use std::io;

use anyhow::Result;

use clap::{CommandFactory, Parser};
use clap_complete::generate;
use cli::{Cli, Command};
use log::debug;
use util::fetch_build_entries;

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let mut app = Cli::command();
    app.build();

    pretty_env_logger::formatted_builder()
        .filter_level(cli.verbose.log_level_filter())
        .init();

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
                let mut a = app
                    .find_subcommand_mut("build")
                    .unwrap()
                    .clone()
                    .display_name("anda-build")
                    .name("anda-build");
                a.print_help().unwrap();
                return Err(anyhow::anyhow!(
                    "No project specified, and --all not specified."
                ));
            }

            debug!("{:?}", &all);
            builder::builder(
                &cli,
                rpm_opts,
                all,
                project,
                package,
                flatpak_opts,
                oci_opts,
            )
            .await?;
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

                println!("{}{}", project_name, project_alias);
            }

            debug!("{:#?}", &config);
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
            let entries = fetch_build_entries(config)?;

            println!("build_matrix={}", serde_json::to_string(&entries)?);
        }
        Command::Update => {
            update::update_rpms(anda_config::load_from_file(&cli.config).unwrap())?;
        }
    }
    Ok(())
}
