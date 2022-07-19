//! API Interaction backend for Andaman.
//! This module contains the facade for interacting with the backend using the REST API.
//! The backend is implemented using the `anda-server` crate running on a separate process.
//! To test this code, you will need to set up and start the backend first.
//! See the `anda-server` crate in the Andaman repository for more information.

use anyhow::{anyhow, Result};
use chrono::{DateTime, NaiveDateTime, Utc};
use reqwest::Client;
use serde_derive::{Deserialize, Serialize};
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
            url: url,
        }
    }

    pub async fn list_artifacts(&self) -> Result<Vec<Artifact>> {
        let url = format!("{}/artifacts", self.url);
        let resp = self.client.get(&url).send().await?;
        //println!("{:?}", &resp.json().await?);
        let artifacts: Vec<Artifact> = resp.json().await?;
        Ok(artifacts)
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
}
