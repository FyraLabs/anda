
use std::str::FromStr;

use clap::{AppSettings, ArgEnum, Parser, Subcommand, ValueEnum};

#[derive(Copy, Clone, ValueEnum, Debug)]
pub enum PackageType {
    Rpm,
    Docker,
    Podman,
    Flatpak,
    RpmOstree
}

impl FromStr for PackageType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "rpm" => Ok(PackageType::Rpm),
            "docker" => Ok(PackageType::Docker),
            "podman" => Ok(PackageType::Podman),
            "flatpak" => Ok(PackageType::Flatpak),
            "rpm-ostree" => Ok(PackageType::RpmOstree),
            _ => Err(format!("Invalid package type: {}", s)),
        }
    }
}