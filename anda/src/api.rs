//! API Interaction backend for Andaman.
//! This module contains the facade for interacting with the backend using the REST API.
//! The backend is implemented using the `anda-server` crate running on a separate process.
//! To test this code, you will need to set up and start the backend first.
//! See the `anda-server` crate in the Andaman repository for more information.

use anyhow::{Ok, Result};
use chrono::{DateTime, Utc};
use log::debug;
use reqwest::{multipart::{Form, self}, Client};
use serde::{Deserialize, Serialize};
use tokio::io::AsyncReadExt;

use std::{env, path::PathBuf};
use uuid::Uuid;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Artifact {
    pub id: Uuid,
    pub name: String,
    pub url: String,
    pub build_id: Uuid,
    pub timestamp: DateTime<Utc>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Build {
    pub id: Uuid,
    pub status: String,
    pub project_id: Option<Uuid>,
    pub timestamp: DateTime<Utc>,
    pub compose_id: Option<Uuid>,
}


#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Target {
    pub id: Uuid,
    pub image: Option<String>,
    pub name: String,
}

#[derive(Clone)]
pub(crate) struct AndaBackend {
    client: Client,
    url: String,
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

    pub async fn upload_build(&self,target_id: Uuid, packfile_path: &PathBuf) -> Result<Build> {
        let url = format!("{}/builds", self.url);

        debug!("{}", target_id);

        let mut buf = Vec::new();

        tokio::fs::File::open(packfile_path).await?.read_to_end(&mut buf).await?;

        let file_part = multipart::Part::bytes(buf)
            .mime_str("application/octet-stream")?
            .file_name(packfile_path.file_name().unwrap().to_str().unwrap().to_owned());

        println!("{:?}", file_part);
        let target_part = multipart::Part::text(target_id.to_string());
            let form = Form::new()
            .percent_encode_noop()
            //.part("target_id", target_part)
            .part("src_file", file_part)
            .text("target_id", target_id.to_string());

        //debug!("{:?}", form);

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
