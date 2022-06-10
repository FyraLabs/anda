use std::{fs::File, io::Read};

use serde::{Deserialize, Serialize};
use serde_yaml::{Value};


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
    // Load a package from a yaml file
    pub fn load_from_yaml(path: &str) -> Package {
        let mut file = File::open(path).unwrap();
        let mut contents = String::new();
        file.read_to_string(&mut contents).unwrap();
        let value: Value = serde_yaml::from_str(&contents).unwrap();
        let package: Package = serde_yaml::from_value(value).unwrap();
        package
    }
}

#[cfg(test)]
mod test_super {
    use super::*;

    #[test]
    fn test_() {
        let pkg = Package::load_from_yaml("tests/test.yml");
        println!("{:#?}", pkg);
    }
}
