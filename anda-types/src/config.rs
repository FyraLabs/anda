use anyhow::{Context, Result};
use log::{debug, trace, warn};
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
    pub metadata: Option<BTreeMap<String,String>>,
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

    let config = load_from_file(config_path);

    check_config(config?)
}

pub fn load_from_file(path: &PathBuf) -> Result<AndaConfig, ProjectError> {
    let config = parse_config(
        std::fs::read_to_string(path)
            .with_context(|| {
                format!(
                    "could not read `anda.hcl` in directory {}",
                    fs::canonicalize(path.parent().unwrap()).unwrap().display()
                )
            })?
            .as_str(),
    );

    //let config = config.map_err(ProjectError::HclError);

    check_config(config?)
}

/// Parses the config file using a custom basic expression engine
/// This is used as a drop-in replacement for the WIP expression engine in hcl-rs
pub fn parse_config(config: &str) -> Result<AndaConfig, ProjectError> {
    let mut config = hcl::parse(config).map_err(ProjectError::HclError)?;

    config = all_blocks(&mut config);

    let str = hcl::to_string(&config)?;
    // deserialize the config
    // very hacky and slow, but it works and i dont know how to serialize body into struct
    let config: Result<AndaConfig, hcl::error::Error> = hcl::from_str(str.as_str());
    //config.

    //println!("{:#?}", config);
    config.map_err(ProjectError::HclError)
}

fn all_blocks(body: &mut hcl::Body) -> hcl::Body {
    for block in body.blocks_mut() {
        //println!("{:#?}", block);

        for attr in block.body.attributes_mut() {
            //println!("{:#?}", attr.expr);

            attr.expr = expr_parse(&attr.expr).unwrap();
            //expr_parse(&attr.expr);
        }
        all_blocks(&mut block.body);
    }
    body.to_owned()
}

fn expr_parse(expr: &hcl::Expression) -> Result<hcl::Expression> {
    let mut test_project_data = HashMap::new();
    test_project_data.insert("test".to_string(), "test".to_string());
    test_project_data.insert("commit_id".to_string(), "test2".to_string());

    let project = ProjectData {
        manifest: test_project_data,
    };
    match &expr {
        hcl::Expression::Array(array) => {
            trace!("array: {:#?}", expr);
            for expr in array {
                expr_parse(expr)?;
            }
            Ok(expr.to_owned())
        }
        hcl::Expression::Raw(raw) => {
            trace!("raw: {:#?}", raw);
            let string = raw.to_string();
            // string is ${expression}, we grab the expression
            let mut string = string
                .strip_prefix("${")
                .unwrap()
                .strip_suffix('}')
                .unwrap()
                .to_string();

            if string.starts_with("project.") {
                let index = string.strip_prefix("project.").unwrap();
                // check if key exists
                if project.manifest.contains_key(index) {
                    string = project.manifest[index].to_owned();
                } else {
                    return Ok(expr.to_owned());
                }

                trace!(
                    "replaced raw expression {:?} to {string}",
                    raw.to_string(),
                    string = string
                );
                let expr = hcl::Expression::String(string);
                trace!("expr: {:#?}", expr);
                return Ok(expr);
            } // parse the expression

            Ok(expr.to_owned())
        }
        hcl::Expression::String(string) => {
            trace!("string: {:#?}", string);

            // regex: find all ${} expressions
            let re = regex::Regex::new(r"\$\{.*?\}").unwrap();
            let mut string = string.to_owned();
            // print all matches
            re.captures_iter(string.clone().as_str()).for_each(|cap| {
                trace!("cap: {:#?}", cap);
                let e = cap[0].to_string();
                // string is ${expression}, we grab the expression
                let expr = e
                    .strip_prefix("${")
                    .unwrap()
                    .strip_suffix('}')
                    .unwrap()
                    .to_string();
            
                trace!("expr: {:#?}", expr);

                // replace the expression with the value

                if expr.starts_with("project.") {
                    let index = expr.strip_prefix("project.").unwrap();
                    // check if key exists
                    if project.manifest.contains_key(index) {
                        let value = project.manifest[index].to_owned();
                        string = string.replace(&e, &value);
                        trace!("replaced expression {e} to {string}", e = e, string = value);
                    }
                } // parse the expression

                //string = string.replace(e.as_str(), expr.as_str());
                trace!("string: {:#?}", string);
            });

            let expr = hcl::Expression::String(string);
            trace!("expr: {:#?}", expr);
            Ok(expr)
        }
        _ => {
            trace!("expr: {:#?}", expr);
            Ok(expr.to_owned())
        }
    }
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
            if rpmbuild.mode == RpmBuildMode::Standard && rpmbuild.spec.is_none() {
                errors.push(ProjectError::InvalidManifest(format!(
                    "project {} has no spec file or package for rpm build",
                    key
                )));
            }

            if rpmbuild.mode == RpmBuildMode::Standard && !rpmbuild.spec.as_ref().unwrap().exists()
            {
                errors.push(ProjectError::InvalidManifest(format!(
                    "spec file {} does not exist for project {}",
                    rpmbuild.spec.as_ref().unwrap().display(),
                    key
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
            script {
                stage "build" {
                    commands = [
                        "echo Commit ID: ${project.commit_id}",
                        project.test
                    ]
                }
            }
            env = {
                TEST = "test"
            }        
        }
        
                
        "#;
        let config = parse_config(config);
    }
}
