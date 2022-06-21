use clap::{App, Arg, SubCommand, arg, Command,};
use log4rs::*;
use log::{info, debug, trace, error};


fn main() {
    let mut app = App::new("anda")
        .version(env!("CARGO_PKG_VERSION"))
        .author(env!("CARGO_PKG_AUTHORS"))
        .about(env!("CARGO_PKG_DESCRIPTION"))
        .arg(
            Arg::with_name("path")
                .help("The path to the package.")
                .default_value(".")
                .value_name("FILE"),
        )
        //.setting(clap::AppSettings::SubcommandRequiredElseHelp)
        .subcommand(
            Command::new("install")
                .about("Install a package")
                // Allow multiple packages to be specified
                .arg(arg!(<PACKAGES>... "The packages to install")
            .multiple_values(true))
                .arg_required_else_help(true),
        )
        .subcommand(
            Command::new("remove")
                .about("Remove a package")
                .arg(arg!(<PACKAGES> "The package to remove"))
                .arg_required_else_help(true),
        );



    let matches = app.clone().get_matches();

    match matches.subcommand() {
        Some(("install", sub_matches)) => {
            let packages = sub_matches.values_of("PACKAGES").unwrap().collect::<Vec<_>>();
            println!("Installing {}", packages.join(", "));
        }
        Some(("remove", sub_matches)) => {
            let packages = sub_matches.values_of("PACKAGES").unwrap().collect::<Vec<_>>();
            println!("Removing {}", packages.join(", "));
        }
        Some(_) => todo!(),

        // print help if no subcommand is used
        None => app.print_help().unwrap(),
    }

}

mod tests {
    #[test]
    fn test_() {}
}
