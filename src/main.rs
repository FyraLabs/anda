#![deny(rust_2018_idioms)]

mod artifacts;
mod builder;
mod cli;
mod flatpak;
mod oci;
mod rpm_spec;
mod util;

use anyhow::Result;

use clap::{CommandFactory, Parser};
use cli::{Cli, Command};
use log::debug;

#[tokio::main]
async fn main() -> Result<()> {
    //println!("Hello, world!");
    let cli = Cli::parse();

    let mut app = Cli::command();

    app.build();

    // let sub = app.get_subcommands();

    // for s in sub {
    //     println!("{:?}", s);
    // }

    // let app = Command::command().find_subcommand("build").unwrap().clone();
    // clap_mangen::Man::new(app).render(&mut std::io::stdout()).unwrap();
    // println!("{:?}", &cli);

    // set up logging according to verbosity level
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
                // print help for build subcommand
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
            // load config
            let config = anda_config::load_from_file(&cli.config).unwrap();

            // list projects

            for (project_name, _project) in config.project.iter() {
                println!("{}", project_name);
            }

            debug!("{:#?}", &config);

            // println!("Listing projects");
        }
        Command::Init { path, yes } => {
            // create a new project
            debug!("Creating new project in {}", path.display());
            util::init(path.as_path(), yes)?;
        }
    }
    Ok(())
}
