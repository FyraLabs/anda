use anyhow::{Context, Result};
use log::warn;
use serde::Deserialize;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use crate::error::ProjectError;

#[derive(Deserialize)]
pub struct AndaConfig {
    pub project: HashMap<String, Project>,
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

#[derive(Deserialize, PartialEq, Eq)]
pub struct Project {
    pub rpmbuild: Option<RpmBuild>,
    pub docker: Option<Docker>,
    pub pre_script: Option<PreScript>,
    pub script: Option<Script>,
    pub post_script: Option<PostScript>,
    pub rollback: Option<Script>,
    pub env: Option<Vec<String>>,
}
#[derive(Deserialize, PartialEq, Eq)]
pub struct Script {
    pub stage: HashMap<String, Stage>,
}

impl Script {
    pub fn get_stage(&self, name: &str) -> Option<&Stage> {
        self.stage.get(name)
    }
    pub fn find_key_for_value(&self, value: &Stage) -> Option<&String> {
        self.stage.iter().find_map(|(key, val)| {
            if val == value {
                Some(key)
            } else {
                None
            }
        })
    }
}

#[derive(Deserialize, Eq, PartialEq, Hash, PartialOrd, Ord)]
pub struct Stage {
    pub depends: Option<Vec<String>>,
    pub commands: Vec<String>,
}

#[derive(Deserialize, Eq, PartialEq, Hash)]
pub struct PreScript {
    pub commands: Vec<String>,
}

#[derive(Deserialize, Eq, PartialEq, Hash)]
pub struct PostScript {
    pub commands: Vec<String>,
}

#[derive(Deserialize, PartialEq, Eq)]
pub struct RpmBuild {
    pub spec: PathBuf,
    // serde default is standard
    /// Mode to use for the build.
    /// Default is `standard`. Builds an RPM normally from the spec file.
    /// `cargo-rpm` builds uses the `cargo-generate-rpm` crate to build an RPM from the Cargo.toml file, using templated values from the spec file.
    #[serde(default = "default_rpm_mode")]
    pub mode: RpmBuildMode,
    pub package: Option<String>,
    pub build_deps: Option<Vec<String>>,
}

fn default_rpm_mode() -> RpmBuildMode {
    RpmBuildMode::Standard
}
#[derive(Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum RpmBuildMode {
    Standard,
    Cargo,
}


#[derive(Deserialize, PartialEq, Eq)]
pub struct Docker {
    pub image: HashMap<String, DockerImage>, // tag, file
}

#[derive(Deserialize, PartialEq, Eq)]
pub struct DockerImage {
    pub dockerfile: Option<PathBuf>,
    pub import: Option<PathBuf>,
    pub tag_latest: Option<bool>,
    pub workdir: PathBuf,
    pub version: Option<String>,
}

pub fn load_config(root: &PathBuf) -> Result<AndaConfig, ProjectError> {
    let config_path = root;

    if !config_path.exists() {
        return Err(ProjectError::NoManifest);
    }

    let config: Result<AndaConfig, hcl::error::Error> = hcl::from_str(
        std::fs::read_to_string(config_path)
            .with_context(|| {
                format!(
                    "could not read `anda.toml` in directory {}",
                    fs::canonicalize(root.parent().unwrap()).unwrap().display()
                )
            })?
            .as_str(),
    );

    let config = config.map_err(ProjectError::HclError);

    check_config(config?)
}

/// Lints and checks the config for errors.
pub fn check_config(config: AndaConfig) -> Result<AndaConfig, ProjectError> {
    let mut errors = vec![];

    for (key, value) in &config.project {
        if value.rpmbuild.is_none() && value.docker.is_none() && value.script.is_none() {
            warn!("project {} has no build manifest!", key);
        }
        if let Some(docker) = &value.docker {
            if docker.image.is_empty() {
                errors.push(ProjectError::InvalidManifest(format!(
                    "project {} has no docker images",
                    key
                )));
            }

            for (tag, image) in &docker.image {
                if image.dockerfile.is_none() && image.import.is_none() {
                    errors.push(ProjectError::InvalidManifest(format!(
                        "project {} has no dockerfile or import for image {}",
                        key, tag
                    )));
                }
            }
        }
    }
    Ok(config)
}
