use std::{fs::File, io::Read};

use serde::{Deserialize, Serialize};
use serde_yaml::Value;

#[derive(Debug, Serialize, Deserialize)]
pub struct Package {
    pub name: String,
    pub version: String,
    pub description: String,
    pub license: String,
    pub build_scripts: Option<Vec<String>>,
}

impl Package {
    pub fn new(name: String, version: String, description: String, license: String) -> Package {
        Package {
            name,
            version,
            description,
            license,
            build_scripts: None,
        }
    }
    pub fn set_build_scripts(&mut self, build_scripts: Vec<String>) {
        self.build_scripts = Some(build_scripts);
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PkgFile {
    pub packages: Vec<Package>,
}

impl PkgFile {
    pub fn new() -> PkgFile {
        PkgFile {
            packages: Vec::new(),
        }
    }

    pub fn from_file(path: &str) -> PkgFile {
        let mut file = File::open(path).unwrap();
        let mut contents = String::new();
        file.read_to_string(&mut contents).unwrap();
        let value: Value = serde_yaml::from_str(&contents).unwrap();
        let mut packages = Vec::new();
        for dict in value.as_mapping().unwrap().iter() {
            let package: Package = serde_yaml::from_value(dict.1.clone()).unwrap();
            packages.push(package);
        }
        PkgFile { packages }
    }
}

#[cfg(test)]
mod test_super {
    use super::*;

    #[test]
    fn test_() {
        let pkg = PkgFile::from_file("tests/test.yml");
        println!("{:#?}", pkg);
    }
}
