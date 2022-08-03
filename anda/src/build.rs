use anyhow::{anyhow, Result};
use bollard::{container::Config, service::HostConfig};
use execute::Execute;
use futures::FutureExt;
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

use crate::{
    config::{AndaConfig, Project},
    container::{Container, ContainerHdl},
    error::{BuilderError, ProjectError},
    util,
};

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

        let builddep_exit = Command::new("sudo")
            .args(&["dnf", "builddep", "-y", spec_path.to_str().unwrap()])
            .status()?;

        builddep_exit.exit_ok_polyfilled()?;
        Ok(())
    }
    /// Prepares environment variables for the build process.
    pub fn prepare_env(&self, project: &Project) -> Result<Vec<String>, BuilderError> {
        let config = crate::config::load_config(&self.root)?;

        let mut envlist = Vec::new();
        if let Some(env) = project.env.as_ref() {
            for key in env {
                envlist.push(key.to_owned())
            }
        }

        if let Some(cid) = util::current_commit(&self.root) {
            //env::set_var("COMMIT_ID", cid);
            //println!("COMMIT_ID: {}", cid);
            envlist.push(format!("COMMIT_ID={}", cid));
        }

        if let Some(branch) = util::branch_name(&self.root) {
            //env::set_var("BRANCH", branch);
            envlist.push(format!("BRANCH={}", branch));
        }

        if let Some(project_name) = config.find_key_for_value(project) {
            //env::set_var("PROJECT_NAME", project_name);
            envlist.push(format!("PROJECT_NAME={}", project_name));
        };

        Ok(envlist)
    }

    pub async fn build_rpm(&self, project: &Project) -> Result<(), BuilderError> {
        let output_path = env::var("ANDA_OUTPUT_PATH").unwrap_or_else(|_| "anda-build".to_string());
        println!(":: {}", "Building RPMs".yellow());
        self.contain("rpm", project)
            .await?
            .run_cmd(vec![
                "sudo",
                "dnf",
                "install",
                "-y",
                "rpm-build",
                "dnf-plugins-core",
            ])
            .await?
            .run_cmd(vec![
                "sudo",
                "dnf",
                "builddep",
                "-y",
                project.rpmbuild.as_ref().unwrap().spec.to_str().unwrap(),
            ])
            .await?
            .run_cmd(vec![
                "rpmbuild",
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
            .await?
            .finish()
            .await?;
        Ok(())
    }

    pub fn run_pre_script(&self, project: &Project) -> Result<(), BuilderError> {
        println!(":: {}", "Running pre-build script...".yellow());
        for command in &project.pre_script.as_ref().unwrap().commands {
            println!("$ {}", command.black());
            let command = execute::shell(command)
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
            let command = execute::shell(command)
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

    pub async fn contain(&self, name: &str, project: &Project) -> Result<Container, BuilderError> {
        //let config = crate::config::load_config(&self.root)?;

        let envs = self.prepare_env(project)?;

        let conhdl = ContainerHdl::new();
        let cwd = self.root.canonicalize()?.to_str().unwrap().to_owned();

        //println!("{}", cwd);
        let hostconf = HostConfig {
            binds: Some(vec![format!("{}:{}", cwd, cwd)]),
            ..Default::default()
        };
        let cfg = Config {
            image: Some("fedora:latest".to_owned()),
            hostname: Some(name.to_string()),
            tty: Some(true),
            working_dir: Some(cwd),
            host_config: Some(hostconf),
            env: Some(envs),
            ..Default::default()
        };
        let c = Container::new(conhdl, Some(cfg)).await?.start().await;

        c.map_err(|e| BuilderError::Command(e.to_string()))
    }

    pub async fn run_stage(
        &self,
        stage: &crate::config::Stage,
        stage_name: &String,
        project: &Project,
    ) -> Result<(), BuilderError> {
        if !stage_name.eq("ANDA_UNTITLED_FINAL") {
            println!(
                " -> {}: `{}`",
                "Starting script stage".yellow(),
                stage_name.white().italic()
            );
        }

        self.contain("stage", project)
            .await?
            .run_cmds(stage.commands.iter().map(|c| c.as_str()).collect())
            .await?
            .finish()
            .await?;
        Ok(())
    }

    pub async fn run_rollback(
        &self,
        project: &Project,
        stage: &crate::config::Stage,
    ) -> Result<(), BuilderError> {
        self.prepare_env(project)?;
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
                match self
                    .contain("rollback", project)
                    .await?
                    .run_cmds(stage.commands.iter().map(|c| c.as_str()).collect())
                    .await
                {
                    Ok(con) => {
                        return con
                            .finish()
                            .await
                            .map(|()| ())
                            .map_err(|e| BuilderError::Other(e.to_string()))
                    }
                    Err(_) => {
                        error!("{}", "Rollback failed".red());
                        return Err(BuilderError::Command("rollback failed".to_string()));
                    }
                }
            }
        }
        Ok(())
    }

    pub async fn run_build_script(
        &self,
        project: &Project,
        stage: Option<String>,
    ) -> Result<(), BuilderError> {
        // we should turn this into a tuple of (stage, stage_name)
        self.prepare_env(project)?;
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
        for node in depgraph
            .dependencies_of(
                &stage
                    .map(|s| script.get_stage(s.as_str()).expect("Stage not found"))
                    .unwrap_or(final_stage),
            )
            .unwrap()
        {
            match node {
                Ok(stage) => {
                    let result = self
                        .run_stage(
                            stage,
                            script
                                .find_key_for_value(stage)
                                .unwrap_or(&"ANDA_UNTITLED_FINAL".to_string()),
                            project,
                        )
                        .await;
                    if result.is_err() {
                        self.run_rollback(project, stage).await?;
                        return Err(result.err().unwrap());
                    }
                }
                Err(e) => return Err(BuilderError::Other(format!("solvent: {:?}", e))),
            }
        }
        Ok(())
    }

    pub async fn build_docker(&self, project: &Project) -> Result<(), BuilderError> {
        println!(":: {}", "Building docker image...".yellow());
        self.prepare_env(project)?;

        let mut tasks = Vec::new();

        for (tag, image) in &project.docker.as_ref().unwrap().image {
            let task = {
                let version = image
                    .version
                    .as_ref()
                    .map(|s| format!(":{}", s))
                    .unwrap_or_else(String::new);

                let tag_string = format!("{}{}", tag, version);

                let command = format!(
                    "docker build -t {} {}",
                    tag_string,
                    &image.workdir.to_str().unwrap()
                );
                println!("$ {}", command.black());
                println!(
                    " -> {} `{}`",
                    "Building docker image".yellow(),
                    tag_string.white().italic().to_string().to_owned()
                );

                tokio::process::Command::new("bash")
                    .arg("-c")
                    .arg(command)
                    .current_dir(&self.root)
                    .status()
            };

            tasks.push(task.boxed());
        }
        for task in tasks {
            task.await?;
        }
        Ok(())
    }

    pub async fn run_whole_project(
        &self,
        name: String,
        project: &Project,
    ) -> Result<(), BuilderError> {
        println!(
            "{} `{}`...",
            "Building project".bright_cyan(),
            &name.white().bold()
        );

        let mut tasks = Vec::new();

        if project.pre_script.is_some() {
            self.run_pre_script(project)?;
        }
        if project.script.is_some() {
            tasks.push(self.run_build_script(project, None).boxed());
        }
        if project.rpmbuild.is_some() {
            tasks.push(self.build_rpm(project).boxed());
        }
        if project.docker.is_some() {
            tasks.push(self.build_docker(project).boxed());
        }
        for task in tasks {
            task.await?;
        }
        if project.post_script.is_some() {
            self.run_post_script(project)?;
        }
        // print empty line to separate projects
        println!();
        Ok(())
    }
    // project -> scope -> stage
    // example: project::script:stage, docker:image/image
    pub async fn build_in_scope(&self, query: &str) -> Result<(), BuilderError> {
        let re = regex::Regex::new(r"(.+)::([^:]+)(:(.+))?")
            .map_err(|e| BuilderError::Other(format!("Can't make regex: {}", e)))?;
        let config = crate::config::load_config(&self.root)?;
        for cap in re.captures_iter(query) {
            let project = &cap[1];
            let scope = &cap[2];
            let project = config.project.get(project).ok_or_else(|| {
                ProjectError::InvalidManifest(format!("no project `{}`", project))
            })?;
            let close = || ProjectError::InvalidManifest(format!("no scope `{}`", scope));
            if cap.get(4).is_none() {
                match scope {
                    "script" => {
                        project.script.as_ref().ok_or_else(close)?;
                        self.run_build_script(project, None).await?;
                    }
                    "pre_script" => {
                        project.pre_script.as_ref().ok_or_else(close)?;
                        self.run_pre_script(project)?;
                    }
                    "post_script" => {
                        project.post_script.as_ref().ok_or_else(close)?;
                        self.run_post_script(project)?;
                    }
                    "rpmbuild" => {
                        project.rpmbuild.as_ref().ok_or_else(close)?;
                        self.build_rpm(project).await?;
                    }
                    _ => {}
                }
            } else {
                let stage = &cap[4];
                match scope {
                    "script" => {
                        project.script.as_ref().ok_or_else(close)?;
                        self.run_build_script(project, Some(stage.to_string()))
                            .await?;
                    }
                    "docker" => {
                        project.docker.as_ref().ok_or_else(close)?;
                        self.build_docker(project).await?;
                    }
                    _ => {}
                }
            }
            // return Err(BuilderError::Command("Invalid argument passed".to_string()));
        }
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
                self.run_whole_project(proj, project).await?;
            }
            return Ok(());
        }

        for (name, project) in config.project {
            self.run_whole_project(name, &project).await?;
        }
        // if env var `ANDA_BUILD_ID` is set, we upload the artifacts
        if env::var("ANDA_BUILD_ID").is_ok() {
            info!("uploading artifacts...");
            self.push_folder(PathBuf::from(output_path)).await?;
        };
        Ok(())
    }
}
