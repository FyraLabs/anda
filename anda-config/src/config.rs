use anyhow::{Context, Result};
use log::{trace, warn};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::PathBuf;

use crate::error::ProjectError;

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct ProjectData {
    pub manifest: HashMap<String, String>,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct AndaConfig {
    pub project: BTreeMap<String, Project>,
}

impl AndaConfig {
    pub fn find_key_for_value(&self, value: &Project) -> Option<&String> {
        self.project.iter().find_map(|(key, val)| {
            if val == value {
                Some(key)
            } else {
                None
            }
        })
    }
}

#[derive(Deserialize, PartialEq, Eq, Serialize, Debug, Clone)]
pub struct Project {
    pub rpm: Option<RpmBuild>,
    pub docker: Option<Docker>,
    pub podman: Option<Docker>,
    pub flatpak: Option<Flatpak>,
    pub pre_script: Option<PreScript>,
    pub post_script: Option<PostScript>,
    pub env: Option<BTreeMap<String, String>>,
}
#[derive(Deserialize, Eq, PartialEq, Hash, PartialOrd, Ord, Serialize, Debug, Clone)]
pub struct PreScript {
    pub commands: Vec<String>,
}

#[derive(Deserialize, Eq, PartialEq, Hash, Serialize, Debug, Clone)]
pub struct PostScript {
    pub commands: Vec<String>,
}

#[derive(Deserialize, PartialEq, Eq, Serialize, Debug, Clone)]
pub struct RpmBuild {
    pub spec: PathBuf,

    pub sources: Option<PathBuf>,

    pub package: Option<String>,

    pub pre_script: Option<PreScript>,
    pub post_script: Option<PostScript>,
}

#[derive(Deserialize, PartialEq, Eq, Serialize, Debug, Clone)]
pub struct Docker {
    pub image: BTreeMap<String, DockerImage>, // tag, file
}

#[derive(Deserialize, PartialEq, Eq, Serialize, Debug, Clone)]
pub struct DockerImage {
    pub dockerfile: Option<PathBuf>,
    pub import: Option<PathBuf>,
    pub tag_latest: Option<bool>,
    pub workdir: PathBuf,
    pub version: Option<String>,
}


#[derive(Deserialize, PartialEq, Eq, Serialize, Debug, Clone)]
pub struct Flatpak {
    pub manifest: Option<PathBuf>,
    pub pre_script: Option<PreScript>,
    pub post_script: Option<PostScript>,
}

pub fn load_from_file(path: &PathBuf) -> Result<AndaConfig, ProjectError> {
    let file = fs::read_to_string(path).context("Failed to read config file")?;

    let config = hcl::from_str(&file).context("Failed to parse config file")?;

    //let config = config.map_err(ProjectError::HclError);

    check_config(config)
}

pub fn load_from_string(config: &str) -> Result<AndaConfig, ProjectError> {
    let config = hcl::from_str(config).context("Failed to parse config file")?;
    check_config(config)
}

/// Lints and checks the config for errors.
pub fn check_config(config: AndaConfig) -> Result<AndaConfig, ProjectError> {
    // do nothing for now
    Ok(config)
}

#[cfg(test)]
mod test_parser {
    use super::*;

    #[test]
    fn test_parse() {
        // set env var
        std::env::set_var("RUST_LOG", "trace");
        env_logger::init();
        let config = r#"
        project "anda" {
            pre_script {
                commands = ["echo 'hello'"]
            }
            env = {
                TEST = "test"
            }
        }
        "#;

        let body = hcl::parse(config).unwrap();

        print!("{:#?}", body);

        let config: AndaConfig = hcl::from_str(config).unwrap();

        println!("{:#?}", config);
    }
}
