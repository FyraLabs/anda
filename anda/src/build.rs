use anyhow::{anyhow, Result};
use buildkit_llb::{
    prelude::{FileSystem, LayerPath, MultiOwnedOutput, OperationBuilder},
    utils::{OperationOutput, OutputIdx},
};
use execute::Execute;
use futures::FutureExt;
use log::{debug, error, info};
use mime_guess::MimeGuess;
use owo_colors::OwoColorize;
use reqwest::{multipart, ClientBuilder};
use serde::Serialize;
use solvent::DepGraph;
use std::{
    collections::{BTreeMap, HashMap},
    env,
    path::PathBuf,
    process::ExitStatus,
};
use tokio::{fs::File, io::AsyncReadExt};
use walkdir::WalkDir;

use crate::{
    config::Project,
    container::{Buildkit, BuildkitOptions},
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

        //debug!("form: {:?}", form);

        let _res = ClientBuilder::new()
            .build()?
            .post(&endpoint)
            .multipart(form)
            .send()
            .await?;
        // debug!("res: {:#?}", res.text().await?);
        Ok(())
    }
}

#[derive(Clone)]
pub struct BuilderOptions {
    pub display_llb: bool,
    pub config_location: PathBuf,
    pub buildkit_log: crate::BuildkitLog,
}
#[allow(clippy::derivable_impls)]
impl Default for BuilderOptions {
    fn default() -> Self {
        Self {
            display_llb: false,
            config_location: PathBuf::from("anda.hcl"),
            buildkit_log: crate::BuildkitLog::Auto,
        }
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
    /// Prepares environment variables for the build process.
    pub fn _prepare_env(
        &self,
        project: &Project,
        opts: &BuilderOptions,
    ) -> Result<BTreeMap<String, String>, BuilderError> {
        let config = crate::config::load_config(&opts.config_location)?;

        //println!("{:#?}", project.env);

        let mut envlist: BTreeMap<String, String> = BTreeMap::new();

        if let Some(env) = project.env.as_ref() {
            for (k, v) in env {
                envlist.insert(k.to_string(), v.to_string());
            }
        }
        if let Some(cid) = util::current_commit(&self.root) {
            envlist.insert("COMMIT_ID".to_owned(), cid);
        };

        if let Some(branch) = util::branch_name(&self.root) {
            //env::set_var("BRANCH", branch);
            envlist.insert("BRANCH".to_owned(), branch);
        }

        if let Some(project_name) = config.find_key_for_value(project) {
            //env::set_var("PROJECT_NAME", project_name);
            envlist.insert("PROJECT_NAME".to_owned(), project_name.to_owned());
        };

        Ok(envlist)
    }

    pub async fn build_rpm(
        &self,
        project: &Project,
        builder_opts: &BuilderOptions,
    ) -> Result<(), BuilderError> {
        let image = if let Some(image) = project.rpmbuild.as_ref().unwrap().image.as_ref() {
            image.to_owned()
        } else if let Some(image) = project.image.as_ref() {
            image.to_owned()
        } else {
            "fedora:latest".to_owned()
        };

        let config = crate::config::load_config(&builder_opts.config_location)?;
        let project_name = config.find_key_for_value(project).unwrap();

        let _output_path =
            env::var("ANDA_OUTPUT_PATH").unwrap_or_else(|_| "anda-build".to_string());
        eprintln!(":: {}", "Building RPMs".yellow());
        let envlist = self._prepare_env(project, builder_opts)?;
        /* let opts = BuildkitOptions {
            env: Some(envlist),
            ..Default::default()
        }; */

        let mut b = Buildkit::new(Some(BuildkitOptions {
            env: Some(envlist),
            context_name: Some(format!("{}::{}", project_name, "rpmbuild")),
            ..Default::default()
        }))
        .image(&image)
        .context(buildkit_llb::prelude::Source::local("context"));
        let mode = &project.rpmbuild.as_ref().unwrap().mode;
        match mode {
            crate::config::RpmBuildMode::Standard => {
                b.build_rpm(
                    project.rpmbuild.as_ref().unwrap().spec.to_str().unwrap(),
                    crate::config::RpmBuildMode::Standard,
                    project.rpmbuild.as_ref().unwrap().build_deps.as_ref(),
                );
                b.execute(builder_opts)?;
            }
            crate::config::RpmBuildMode::Cargo => {
                let cargo_project =
                    if let Some(proj) = project.rpmbuild.as_ref().unwrap().package.as_ref() {
                        proj.to_owned()
                    } else {
                        "".to_owned()
                    };
                b.build_rpm(
                    &cargo_project,
                    crate::config::RpmBuildMode::Cargo,
                    project.rpmbuild.as_ref().unwrap().build_deps.as_ref(),
                );
                b.execute(builder_opts)?;
            }
        };

        Ok(())
    }

    pub fn run_pre_script(
        &self,
        project: &Project,
        _opts: &BuilderOptions,
    ) -> Result<(), BuilderError> {
        eprintln!(":: {}", "Running pre-build script...".yellow());
        for command in &project.pre_script.as_ref().unwrap().commands {
            eprintln!("$ {}", command.black());
            let command = execute::shell(command)
                .execute_output()
                .map_err(BuilderError::Script)?;

            // create anda-build folder if it doesn't exist
            if PathBuf::from("anda-build").exists() {
                println!("anda-build folder exists");
            } else {
                println!("anda-build folder doesn't exist");
                std::fs::create_dir("anda-build").unwrap();
            }

            if !command.status.success() {
                println!(":: {}", "Pre-build script failed".red());
                return Err(BuilderError::Command("pre-script failed".to_string()));
            }
        }
        eprintln!("{}", "Pre-build script finished.".green());
        Ok(())
    }

    pub fn run_post_script(
        &self,
        project: &Project,
        _opts: &BuilderOptions,
    ) -> Result<(), BuilderError> {
        eprintln!(":: {}", "Running post-build script...".yellow());
        for command in &project.post_script.as_ref().unwrap().commands {
            eprintln!("$ {}", command.black());
            let command = execute::shell(command)
                .execute_output()
                .map_err(BuilderError::Script)?;

            if !command.status.success() {
                eprintln!(":: {}", "Post-build script failed".red());
                return Err(BuilderError::Command("post-script failed".to_string()));
            }
        }
        eprintln!("{}", "Post-build script finished.".green());
        Ok(())
    }

    pub async fn run_stage(
        &self,
        stage: &crate::config::Stage,
        stage_name: &String,
        project: &Project,
        builder_opts: &BuilderOptions,
        prev_output: Option<OperationOutput<'static>>,
    ) -> Result<OperationOutput<'static>, BuilderError> {
        // load config
        let config = crate::config::load_config(&builder_opts.config_location)?;
        if !stage_name.eq("") {
            eprintln!(
                " -> {}: `{}`",
                "Starting script stage".yellow(),
                stage_name.white().italic()
            );
        }

        if stage.commands.is_empty() {
            return Ok(prev_output.unwrap());
        }

        let envlist = self._prepare_env(project, builder_opts)?;

        let project_name = config.find_key_for_value(project).unwrap();

        let opts = BuildkitOptions {
            env: Some(envlist),
            //transfer_artifacts: Some(true),
            context_name: Some(format!("{}::{}", project_name, stage_name)),
            ..Default::default()
        };

        //println!("{:?}", stage.image.as_ref());

        let image = if let Some(image) = stage.image.as_ref() {
            //println!("{}", image);
            image.to_owned()
        } else if let Some(image) = project.image.as_ref() {
            image.to_owned()
        } else {
            "fedora:latest".to_owned()
        };

        //println!("{}", image);

        let mut b = Buildkit::new(Some(opts))
            .image(&image)
            .context(buildkit_llb::prelude::Source::local("context"));

        if let Some(ref _out) = prev_output {
            //println!("{:?}", _out);
            b.artifact_cache = Some(prev_output.as_ref().unwrap().clone());
        }

        /* self.contain("stage", project)
        .await?
        .run_cmds(stage.commands.iter().map(|c| c.as_str()).collect())
        .await?
        .finish()
        .await?; */

        for command in &stage.commands {
            b.command(command);
        }

        /* let (cmd1, cmdn) = &stage.commands.split_first().unwrap();
        b.command(cmd1);
        for command in cmdn.iter() {
            b.command_nocontext(command.as_str());
        } */
        //b.execute(builder_opts)?;

        Ok(b.build_graph_builder())
    }

    pub async fn run_rollback(
        &self,
        project: &Project,
        stage: &crate::config::Stage,
        opts: &BuilderOptions,
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
                todo!();
            }
        }
        Ok(())
    }

    pub async fn run_build_script(
        &self,
        project: &Project,
        stage: Option<String>,
        opts: &BuilderOptions,
    ) -> Result<(), BuilderError> {
        // we should turn this into a tuple of (stage, stage_name)
        let mut depgraph: DepGraph<&crate::config::Stage> = DepGraph::new();
        eprintln!(":: {}", "Running build script...".yellow());
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
            image: None,
        };
        depgraph.register_dependencies(
            final_stage,
            script.stage.iter().map(|(_, stage)| stage).collect(),
        );

        // dummy buildkit output for the final stage
        let bk_opts = BuildkitOptions {
            env: Some(self._prepare_env(project, opts)?),
            ..Default::default()
        };

        let mut b = Buildkit::new(Some(bk_opts))
            .image(
                project
                    .image
                    .as_ref()
                    .unwrap_or(&"fedora:latest".to_string()),
            )
            .context(buildkit_llb::prelude::Source::local("context"));

        let mut dummyout = None;

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
                            script.find_key_for_value(stage).unwrap_or(&"".to_string()),
                            project,
                            opts,
                            dummyout,
                        )
                        .await;
                    if result.is_err() {
                        self.run_rollback(project, stage, opts).await?;
                        return Err(result.err().unwrap());
                    }

                    let cache: OperationOutput<'_> = result.ok().unwrap();
                    dummyout = Some(cache);
                    b.artifact_cache = dummyout.clone();
                }
                Err(e) => return Err(BuilderError::Other(format!("solvent: {:?}", e))),
            }
        }
        b.merge_artifact_output(dummyout.unwrap());
        b.execute(opts)?;
        Ok(())
    }

    pub async fn build_docker(
        &self,
        project: &Project,
        opts: &BuilderOptions,
    ) -> Result<(), BuilderError> {
        eprintln!(":: {}", "Building docker image...".yellow());
        todo!();
        // TODO: Rewrite this with BuildKit in mind

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
                eprintln!("$ {}", command.black());
                eprintln!(
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
        opts: &BuilderOptions,
    ) -> Result<(), BuilderError> {
        eprintln!(
            "{} `{}`...",
            "Building project".bright_cyan(),
            &name.white().bold()
        );

        let mut tasks = Vec::new();

        if project.pre_script.is_some() {
            self.run_pre_script(project, opts)?;
        }
        if project.script.is_some() {
            tasks.push(self.run_build_script(project, None, opts).boxed());
        }
        if project.rpmbuild.is_some() {
            tasks.push(self.build_rpm(project, opts).boxed());
        }
        if project.docker.is_some() {
            tasks.push(self.build_docker(project, opts).boxed());
        }
        for task in tasks {
            task.await?;
        }
        if project.post_script.is_some() {
            self.run_post_script(project, opts)?;
        }
        // print empty line to separate projects
        eprintln!();
        Ok(())
    }
    // project -> scope -> stage
    // example: project::script:stage, docker:image/image
    pub async fn build_in_scope(
        &self,
        query: &str,
        opts: &BuilderOptions,
    ) -> Result<(), BuilderError> {
        let re = regex::Regex::new(r"(.+)::([^:]+)(:(.+))?")
            .map_err(|e| BuilderError::Other(format!("Can't make regex: {}", e)))?;
        let config = crate::config::load_config(&opts.config_location)?;
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
                        self.run_build_script(project, None, opts).await?;
                    }
                    "pre_script" => {
                        project.pre_script.as_ref().ok_or_else(close)?;
                        self.run_pre_script(project, opts)?;
                    }
                    "post_script" => {
                        project.post_script.as_ref().ok_or_else(close)?;
                        self.run_post_script(project, opts)?;
                    }
                    "rpmbuild" => {
                        project.rpmbuild.as_ref().ok_or_else(close)?;
                        self.build_rpm(project, opts).await?;
                    }
                    _ => {}
                }
            } else {
                let stage = &cap[4];
                match scope {
                    "script" => {
                        project.script.as_ref().ok_or_else(close)?;
                        self.run_build_script(project, Some(stage.to_string()), opts)
                            .await?;
                    }
                    "docker" => {
                        project.docker.as_ref().ok_or_else(close)?;
                        self.build_docker(project, opts).await?;
                    }
                    _ => {}
                }
            }
            // return Err(BuilderError::Command("Invalid argument passed".to_string()));
        }
        Ok(())
    }

    ///  Builds an Andaman project.
    pub async fn build(
        &self,
        projects: Vec<String>,
        opts: &BuilderOptions,
    ) -> Result<(), BuilderError> {
        let config = crate::config::load_config(&opts.config_location)?;
        let output_path = env::var("ANDA_OUTPUT_PATH").unwrap_or_else(|_| "anda-build".to_string());

        if !projects.is_empty() {
            for proj in projects {
                let project = config
                    .project
                    .get(&proj)
                    .ok_or_else(|| BuilderError::Other(format!("Project `{}` not found", &proj)))?;
                self.run_whole_project(proj, project, opts).await?;
            }
            return Ok(());
        }

        for (name, project) in config.project {
            self.run_whole_project(name, &project, opts).await?;
        }
        // if env var `ANDA_BUILD_ID` is set, we upload the artifacts
        if env::var("ANDA_BUILD_ID").is_ok() {
            info!("uploading artifacts...");
            self.push_folder(PathBuf::from(output_path)).await?;
        };
        Ok(())
    }
}
