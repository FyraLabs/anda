//! Andaman builder backend
//! This module will be called by the Andaman server to interact with builders
//! It will accept build parameters and return the build object.
//! The builder will then be responsible for building the project.
//! The server will start a Kubernetes job and manage the build process (hopefully).

// TODO: Actually send kubernetes job to the server.

use std::{path::PathBuf, time::SystemTime};

use anyhow::Result;
use aws_sdk_s3::types::{ByteStream, DateTime};
//use aws_smithy_types::DateTime;
use rocket::serde::uuid::Uuid;
use tokio::{fs::File, io::AsyncReadExt};

use crate::s3_object::{S3Artifact, BUCKET, S3_ENDPOINT};

pub enum BuildMethod {
    Url { url: String },
    SrcFile { path: PathBuf, filename: String },
}

pub struct AndaBackend {
    method: BuildMethod,
}

impl AndaBackend {
    pub fn new(method: BuildMethod) -> Self {
        AndaBackend { method }
    }
    pub fn new_src_file<T: Into<String>>(path: PathBuf, filename: T) -> Self {
        AndaBackend {
            method: BuildMethod::SrcFile {
                path,
                filename: filename.into(),
            },
        }
    }
    pub fn new_url<T: Into<String>>(url: T) -> Self {
        AndaBackend {
            method: BuildMethod::Url { url: url.into() },
        }
    }

    // Proxy function to the actual build method.
    // Matches the method enum and calls the appropriate method.
    pub async fn build(&self) -> Result<()> {
        match &self.method {
            BuildMethod::Url { url } => {
                println!("Building from url: {}", url);

                //crate::kubernetes::dispatch_build(id, image);
            }
            BuildMethod::SrcFile { path, filename } => {
                println!("Building from src file: {:?}", path);
                println!("actual filename: {}", filename);


                // now check what kind of file it is, so we can determine which build backend to use.
                // match file extension
                if filename.ends_with("src.rpm") {
                    // call rpmbuild backend
                    panic!("rpmbuild backend not implemented yet");
                } else if filename.ends_with("andasrc.tar") {
                    // We have an andaman tarball.
                    todo!();
                }
            }
        }
        Ok(())
    }

    // Builds a project from a URL (e.g. github)
    pub fn build_url(&self) {
        // check if file is valid
        todo!();
    }
}
#[async_trait]
pub trait S3Object {
    fn get_url(&self) -> String;
    async fn get(uuid: Uuid) -> Result<Self>
    where
        Self: Sized;
    /// Pull raw data from S3
    async fn pull_bytes(&self) -> Result<ByteStream>
    where
        Self: Sized;
    /// Upload file to S3
    async fn upload_file(self, path: PathBuf) -> Result<Self>
    where
        Self: Sized;
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
        UploadCache { path, filename }
    }

    pub async fn upload(&self) -> Result<()> {
        // Upload to S3 or whatever
        let obj = crate::s3_object::S3Artifact::new()?;

        let dest_path = format!("build_cache/{}/{}", Uuid::new_v4().simple(), self.filename);

        let _ = obj.upload_file(&dest_path, self.path.to_owned()).await?;
        println!("Uploaded {}", dest_path);
        Ok(())
    }
}

#[derive(Debug, Clone)]
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
        // get url from S3
        format!(
            "{endpoint}/{bucket}/build_cache/{id_simple}/{filename}",
            endpoint = S3_ENDPOINT.as_str(),
            bucket = BUCKET.as_str(),
            id_simple = self.id.simple(),
            filename = self.filename
        )
    }

    async fn get(uuid: Uuid) -> Result<Self>
    where
        Self: Sized,
    {
        // List all files in S3
        let obj = S3Artifact::new()?.connection;

        // Find an object with a tag called "BuildCacheID" with the value of the uuid.

        let objects = obj
            .list_objects_v2()
            .bucket(BUCKET.as_str())
            .prefix(format!("build_cache/{}", uuid.simple()).as_str())
            .send()
            .await?
            .contents
            .unwrap();
        // get the first object
        let object = objects.first().unwrap();

        let filename = object.key.clone().unwrap();

        println!("Found build cache: {}", filename);

        Ok(BuildCache { id: uuid, filename })
    }

    async fn pull_bytes(&self) -> Result<ByteStream>
    where
        Self: Sized,
    {
        // Get from S3
        todo!()
    }

    async fn upload_file(self, path: PathBuf) -> Result<Self> {
        let file_path = path.canonicalize()?;
        let mut file = File::open(file_path).await?;
        let metadata = file.metadata().await?;
        let mut bytes = vec![0; metadata.len() as usize];
        file.read_buf(&mut bytes).await?;

        let obj = crate::s3_object::S3Artifact::new()?;
        let dest_path = format!("build_cache/{}/{}", self.id.simple(), self.filename);
        let chrono_time = chrono::Utc::now() + chrono::Duration::days(7);
        //let aws_time = DateTime::from_chrono_utc(chrono_time);
        // convert chrono time to system time
        let sys_time = SystemTime::from(chrono_time);
        obj.connection
            .put_object()
            .bucket(BUCKET.as_str())
            .key(dest_path.as_str())
            // 7 days
            .expires(DateTime::from(sys_time))
            .send()
            .await?;
        println!("Uploaded {}", dest_path);
        Ok(self)
    }
}

#[derive(Debug, Clone)]
pub struct Artifact {
    pub id: Uuid,
    pub filename: String,
    pub path: String,
    pub build_id: Uuid,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

impl Artifact {
    pub fn new(filename: String, path: String, build_id: Uuid) -> Self {
        dotenv::dotenv().ok();
        Self {
            id: Uuid::new_v4(),
            filename,
            path,
            build_id,
            timestamp: chrono::Utc::now(),
        }
    }

    pub async fn get_for_build(&self, build_id: Uuid) -> Result<Vec<Self>> {
        let arts = crate::db_object::Artifact::get_by_build_id(build_id).await?;

        Ok(arts
            .iter()
            .map(|art| Self {
                id: art.id,
                filename: art.name.clone().split("/").last().unwrap().to_string(),
                path: art.name.clone(),
                build_id: art.build_id,
                timestamp: art.timestamp,
            })
            .collect())
    }

    pub async fn metadata(&self) -> Result<crate::db_object::Artifact> {
        crate::db_object::Artifact::get(self.id).await
    }
}

#[async_trait]
impl S3Object for Artifact {
    fn get_url(&self) -> String {
        // get url from S3
        format!(
            "{endpoint}/{bucket}/artifacts/{id_simple}/{filename}",
            endpoint = S3_ENDPOINT.as_str(),
            bucket = BUCKET.as_str(),
            id_simple = self.id.simple(),
            filename = self.filename
        )
    }
    async fn upload_file(self, path: PathBuf) -> Result<Self> {
        let obj = crate::s3_object::S3Artifact::new()?;
        let dest_path = format!("artifacts/{}/{}", self.id.simple(), self.path);
        let _ = obj.upload_file(&dest_path, path.to_owned()).await?;
        // now update the database
        crate::db_object::Artifact::new(
            self.id,
            self.build_id,
            dest_path
                .strip_prefix(&format!("artifacts/{}/", self.id.simple()))
                .unwrap()
                .to_string(),
            self.get_url(),
        )
        .add().await?;

        println!("Uploaded {}", dest_path);
        Ok(self)
    }
    async fn pull_bytes(&self) -> Result<ByteStream> {
        // Get from S3

        let s3 = crate::s3_object::S3Artifact::new()?;
        s3.get_file(&self.path).await
    }

    async fn get(uuid: Uuid) -> Result<Self>
    where
        Self: Sized,
    {
        // Query the database for the artifact with the given uuid.
        let artifact_meta = crate::db_object::Artifact::get(uuid).await?;

        let filepath = artifact_meta.name.clone();
        let filename = filepath.strip_prefix("artifacts/").unwrap().to_string();
        let id = artifact_meta.id;

        Ok(Artifact {
            id,
            filename: filename.to_string(),
            path: filepath,
            build_id: artifact_meta.build_id,
            timestamp: artifact_meta.timestamp,
        })
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
        println!("{:?}", obj);
    }
}
