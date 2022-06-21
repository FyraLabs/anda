use clap::{App, Arg, SubCommand, arg, Command,};
use log4rs::*;
use log::{info, debug, trace, error};


fn main() {
    let app = App::new("anda")
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
                .arg(arg!(<PACKAGE> "The package to install"))
                .arg_required_else_help(true),
        )
        .subcommand(
            Command::new("remove")
                .about("Remove a package")
                .arg(arg!(<PACKAGE> "The package to remove"))
                .arg_required_else_help(true),
        );


    let matches = app.get_matches();

    match matches.subcommand() {
        Some(("install", sub_matches)) => {
            let package = sub_matches.value_of("PACKAGE").unwrap();
            println!("Installing {}", package);
        }
        Some(("remove", sub_matches)) => {
            let package = sub_matches.value_of("PACKAGE").unwrap();
            println!("Removing {}", package);
        }
        Some(_) => todo!(),
        None => todo!(),
    }

}

mod tests {
    #[test]
    fn test_() {}
}
