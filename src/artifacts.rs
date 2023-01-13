use std::collections::BTreeMap;

use crate::cli::PackageType;

#[derive(Clone)]
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
