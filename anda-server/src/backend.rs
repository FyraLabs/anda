//! Andaman builder backend
//! This module will be called by the Andaman server to interact with builders
//! It will accept build parameters and return the build object.
//! The builder will then be responsible for building the project.
//! The server will start a Kubernetes job and manage the build process (hopefully).

// TODO: Actually send kubernetes job to the server.

use std::path::PathBuf;

use anyhow::{anyhow, Result};
use rocket::serde::uuid::Uuid;

use crate::{db_object, artifacts::{S3Artifact, BUCKET}};


pub enum BuildMethod {
    Url { url: String },
    SrcFile { path: PathBuf, build_type: String },
}

pub struct AndaBackend {
    method: BuildMethod,
}

impl AndaBackend {
    pub fn new(method: BuildMethod) -> Self {
        AndaBackend {
            method
        }
    }
    pub fn new_src_file<T: Into<String>>(path: PathBuf, build_type: T) -> Self {
        AndaBackend {
            method: BuildMethod::SrcFile {
                path: PathBuf::new(),
                build_type: "".to_string()
            }
        }
    }
    pub fn new_url<T: Into<String>> (url: T) -> Self {
        AndaBackend {
            method: BuildMethod::Url {
                url: url.into()
            }
        }
    }

    // Proxy function to the actual build method.
    // Matches the method enum and calls the appropriate method.
    pub async fn build(&self) -> Result<()> {
        match &self.method {
            BuildMethod::Url { url } => {
                println!("Building from url: {}", url);
            }
            BuildMethod::SrcFile { path , build_type } => {
                println!("Building from src file: {}", path.display());
            }
        }
        Ok(())
    }

    // Builds a project from a URL (e.g. github)
    pub fn build_url(&self) {
        // check if file is valid

    }
}
#[async_trait]
trait S3Object {
    fn get_url(&self) -> String;
    async fn get(uuid: Uuid) -> Result<Self> where Self: Sized;
    /// Pull raw data from S3
    fn pull_bytes(&self) -> Result<Vec<u8>>;
    /// Upload file to S3
    fn upload_file(path: String) -> Result<()>;

}


// Temporary files for file uploads.
#[derive(Debug, Clone)]
pub struct UploadCache {
    pub path: PathBuf,
    pub filename: String,
}

impl UploadCache {
    /// Creates a new upload cache
    pub fn new(path: PathBuf, filename: String) -> Self {
        UploadCache {
            path,
            filename,
        }
    }

    pub async fn upload(&self) -> Result<()> {
        // Upload to S3 or whatever
        let obj = crate::artifacts::S3Artifact::new()?;

        let dest_path = format!("build_cache/{}/{}", Uuid::new_v4().simple(), self.filename);

        let _ = obj.upload_file(&dest_path, self.path.to_owned()).await?;
        println!("Uploaded {}", dest_path);
        Ok(())
    }
}

pub struct BuildCache {
    pub id: Uuid,
    pub filename: String,
}

impl BuildCache {
    pub fn new(filename: String) -> Self {
        dotenv::dotenv().ok();
        BuildCache {
            id: Uuid::new_v4(),
            filename,
        }
    }
}
#[async_trait]
impl S3Object for BuildCache {
    fn get_url(&self) -> String {
        format!("https://s3.amazonaws.com/andaman-build-cache/{}", self.filename)
    }
    async fn get(uuid: Uuid) -> Result<Self> where Self: Sized {
        // List all files in S3
        let obj = S3Artifact::new()?.connection;

        // Find an object with a tag called "BuildCacheID" with the value of the uuid.

        let objects = obj.list_objects_v2()
        .bucket(BUCKET.as_str())
        .prefix(format!("build_cache/{}", uuid.simple()).as_str()).send().await?.contents.unwrap();
        // get the first object
        let object = objects.first().unwrap();

        let filename = object.key.clone().unwrap();

        println!("Found build cache: {}", filename);

        Ok(BuildCache::new("".to_string()))
    }
    fn pull_bytes(&self) -> Result<Vec<u8>> {
        // Get from S3
        Ok(vec![])
    }
    fn upload_file(path: String) -> Result<()> {
        // Upload to S3
        Ok(())
    }
}

// Artifact API
// #[derive(Debug, Clone)]
#[cfg(test)]
mod test_super {
    use super::*;

    #[tokio::test]
    async fn get_obj() {
        let uuid = Uuid::parse_str("3e17f157e9cf4871896bc908265ec41b").unwrap();
        let obj = BuildCache::get(uuid).await.unwrap();

    }
}
