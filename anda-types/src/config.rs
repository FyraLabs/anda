use anyhow::{Context, Result};
use log::warn;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use crate::error::ProjectError;

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
    pub image: Option<String>,
    pub rpmbuild: Option<RpmBuild>,
    pub docker: Option<Docker>,
    pub pre_script: Option<PreScript>,
    pub script: Option<Script>,
    pub post_script: Option<PostScript>,
    pub rollback: Option<Script>,
    pub env: Option<BTreeMap<String, String>>,
}
#[derive(Deserialize, PartialEq, Eq, Serialize, Debug, Clone)]
pub struct Script {
    pub stage: BTreeMap<String, Stage>,
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

#[derive(Deserialize, Eq, PartialEq, Hash, PartialOrd, Ord, Serialize, Debug, Clone)]
pub struct Stage {
    pub image: Option<String>,
    pub depends: Option<Vec<String>>,
    pub commands: Vec<String>,
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
    /// Image to build RPMs with
    /// If not specified, the image of the project is used
    pub image: Option<String>,
    pub spec: Option<PathBuf>,
    // serde default is standard
    /// Mode to use for the build.
    /// Default is `standard`. Builds an RPM normally from the spec file.
    /// `cargo-rpm` builds uses the `cargo-generate-rpm` crate to build an RPM from the Cargo.toml file
    #[serde(default = "default_rpm_mode")]
    pub mode: RpmBuildMode,
    pub package: Option<String>,
    /// Internal project dependencies
    pub project_depends: Option<Vec<String>>,
    pub build_deps: Option<Vec<String>>,

    pub pre_script: Option<PreScript>,
    pub post_script: Option<PostScript>,
}

fn default_rpm_mode() -> RpmBuildMode {
    RpmBuildMode::Standard
}
#[derive(Deserialize, PartialEq, Eq, Serialize, Debug, Clone)]
#[serde(rename_all = "kebab-case")]
pub enum RpmBuildMode {
    Standard,
    Cargo,
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

pub fn load_config(root: &PathBuf) -> Result<AndaConfig, ProjectError> {
    let config_path = root;

    if !config_path.exists() {
        return Err(ProjectError::NoManifest);
    }

    let config: Result<AndaConfig, hcl::error::Error> = hcl::from_str(
        std::fs::read_to_string(config_path)
            .with_context(|| {
                format!(
                    "could not read `anda.hcl` in directory {}",
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
        if let Some(rpmbuild) = &value.rpmbuild {
            
            if rpmbuild.mode == RpmBuildMode::Standard && rpmbuild.spec.is_none() || rpmbuild.mode == RpmBuildMode::Cargo && rpmbuild.package.is_none() {
                errors.push(ProjectError::InvalidManifest(format!(
                    "project {} has no spec file or package for rpm build",
                    key
                )));
            }

            if rpmbuild.mode == RpmBuildMode::Standard && !rpmbuild.spec.as_ref().unwrap().exists() {
                errors.push(ProjectError::InvalidManifest(format!(
                    "spec file {} does not exist for project {}",
                    rpmbuild.spec.as_ref().unwrap().display(), key
                )));
            }
            

            if let Some(projects) = &rpmbuild.project_depends {
                for project in projects {
                    if !config.project.contains_key(project) {
                        errors.push(ProjectError::InvalidManifest(format!(
                            "project `{}` depends on project `{}` for RPMs, which does not exist",
                            key, project
                        )));
                    }
                }
            }
        }
    }
    if !errors.is_empty() {
        return Err(ProjectError::Multiple(errors));
    }

    Ok(config)
}
