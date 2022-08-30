//! API Interaction backend for Andaman.
//! This module contains the facade for interacting with the backend using the REST API.
//! The backend is implemented using the `anda-server` crate running on a separate process.
//! To test this code, you will need to set up and start the backend first.
//! See the `anda-server` crate in the Andaman repository for more information.

use anyhow::{Ok, Result};


use reqwest::{
    multipart::{self, Form},
    Client,
};
use reqwest_eventsource::EventSource;

use tokio::io::AsyncReadExt;

use std::{env, path::PathBuf};
use uuid::Uuid;

use anda_types::config::AndaConfig;
pub use anda_types::api::*;

#[derive(Clone)]
pub(crate) struct AndaBackend {
    client: Client,
    pub url: String,
}

impl AndaBackend {
    pub fn new(url: Option<String>) -> Self {
        dotenv::dotenv().ok();
        let url = url.unwrap_or_else(|| env::var("ANDA_ENDPOINT").expect("ANDA_ENDPOINT not set"));
        AndaBackend {
            client: Client::new(),
            url,
        }
    }

    pub async fn list_artifacts(&self) -> Result<Vec<Artifact>> {
        let url = format!("{}/artifacts", self.url);
        let resp = self
            .client
            .get(&url)
            .query(&[("limit", "10")])
            .send()
            .await?;
        //println!("{:?}", &resp.json().await?);
        Ok(resp.json().await?)
    }

    pub async fn list_builds(&self) -> Result<Vec<Build>> {
        let url = format!("{}/builds", self.url);
        let resp = self
            .client
            .get(&url)
            .query(&[("limit", "10")])
            .send()
            .await?;
        //println!("{:?}", &resp.json().await?);
        let builds: Vec<Build> = resp.json().await?;
        Ok(builds)
    }

    pub async fn list_projects(&self) -> Result<Vec<Project>> {
        let url = format!("{}/projects", self.url);
        let resp = self
            .client
            .get(&url)
            .query(&[("limit", "10")])
            .send()
            .await?;
        //println!("{:?}", &resp.json().await?);
        let projects: Vec<Project> = resp.json().await?;
        Ok(projects)
    }

    pub async fn build_metadata(&self, build_id: Uuid, scope: Option<String>, source: Option<String>, config: &AndaConfig) -> Result<Build> {
        let url = format!("{}/builds/{}/metadata", self.url, build_id);
        let meta = BuildMeta {
            scope,
            source,
            config_meta: Some(config.clone()),
        };
        let resp = self
            .client
            .post(&url)
            .json(&meta)
            .send()
            .await?;
        //println!("{:?}", &resp.json().await?);
        let build: Build = resp.json().await?;
        Ok(build)
    }

    pub async fn get_build(&self, id: Uuid) -> Result<Build> {
        let url = format!("{}/builds/{}", self.url, id);
        let resp = self.client.get(&url).send().await?;
        //println!("{:?}", &resp.json().await?);
        let build: Build = resp.json().await?;
        Ok(build)
    }

    pub async fn get_build_by_target(&self, target_id: Uuid) -> Result<Vec<Build>> {
        let url = format!("{}/builds/by_target/{}", self.url, target_id);
        let resp = self.client.get(&url).send().await?;
        //println!("{:?}", &resp.json().await?);
        let builds: Vec<Build> = resp.json().await?;
        Ok(builds)
    }

    pub async fn get_build_by_project(&self, project_id: Uuid) -> Result<Vec<Build>> {
        let url = format!("{}/builds/by_project/{}", self.url, project_id);
        let resp = self.client.get(&url).send().await?;
        //println!("{:?}", &resp.json().await?);
        let builds: Vec<Build> = resp.json().await?;
        Ok(builds)
    }

    pub async fn update_build_status(&self, id: Uuid, status: i32) -> Result<Build> {
        let url = format!("{}/builds/update_status", self.url);
        let form = Form::new()
            .percent_encode_noop()
            .text("id", id.to_string())
            .text("status", status.to_string());

        let resp = self.client.post(&url).multipart(form).send().await?;

        let build: Build = resp.json().await?;
        Ok(build)
    }

    pub async fn get_project(&self, id: Uuid) -> Result<Project> {
        let url = format!("{}/projects/{}", self.url, id);
        let resp = self.client.get(&url).send().await?;
        //println!("{:?}", &resp.json().await?);
        let project: Project = resp.json().await?;
        Ok(project)
    }

    pub async fn get_project_by_name(&self, name: String) -> Result<Project> {
        let url = format!("{}/projects/by_name/{}", self.url, name);
        let resp = self.client.get(&url).send().await?;
        //println!("{:?}", &resp.json().await?);
        let project: Project = resp.json().await?;
        Ok(project)
    }

    pub async fn upload_build(&self, target_id: Uuid, packfile_path: &PathBuf, scope: Option<String>) -> Result<Build> {
        let url = format!("{}/builds", self.url);

        //debug!("{}", target_id);

        let mut buf = Vec::new();

        tokio::fs::File::open(packfile_path)
            .await?
            .read_to_end(&mut buf)
            .await?;

        let file_part = multipart::Part::bytes(buf)
            .mime_str("application/octet-stream")?
            .file_name(
                packfile_path
                    .file_name()
                    .unwrap()
                    .to_str()
                    .unwrap()
                    .to_owned(),
            );

        //println!("{:?}", file_part);
        let _target_part = multipart::Part::text(target_id.to_string());
        let mut form = Form::new()
            .percent_encode_noop()
            //.part("target_id", target_part)
            .part("src_file", file_part)
            .text("target_id", target_id.to_string());

        //debug!("{:?}", form);

        if let Some(scope) = scope {
            form = form.text("project", scope);
        }

        let resp = self.client.post(&url).multipart(form).send().await?;
        //println!("{:?}", &resp.json().await?);
        let build: Build = resp.json().await?;
        //todo!();
        Ok(build)
    }


    pub async fn tag_build_project(&self, build_id: Uuid, project_id: Uuid) -> Result<Build> {
        let url = format!("{}/builds/tag_project", self.url);
        let form = Form::new()
            .percent_encode_noop()
            .text("id", build_id.to_string())
            .text("tag", project_id.to_string());

        let resp = self.client.post(&url).multipart(form).send().await?;
        //println!("{:?}", &resp.json().await?);
        let build: Build = resp.json().await?;
        //todo!();
        Ok(build)
    }

    pub async fn get_target_by_id(&self, id: Uuid) -> Result<Target> {
        let url = format!("{}/targets/{}", self.url, id);
        let resp = self.client.get(&url).send().await?;
        //println!("{:?}", &resp.json().await?);
        let target: Target = resp.json().await?;
        Ok(target)
    }

    pub async fn get_target_by_name(&self, name: &str) -> Result<Target> {
        let url = format!("{}/targets/by_name/{}", self.url, name);
        let resp = self.client.get(&url).send().await?;
        //println!("{:?}", &resp.json().await?);
        let target: Target = resp.json().await?;
        Ok(target)
    }

    pub async fn new_target(&self, name: &str, arch: &str, image: Option<String>) -> Result<Target> {
        let url = format!("{}/targets/", self.url);
        let target = Target {
            id: Uuid::nil(),
            name: name.to_string(),
            arch: arch.to_string(),
            image: image.clone(),
        };

        let resp = self.client.post(&url).json(&target).send().await?;
        
        let target: Target = resp.json().await?;
        Ok(target)
    }

    pub async fn new_artifact_with_metadata(&self, artifact: Artifact) -> Result<Artifact> {
        let url = format!("{}/artifacts/metadata", self.url);
        let resp = self.client.post(&url).json(&artifact).send().await?;
        let artifact: Artifact = resp.json().await?;
        Ok(artifact)
    }

    pub fn stream_logs(&self, id: Uuid) -> EventSource {
        let url = format!("{}/builds/{}/log", self.url, id);
        EventSource::get(&url)
    }
}

#[cfg(test)]
mod test_api {
    use super::*;

    #[tokio::test]
    async fn test_artifacts() {
        // dotenv file must be present in the current directory
        let backend = AndaBackend::new(None);
        let a = backend.list_artifacts().await.unwrap();
        println!("{:#?}", a);
    }
    #[tokio::test]
    async fn test_update_status() {
        let backend = AndaBackend::new(None);
        let id = "64b24bea5d504c64a81518ebec0a063b".parse::<Uuid>().unwrap();
        let a = backend.update_build_status(id, 1).await.unwrap();
        println!("{:#?}", a);
    }
}
