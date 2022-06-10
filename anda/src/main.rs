use clap::{App, Arg, SubCommand, crate_authors, crate_description, crate_version};
use log4rs::*;
use log::{info, debug, trace, error};


fn main() {
    let app = App::new("anda")
        .version(crate_version!())
        .author(crate_authors!("\n"))
        .about(crate_description!())
        .arg(
            Arg::with_name("path")
                .help("The path to the package.")
                .default_value(".")
                .value_name("FILE"),
        )
        .setting(clap::AppSettings::SubcommandRequiredElseHelp)
        .subcommands([
            SubCommand::with_name("build").about("Builds a package from source"),
            SubCommand::with_name("buildsrc").about("Builds source RPM from a spec file"),
            SubCommand::with_name("bs").about("Runs build scripts"),
            SubCommand::with_name("koji_prepare").about("Prepares Koji build environment"),
            SubCommand::with_name("push")
                .about("Pushes a package to Koji")
                .args(&[
                    Arg::with_name("tag")
                        .help("The Koji tag to push")
                        .required(true),
                    Arg::with_name("branch")
                        .help("The branch to push to")
                        .default_value("same as tag"), // substitute default value
                    Arg::with_name("repo")
                        .short("r")
                        .long("repo")
                        .default_value("origin"),
                    Arg::with_name("prf")
                        .help("Koji Profile")
                        .default_value("ultramarine"),
                    Arg::with_name("scratch")
                        .short("s")
                        .long("scratch")
                        .default_value("False") // substitute default value
                        .possible_values(&["True", "False"])
                        .help("Uses scratch build"),
                    Arg::with_name("wait")
                        .short("w")
                        .long("wait")
                        .default_value("False") // substitute default value
                        .possible_values(&["True", "False"])
                        .help("Watch the Koji task"),
                ]),
            SubCommand::with_name("add")
                .about("Adds a package to Koji.")
                .arg(
                    Arg::with_name("tag")
                        .help("The Koji tag to add")
                        .required(true),
                ),
            SubCommand::with_name("init")
                .about("Initializes a umpkg project")
                .args(&[
                    Arg::with_name("name")
                        .help("Name of the project")
                        .required(true),
                    Arg::with_name("type")
                        .help("Type of the project")
                        .default_value("spec")
                        .possible_values(&["spec", "rust"]),
                ]),
            SubCommand::with_name("get")
                .about("Clone a git repo")
                .args(&[
                    Arg::with_name("repo")
                        .help("Repository name")
                        .required(true),
                    Arg::with_name("path")
                        .help("Output directory")
                        .default_value("repo name"),
                ]),
            SubCommand::with_name("setup").about("Sets up a umpkg development environment"),

            SubCommand::with_name("install")
                .about("Installs packages")
                .args(&[
                    Arg::with_name("packages")
                        .help("The packages to install")
                        .min_values(1)
                        .required(true),
                ]),
            SubCommand::with_name("uninstall")
                .about("Uninstalls packages")
                .alias("remove")
                .args(&[
                    Arg::with_name("packages")
                        .help("The packages to uninstall")
                        .min_values(1)
                        .required(true),
                ]),
        ]);
    let matches = app.get_matches();
    let path = matches.value_of("path").unwrap_or(".");
    info!("Path: {}", path);
    match matches.subcommand() {
        ("build", None) => {
            println!("Building package from source");
        }
        ("buildsrc", None) => {
            println!("Building source RPM from spec file");
        }
        ("bs", None) => {
            println!("Running build scripts");
        }
        ("koji_prepare", None) => {
            println!("Preparing Koji build environment");
        }
        ("push", Some(matches)) => {
            println!("Pushing package to Koji");
            let tag = matches.value_of("tag").unwrap();
            let mut branch = matches.value_of("branch").unwrap();
            let repo = matches.value_of("repo").unwrap();
            let prf = matches.value_of("prf").unwrap();
            let raw_scratch = matches.value_of("scratch").unwrap();
            let raw_wait = matches.value_of("wait").unwrap();
            if branch == "same as tag" {
                branch = tag;
            }
            let scratch = if raw_scratch == "True" { true } else { false };
            let wait = if raw_wait == "True" { true } else { false };
            println!("Tag: {}", tag);
            println!("Branch: {}", branch);
            println!("Repo: {}", repo);
            println!("Profile: {}", prf);
            println!("Scratch: {}", scratch);
            println!("Wait: {}", wait);
        }
        ("add", Some(matches)) => {
            println!("Adding package to Koji");
            let tag = matches.value_of("tag").unwrap();
            println!("Tag: {}", tag);
        }
        ("init", Some(matches)) => {
            println!("Initializing umpkg project");
            let name = matches.value_of("name").unwrap();
            let type_ = matches.value_of("type").unwrap();
            println!("Name: {}", name);
            println!("Type: {}", type_);
        }
        ("get", Some(matches)) => {
            println!("Cloning git repo");
            let repo = matches.value_of("repo").unwrap();
            let mut path = matches.value_of("path").unwrap();
            // I don't expect a repo called "repo name" so
            if path == "repo name" {
                path = repo;
            }
            println!("Repo: {}", repo);
            println!("Path: {}", path);
        }
        ("setup", None) => {
            println!("Setting up umpkg development environment");
        }

        ("install", Some(matches)) => {
            println!("Installing packages");
            let packages = matches.values_of("packages").unwrap();
            for pkg in packages {
                println!("Package: {}", pkg);
            }
        }
        ("uninstall", Some(matches)) => {
            println!("Uninstalling packages");
            let packages = matches.values_of("packages").unwrap();
            for pkg in packages {
                println!("Package: {}", pkg);
            }
        }
        _ => println!("No subcommand specified, run --help for more info"),
    }
}

mod tests {
    #[test]
    fn test_() {}
}
