use anyhow::{anyhow, Result, Ok};
use log::debug;
use serde_derive::Serialize;
use tokio::io::AsyncReadExt;
use walkdir::WalkDir;
use std::{fs, env};
use std::path::Path;
use std::{
    path::PathBuf,
    process::{Command, ExitStatus},
    collections::HashMap,
};
use tokio::fs::File;
use reqwest::{Client, ClientBuilder};
use reqwest::multipart;

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
        Self {
            files,
        }
    }

    pub async fn upload(&self) -> Result<()> {
        let endpoint = format!("{}/artifacts",env::var("ANDA_ENDPOINT")?);
        let build_id = env::var("ANDA_BUILD_ID")?;

        // files is a hashmap of path -> actual file path
        // we need to convert them into a tuple of (path, file)
        // files[path] = actual_path
        let files: Vec<(String, PathBuf)> = self.files.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
        let mut form = multipart::Form::new()
            .text("build_id", build_id);


        for file in files {
            // add to array of form data
            let (path, aa) = file;

            debug!("adding file: {}", aa.display());
            // add part to form
            let file_part = multipart::Part::text("files")
                .file_name(aa.display().to_string())
                .mime_str("application/octet-stream")?;

            form = form.part(format!("files[{}]", path), file_part);
        }

        //debug!("form: {:#?}", form);

        // BUG: Only the files in the top directory are uploaded.
        // Please fix this.

        let res = ClientBuilder::new()
            .build()
            .unwrap()
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
                let real_path = file_path.strip_prefix(&folder).unwrap();
                println!("path: {}", real_path.display());
                hash.insert(real_path.display().to_string(), file_path.canonicalize()?);
            }
        }

        let uploader = ArtifactUploader::new(hash);
        uploader.upload().await?;

        Ok(())
    }

    ///  Builds an Andaman project.
    pub async fn build(&self) -> Result<()> {
        // TODO: Move this to a method called `build_rpm` as we support more project types
        let config = crate::config::load_config(&self.root)?;
        sudo::with_env(&["ANDA_"]).unwrap();
        let builddep_exit = Command::new("dnf")
            .args(vec![
                "builddep",
                "-y",
                config.package.spec.to_str().unwrap(),
            ])
            .current_dir(&self.root)
            .status()?;

        builddep_exit.exit_ok_polyfilled()?;

        let rpmbuild_exit = Command::new("rpmbuild")
            .args(vec![
                "-ba",
                config.package.spec.to_str().unwrap(),
                "--define",
                format!("_rpmdir anda-build").as_str(),
                "--define",
                format!("_srcrpmdir anda-build").as_str(),
                "--define",
                "_disable_source_fetch 0",
                "--define",
                format!("_sourcedir {}", fs::canonicalize(&self.root)?.to_str().unwrap()).as_str(),
            ])
            .current_dir(&self.root)
            .status()?;

        rpmbuild_exit.exit_ok_polyfilled()?;

        self.push_folder(PathBuf::from("anda-build")).await?;

        Ok(())
    }
}
