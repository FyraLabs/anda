
use std::{str::FromStr, collections::BTreeMap};

use clap::{AppSettings, ArgEnum, Parser, Subcommand, ValueEnum};

#[derive(Copy, Clone, ValueEnum, Debug)]
pub enum PackageType {
    Rpm,
    Docker,
    Podman,
    Flatpak,
    RpmOstree,
    All,
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
            "all" => Ok(PackageType::All),
            _ => Err(format!("Invalid package type: {}", s)),
        }
    }
}

pub struct Artifacts {
    pub packages: BTreeMap<String, PackageType>,
}

impl Artifacts {
    pub fn new() -> Self {
        Artifacts {
            packages: BTreeMap::new(),
        }
    }
    pub fn add(&mut self, name: String, package_type: PackageType) {
        self.packages.insert(name, package_type);
    }
}
