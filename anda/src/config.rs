use anyhow::{Context, Result};
use serde_derive::Deserialize;
use std::path::PathBuf;
use std::fs;

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

pub fn load_config(root: &PathBuf) -> Result<AndaConfig> {
    let config_path = root.join("anda.toml");
    let config: AndaConfig = toml::from_str(
        std::fs::read_to_string(config_path)
            .with_context(|| {
                format!(
                    "could not read `anda.toml` in directory {}",
                    fs::canonicalize(root).unwrap().display()
                )
            })?
            .as_str(),
    )?;

    Ok(config)
}
