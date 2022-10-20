use anyhow::{Context, Result};

use log::debug;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::io::ErrorKind;
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

#[derive(Deserialize, PartialEq, Eq, Serialize, Debug, Clone, Default)]
pub struct Project {
    pub rpm: Option<RpmBuild>,
    pub podman: Option<Docker>,
    pub docker: Option<Docker>,
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

#[derive(Deserialize, PartialEq, Eq, Serialize, Debug, Clone, Default)]
pub struct RpmBuild {
    pub spec: PathBuf,
    pub sources: Option<PathBuf>,
    pub package: Option<String>,
    pub pre_script: Option<PreScript>,
    pub post_script: Option<PostScript>,
    pub enable_scm: Option<bool>,
    pub scm_opts: Option<BTreeMap<String, String>>,
    pub config: Option<BTreeMap<String, String>>,
    pub mock_config: Option<String>,
    pub plugin_opts: Option<BTreeMap<String, String>>,
}

#[derive(Deserialize, PartialEq, Eq, Serialize, Debug, Clone, Default)]
pub struct Docker {
    pub image: BTreeMap<String, DockerImage>, // tag, file
}

#[derive(Deserialize, PartialEq, Eq, Serialize, Debug, Clone, Default)]
pub struct DockerImage {
    pub dockerfile: Option<String>,
    pub import: Option<PathBuf>,
    pub tag_latest: Option<bool>,
    pub context: String,
    pub version: Option<String>,
}

#[derive(Deserialize, PartialEq, Eq, Serialize, Debug, Clone)]
pub struct Flatpak {
    pub manifest: PathBuf,
    pub pre_script: Option<PreScript>,
    pub post_script: Option<PostScript>,
}

pub fn to_string(config: AndaConfig) -> Result<String> {
    let config = hcl::to_string(&config)?;
    Ok(config)
}

pub fn load_from_file(path: &PathBuf) -> Result<AndaConfig, ProjectError> {
    let file = fs::read_to_string(path).map_err(|e| match e.kind() {
        ErrorKind::NotFound => ProjectError::NoManifest,
        _ => ProjectError::InvalidManifest(e.to_string()),
    })?;

    let mut config = load_from_string(&file)?;
    debug!("Loading config from {}", path.display());

    // recursively merge configs

    // get parent path of config file
    let parent = if path.parent().unwrap().to_str().unwrap() == "" {
        PathBuf::from(".")
    } else {
        path.parent().unwrap().to_path_buf()
    };

    let walk = ignore::Walk::new(parent);

    for entry in walk {
        // debug!("Loading config from {:?}", entry);
        let entry = entry.unwrap();

        // check if path is same path as config file
        if entry.path().strip_prefix("./").unwrap() == path {
            continue;
        }

        if entry.file_type().unwrap().is_file() && entry.path().file_name().unwrap() == "anda.hcl" {
            let readfile = fs::read_to_string(entry.path())
                .map_err(|e| ProjectError::InvalidManifest(e.to_string()))?;

            let nested_config = prefix_config(
                load_from_string(&readfile)?,
                &entry
                    .path()
                    .parent()
                    .unwrap()
                    .strip_prefix("./")
                    .unwrap()
                    .display()
                    .to_string(),
            );
            // merge the btreemap
            config.project.extend(nested_config.project);
        }
    }

    debug!("Loaded config: {:#?}", config);
    //let config = config.map_err(ProjectError::HclError);

    check_config(config)
}

pub fn prefix_config(config: AndaConfig, prefix: &str) -> AndaConfig {
    let mut new_config = config.clone();

    for (project_name, project) in config.project.iter() {
        // set project name to prefix
        let new_project_name = format!("{}/{}", prefix, project_name);
        // modify project data
        let mut new_project = project.clone();

        if let Some(rpm) = &mut new_project.rpm {
            rpm.spec = PathBuf::from(format!("{}/{}", prefix, rpm.spec.display()));
            if let Some(sources) = &mut rpm.sources {
                *sources = PathBuf::from(format!("{}/{}", prefix, sources.display()));
            } else {
                rpm.sources = Some(PathBuf::from(prefix.to_string()));
            }
        }

        new_config.project.remove(project_name);
        new_config.project.insert(new_project_name, new_project);
    }

    new_config
}

pub fn load_from_string(config: &str) -> Result<AndaConfig, ProjectError> {
    let config = hcl::from_str(config).context("Failed to parse config file")?;
    check_config(config)
}

// Lints and checks the config for errors.
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
