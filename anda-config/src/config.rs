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
pub struct Manifest {
    pub project: BTreeMap<String, Project>,
    #[serde(default)]
    pub config: Config,
}


#[derive(Deserialize, Serialize, Debug, Clone, Default)]
pub struct Config {
    pub mock_config: Option<String>,
    pub strip_prefix: Option<String>,
    pub strip_suffix: Option<String>,
    pub project_regex: Option<String>,
}

impl Manifest {
    pub fn find_key_for_value(&self, value: &Project) -> Option<&String> {
        self.project.iter().find_map(|(key, val)| {
            if val == value {
                Some(key)
            } else {
                None
            }
        })
    }

    pub fn get_project(&self, key: &str) -> Option<&Project> {
        if let Some(project) = self.project.get(key) {
            Some(project)
        } else {
            // check for alias
            self.project.iter().find_map(|(_k, v)| {
                if let Some(alias) = &v.alias {
                    if alias.contains(&key.to_string()) {
                        Some(v)
                    } else {
                        None
                    }
                } else {
                    None
                }
            })
        }
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
    pub alias: Option<Vec<String>>,
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
    pub macros: Option<BTreeMap<String, String>>,
    pub opts: Option<BTreeMap<String, String>>,
    pub update: Option<String>,
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

pub fn to_string(config: Manifest) -> Result<String> {
    let config = hcl::to_string(&config)?;
    Ok(config)
}

pub fn load_from_file(path: &PathBuf) -> Result<Manifest, ProjectError> {
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
    generate_alias(&mut config);

    check_config(config)
}

pub fn prefix_config(config: Manifest, prefix: &str) -> Manifest {
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
    generate_alias(&mut new_config);
    new_config
}

pub fn generate_alias(config: &mut Manifest) {
    fn append_vec(vec: &mut Option<Vec<String>>, value: &str) {
        if let Some(vec) = vec {

            if vec.contains(&value.to_string()) {
                return;
            }

            vec.push(value.to_string());
        } else {
            *vec = Some(vec![value.to_string()]);
        }
    }

    for (name, project) in config.project.iter_mut() {
        
        if config.config.strip_prefix.is_some() || config.config.strip_suffix.is_some() {
            let mut new_name = name.clone();
            if let Some(strip_prefix) = &config.config.strip_prefix {
                new_name = new_name.strip_prefix(strip_prefix).unwrap_or(&new_name).to_string();
            }
            if let Some(strip_suffix) = &config.config.strip_suffix {
                new_name = new_name.strip_suffix(strip_suffix).unwrap_or(&new_name).to_string();
            }

            
            if name.clone() != new_name {
                append_vec(&mut project.alias, &new_name);
            }
        }
    }
}

pub fn load_from_string(config: &str) -> Result<Manifest, ProjectError> {
    let mut config: Manifest = hcl::eval::from_str(config, &crate::context::hcl_context())?;

    generate_alias(&mut config);

    check_config(config)
}

// Lints and checks the config for errors.
pub fn check_config(config: Manifest) -> Result<Manifest, ProjectError> {
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
        hello = "world"
        project "anda" {
            pre_script {
                commands = [
                    "echo '${env.RUST_LOG}'",
                ]
            }
        }
        "#;

        let body = hcl::parse(config).unwrap();

        print!("{:#?}", body);

        let config = load_from_string(config);

        println!("{:#?}", config);
    }
}
