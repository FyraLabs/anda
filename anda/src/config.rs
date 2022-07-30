use anyhow::{Context, Result};
use serde::Deserialize;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use crate::error::ProjectError;

#[derive(Deserialize)]
pub struct AndaConfig {
    pub project: HashMap<String, Project>,
}

#[derive(Deserialize)]
pub struct Project {
    pub proj_type: String,
    pub spec: Option<PathBuf>,
    pub dockerfile: Option<PathBuf>,
    pub scripts: Option<Vec<Script>>,
    // FIXME: Option types are currently not supported in hcl-rs
    // This will fail unless upstream fixes thisq
}
#[derive(Deserialize)]
pub struct Script {
    pub name: String,
    pub command: String,
}

pub fn load_config(root: &PathBuf) -> Result<AndaConfig, ProjectError> {
    let config_path = root.join("anda.hcl");

    if !config_path.exists() {
        return Err(ProjectError::NoManifest);
    }

    let config: Result<AndaConfig, hcl::error::Error> = hcl::from_str(
        std::fs::read_to_string(config_path)
            .with_context(|| {
                format!(
                    "could not read `anda.toml` in directory {}",
                    fs::canonicalize(root).unwrap().display()
                )
            })?
            .as_str(),
    );

    match config {
        Ok(config) => Ok(config),
        Err(e) => Err(ProjectError::InvalidManifest(e)),
    }
}
