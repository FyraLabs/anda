use anyhow::{Context, Result};
use serde::Deserialize;
use std::fs;
use std::path::PathBuf;

use crate::error::ProjectError;

#[derive(Deserialize)]
pub struct AndaConfig {
    pub package: Package,
}

#[derive(Deserialize)]
pub struct Package {
    pub spec: PathBuf,
    pub name: String,
    pub description: Option<String>,
}

pub fn load_config(root: &PathBuf) -> Result<AndaConfig, ProjectError> {
    let config_path = root.join("anda.toml");

    if !config_path.exists() {
        return Err(ProjectError::NoManifest);
    }

    let config = toml::from_str(
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
        Err(e) => Err(ProjectError::InvalidManifest),
    }

}
