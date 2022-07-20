//! Andaman builder backend
//! This module will be called by the Andaman server to interact with builders
//! It will accept build parameters and return the build object.
//! The builder will then be responsible for building the project.
//! The server will start a Kubernetes job and manage the build process (hopefully).

// TODO: Actually send kubernetes job to the server.

use std::path::PathBuf;

use anyhow::{anyhow, Result};
use rocket::serde::uuid::Uuid;

use crate::db_object;


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

trait S3Object {
    fn get_url(&self) -> String;
    fn get(uuid: Uuid) -> Result<Self> where Self: Sized;
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


// Artifact API
// #[derive(Debug, Clone)]
