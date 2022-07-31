use anyhow::{anyhow, Result};
use execute::Execute;
use log::{debug, error, info, warn};
use mime_guess::MimeGuess;
use owo_colors::OwoColorize;
use reqwest::{multipart, ClientBuilder};
use serde::Serialize;
use solvent::DepGraph;
use std::{
    collections::HashMap,
    env,
    io::{BufRead, BufReader},
    path::PathBuf,
    process::{Command, ExitStatus, Stdio},
};
use tokio::{fs::File, io::AsyncReadExt};
use walkdir::WalkDir;

use crate::{config::Project, error::BuilderError};

trait ExitOkPolyfill {
    fn exit_ok_polyfilled(&self) -> Result<()>;
}

impl ExitOkPolyfill for ExitStatus {
    fn exit_ok_polyfilled(&self) -> Result<()> {
        if self.success() {
            Ok(())
        } else {
            Err(anyhow!("process exited with non-zero status"))
        }
    }
}

#[derive(Debug, Clone, Serialize)]
struct ArtifactUploader {
    pub files: HashMap<String, PathBuf>,
}

impl ArtifactUploader {
    pub fn new(files: HashMap<String, PathBuf>) -> Self {
        Self { files }
    }

    pub async fn upload(&self) -> Result<()> {
        let endpoint = format!("{}/artifacts", env::var("ANDA_ENDPOINT")?);
        let build_id = env::var("ANDA_BUILD_ID")?;

        // files is a hashmap of path -> actual file path
        // we need to convert them into a tuple of (path, file)
        // files[path] = actual_path
        let files: Vec<(String, PathBuf)> = self
            .files
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        let mut form = multipart::Form::new()
            .percent_encode_noop()
            .text("build_id", build_id);
        let output_path = env::var("ANDA_OUTPUT_PATH").unwrap_or_else(|_| "anda-build".to_string());

        for file in &files {
            // add to array of form data
            let (path, aa) = file;
            let mut buf = Vec::new();
            File::open(&aa).await?.read_to_end(&mut buf).await?;

            let p = aa.strip_prefix(&output_path).unwrap();
            debug!("adding file: {}", aa.display());
            let mimetype = MimeGuess::from_path(&aa).first_or_octet_stream();
            // add part to form
            let file_part = multipart::Part::bytes(buf)
                .file_name(p.display().to_string())
                .mime_str(mimetype.essence_str())?;

            // Get a position of the hashmap by matching the key to the path
            //let pos = files.clone().iter().position(|(k, _)| &k == &path);

            //form = form.part(format!("files[{}]", pos.unwrap()), file_part);
            form = form.part(format!("files[{}]", path), file_part);
        }

        debug!("form: {:?}", form);

        let res = ClientBuilder::new()
            .build()?
            .post(&endpoint)
            .multipart(form)
            .send()
            .await?;
        debug!("res: {:#?}", res.text().await?);
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct ProjectBuilder {
    root: PathBuf,
}

impl ProjectBuilder {
    pub fn new(root: PathBuf) -> Self {
        ProjectBuilder { root }
    }

    pub async fn push_folder(&self, folder: PathBuf) -> Result<()> {
        let mut hash = HashMap::new();

        for entry in WalkDir::new(&folder) {
            let entry = entry?;
            if entry.file_type().is_file() {
                let file_path = entry.into_path();
                let real_path = file_path.strip_prefix(&folder)?;
                hash.insert(real_path.display().to_string(), file_path);
            }
        }

        let uploader = ArtifactUploader::new(hash);
        uploader.upload().await?;

        Ok(())
    }

    pub fn dnf_builddep(&self, project: &Project) -> Result<(), BuilderError> {
        let spec_path = &project.rpmbuild.as_ref().unwrap().spec;

        let builddep_exit = runas::Command::new("dnf")
            .args(&["builddep", "-y", spec_path.to_str().unwrap()])
            .status()?;

        builddep_exit.exit_ok_polyfilled()?;
        Ok(())
    }

    pub async fn build_rpm(&self, project: &Project) -> Result<(), BuilderError> {
        let output_path = env::var("ANDA_OUTPUT_PATH").unwrap_or_else(|_| "anda-build".to_string());

        // if env var `ANDA_SKIP_BUILDDEP` is set to 1, we skip the builddep step
        if env::var("ANDA_SKIP_BUILDDEP").unwrap_or_default() != "1" {
            self.dnf_builddep(project)?;
        } else {
            warn!("builddep step skipped, builds may fail due to missing dependencies!");
        }
        let mut rpmbuild = Command::new("rpmbuild")
            .args(vec![
                "-ba",
                project.rpmbuild.as_ref().unwrap().spec.to_str().unwrap(),
                "--define",
                format!("_rpmdir {}", output_path).as_str(),
                "--define",
                format!("_srcrpmdir {}/src", output_path).as_str(),
                "--define",
                "_disable_source_fetch 0",
                "--define",
                format!(
                    "_sourcedir {}",
                    tokio::fs::canonicalize(&self.root)
                        .await?
                        .to_str()
                        .ok_or_else(|| BuilderError::Other(
                            "invalid unicode for path".to_string()
                        ))?
                )
                .as_str(),
            ])
            .current_dir(&self.root)
            .stderr(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()?;

        let stdout = rpmbuild.stdout.take().expect("Can't get stdout");
        let stderr = rpmbuild.stderr.take().expect("Can't get stderr");
        let reader_out = BufReader::new(stdout);
        let reader_err = BufReader::new(stderr);

        reader_out.lines().for_each(|line| {
            info!("rpmbuild:\t{}", line.unwrap());
        });
        reader_err.lines().for_each(|line| {
            warn!("rpmbuild:\t{}", line.unwrap());
        });

        // stream log output from rpmbuild to rust log

        //let rpmbuild_exit_status = rpmbuild.status()?;
        //rpmbuild_exit_status.exit_ok_polyfilled()?;
        rpmbuild.wait()?.exit_ok_polyfilled()?;

        Ok(())
    }

    pub fn run_pre_script(&self, project: &Project) -> Result<(), BuilderError> {
        println!(":: {}", "Running pre-build script...".yellow());
        for command in &project.pre_script.as_ref().unwrap().commands {
            println!("$ {}", command.black());
            let command = execute::command(command)
                .execute_output()
                .map_err(BuilderError::Script)?;

            if !command.status.success() {
                println!(":: {}", "Pre-build script failed".red());
                return Err(BuilderError::Command("pre-script failed".to_string()));
            }
        }
        println!("{}", "Pre-build script finished.".green());
        Ok(())
    }

    pub fn run_post_script(&self, project: &Project) -> Result<(), BuilderError> {
        println!(":: {}", "Running post-build script...".yellow());
        for command in &project.post_script.as_ref().unwrap().commands {
            println!("$ {}", command.black());
            let command = execute::command(command)
                .execute_output()
                .map_err(BuilderError::Script)?;

            if !command.status.success() {
                println!(":: {}", "Post-build script failed".red());
                return Err(BuilderError::Command("post-script failed".to_string()));
            }
        }
        println!("{}", "Post-build script finished.".green());
        Ok(())
    }

    pub fn run_stage(
        stage: &crate::config::Stage,
        stage_name: &String,
    ) -> Result<(), BuilderError> {
        println!(
            " -> {}: `{}`",
            "Starting script stage".yellow(),
            stage_name.white().italic()
        );
        for command in &stage.commands {
            println!("$ {}", command.black());

            let command = execute::command(command)
                .execute_output()
                .map_err(BuilderError::Script)?;

            if !command.status.success() {
                println!(":: {}", "Build script failed".red());
                return Err(BuilderError::Command("build script failed".to_string()));
            }
        }
        Ok(())
    }

    pub fn run_rollback(
        &self,
        project: &Project,
        stage: &crate::config::Stage,
    ) -> Result<(), BuilderError> {
        if project.rollback.is_some() {
            let rollback = project.rollback.as_ref().unwrap();
            let name = project
                .script
                .as_ref()
                .unwrap()
                .find_key_for_value(stage)
                .unwrap();
            if rollback.get_stage(name).is_some() {
                let stage = rollback.get_stage(name).unwrap();
                println!(
                    " -> {}: `{}`",
                    "Rolling back".yellow(),
                    name.white().italic()
                );
                for command in &stage.commands {
                    println!("$ {}", command.black());

                    let command = execute::command(command)
                        .execute_output()
                        .map_err(BuilderError::Script)?;

                    if !command.status.success() {
                        error!("{}", "Rollback failed".red());
                        return Err(BuilderError::Command("rollback failed".to_string()));
                    }
                }
            }
        }
        Ok(())
    }

    pub fn run_build_script(&self, project: &Project) -> Result<(), BuilderError> {
        // we should turn this into a tuple of (stage, stage_name)
        let mut depgraph: DepGraph<&crate::config::Stage> = DepGraph::new();
        println!(":: {}", "Running build script...".yellow());
        let script = project.script.as_ref().unwrap();
        for stage in script.stage.values() {
            let empty_vec: Vec<String> = Vec::new();
            let depends = stage.depends.as_ref().unwrap_or(&empty_vec);
            let depends = depends
                .iter()
                .map(|d| {
                    script
                        .get_stage(d)
                        .unwrap_or_else(|| panic!("Can't find stage {}", d.as_str()))
                })
                .collect::<Vec<&crate::config::Stage>>();
            depgraph.register_dependencies(stage, depends);
        }
        let final_stage = &crate::config::Stage {
            depends: None,
            commands: vec![],
        };
        depgraph.register_dependencies(
            final_stage,
            script.stage.iter().map(|(_, stage)| stage).collect(),
        );
        for node in depgraph.dependencies_of(&final_stage).unwrap() {
            match node {
                // FIXME: find_key_for_value fails to match data
                Ok(stage) => {
                    let result = Self::run_stage(
                        stage,
                        script
                            .find_key_for_value(stage)
                            .unwrap_or(&"final_or_untitled".to_string()),
                    );
                    if result.is_err() {
                        self.run_rollback(project, stage)?;
                        return Err(result.err().unwrap());
                    }
                }
                Err(e) => return Err(BuilderError::Other(format!("solvent: {:?}", e))),
            }
        }
        Ok(())
    }

    pub fn build_docker(&self, project: &Project) -> Result<(), BuilderError> {
        for (tag, image) in &project.docker.as_ref().unwrap().image {
            let version = image
                .version
                .as_ref()
                .map(|s| format!(":{}", s))
                .unwrap_or_else(String::new);
            let status = Command::new("docker")
                .arg("build")
                .arg("-t")
                .arg(format!("{}{}", tag, version))
                .arg(&image.workdir)
                .current_dir(&self.root)
                .status();

            if !status.unwrap().success() {
                return Err(BuilderError::Command("docker build failed".to_string()));
            }
        }
        Ok(())
    }

    pub async fn run_project(&self, name: String, project: &Project) -> Result<(), BuilderError> {
        println!(
            "{} `{}`...",
            "Building project".bright_cyan(),
            &name.white().bold()
        );

        if project.pre_script.is_some() {
            self.run_pre_script(project)?;
        }
        if project.script.is_some() {
            self.run_build_script(project)?;
        }
        if project.rpmbuild.is_some() {
            self.build_rpm(project).await?;
        }
        if project.docker.is_some() {
            self.build_docker(project)?;
        }
        if project.post_script.is_some() {
            self.run_post_script(project)?;
        }

        // print empty line to separate projects
        println!();
        Ok(())
    }

    ///  Builds an Andaman project.
    pub async fn build(&self, projects: Vec<String>) -> Result<(), BuilderError> {
        let config = crate::config::load_config(&self.root)?;
        let output_path = env::var("ANDA_OUTPUT_PATH").unwrap_or_else(|_| "anda-build".to_string());

        if !projects.is_empty() {
            for proj in projects {
                let project = config
                    .project
                    .get(&proj)
                    .ok_or_else(|| BuilderError::Other(format!("Project `{}` not found", &proj)))?;
                self.run_project(proj, project).await?;
            }
            return Ok(());
        }

        for (name, project) in config.project {
            self.run_project(name, &project).await?;
        }
        // if env var `ANDA_BUILD_ID` is set, we upload the artifacts
        if env::var("ANDA_BUILD_ID").is_ok() {
            info!("uploading artifacts...");
            self.push_folder(PathBuf::from(output_path)).await?;
        };
        Ok(())
    }
}
