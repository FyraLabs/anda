//! API Interaction backend for Andaman.
//! This module contains the facade for interacting with the backend using the REST API.
//! The backend is implemented using the `anda-server` crate running on a separate process.
//! To test this code, you will need to set up and start the backend first.
//! See the `anda-server` crate in the Andaman repository for more information.

use anyhow::{anyhow, Ok, Result};
use chrono::{DateTime, NaiveDateTime, Utc};
use reqwest::{multipart::Form, Client};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::env;
use uuid::Uuid;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Artifact {
    pub id: Uuid,
    pub name: String,
    pub url: String,
    pub build_id: Uuid,
    pub timestamp: NaiveDateTime,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Build {
    pub id: Uuid,
    pub worker: Uuid,
    pub status: i32,
    pub project_id: Option<Uuid>,
    pub timestamp: NaiveDateTime,
    pub compose_id: Option<Uuid>,
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
