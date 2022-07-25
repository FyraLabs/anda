//! Andaman builder backend
//! This module will be called by the Andaman server to interact with builders
//! It will accept build parameters and return the build object.
//! The builder will then be responsible for building the project.
//! The server will start a Kubernetes job and manage the build process (hopefully).

// TODO: Actually send kubernetes job to the server.

use std::{path::PathBuf, time::SystemTime};

use anyhow::Result;
use aws_sdk_s3::types::{ByteStream, DateTime};
use chrono::Utc;
//use aws_smithy_types::DateTime;
use rocket::serde::uuid::Uuid;
use tokio::{fs::File, io::AsyncReadExt};

use crate::s3_object::{S3Artifact, BUCKET, S3_ENDPOINT};
use num_derive::FromPrimitive;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, FromPrimitive, Serialize, Deserialize)]
pub enum BuildStatus {
    Pending = 0,
    Running = 1,
    Success = 2,
    Failure = 3,
}

pub enum BuildMethod {
    Url { url: String },
    SrcFile { path: PathBuf, filename: String },
}

pub struct AndaBackend {
    method: BuildMethod,
    build_id: Uuid,
}

impl AndaBackend {
    pub fn new(method: BuildMethod, build_id: Uuid) -> Self {
        AndaBackend { method, build_id }
    }

    pub async fn new_build(source: BuildMethod, project_id: Option<Uuid>) -> Result<Build> {
        let build = Build::new(None, project_id, None, "BuildSubmission".to_string())
            .add()
            .await?;

        Self::new(source, build.id).build().await?;

        Ok(build)
    }

    pub fn new_src_file<T: Into<String>>(path: PathBuf, filename: T, build_id: Uuid) -> Self {
        AndaBackend {
            method: BuildMethod::SrcFile {
                path,
                filename: filename.into(),
            },
            build_id,
        }
    }
    pub fn new_url<T: Into<String>>(url: T, build_id: Uuid) -> Self {
        AndaBackend {
            method: BuildMethod::Url { url: url.into() },
            build_id,
        }
    }

    // Proxy function to the actual build method.
    // Matches the method enum and calls the appropriate method.
    pub async fn build(&self) -> Result<()> {
        match &self.method {
            BuildMethod::Url { url } => {
                self.build_url(url);

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
    pub fn build_url(&self, url: &str) -> Result<()> {
        // call kubernetes to call anda with the params.
        todo!();
    }

    pub fn build_src_file(&self, path: &PathBuf, filename: &str) -> Result<()> {
        println!("Building from src file: {:?}", path);
        println!("actual filename: {}", filename);

        // now check what kind of file it is, so we can determine which build backend to use.
        // match file extension
        if filename.ends_with("src.rpm") {
            // call rpmbuild backend
            panic!("rpmbuild backend not implemented yet");
        } else if filename.ends_with("andasrc.zip") {
            // We have an andaman tarball.
            // create a kubernetes job to build the project, then copy the file.
            // get the path to the copied file, and then call anda to do the work.
            todo!();
        }
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
                filename: art.name.clone().split('/').last().unwrap().to_string(),
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
        .add()
        .await?;

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
            filename,
            path: filepath,
            build_id: artifact_meta.build_id,
            timestamp: artifact_meta.timestamp,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Project {
    pub id: Uuid,
    pub name: String,
    pub description: Option<String>,
}

impl From<crate::db_object::Project> for Project {
    fn from(project: crate::db_object::Project) -> Self {
        Self {
            id: project.id,
            name: project.name,
            description: Some(project.description),
        }
    }
}

impl Project {
    pub fn new(name: String, description: Option<String>) -> Self {
        dotenv::dotenv().ok();
        Self {
            id: Uuid::new_v4(),
            name,
            description,
        }
    }

    pub async fn get(uuid: Uuid) -> Result<Self>
    where
        Self: Sized,
    {
        // Query the database for the project with the given uuid.
        let project_meta = crate::db_object::Project::get(uuid).await?;
        Ok(Self::from(project_meta))
    }

    pub async fn list(limit: usize, page: usize) -> Result<Vec<Self>>
    where
        Self: Sized,
    {
        // Query the database for the projects.
        let projects = crate::db_object::Project::list(limit, page).await?;
        Ok(projects
            .iter()
            .map(|project| Self::from(project.clone()))
            .collect())
    }

    pub async fn add(self) -> Result<Self>
    where
        Self: Sized,
    {
        // Add the project to the database.
        let project_meta =
            crate::db_object::Project::new(self.id, &self.name, self.description.clone().as_ref())
                .add()
                .await?;
        Ok(Self::from(project_meta))
    }

    pub async fn update_name(self, name: String) -> Result<Self>
    where
        Self: Sized,
    {
        // Update the project name in the database.
        let project_meta = crate::db_object::Project::get(self.id)
            .await?
            .update_name(name)
            .await?;

        Ok(Self::from(project_meta))
    }

    pub async fn update_description(self, description: Option<String>) -> Result<Self>
    where
        Self: Sized,
    {
        // Update the project description in the database.
        let project_meta = crate::db_object::Project::get(self.id)
            .await?
            .update_description(description.unwrap_or_else(|| "".to_string()))
            .await?;
        Ok(Self::from(project_meta))
    }

    pub async fn delete(self) -> Result<()>
    where
        Self: Sized,
    {
        // Delete the project from the database.
        crate::db_object::Project::get(self.id)
            .await?
            .delete()
            .await?;
        Ok(())
    }

    pub async fn get_builds(&self) -> Result<Vec<Build>>
    where
        Self: Sized,
    {
        // Query the database for the builds for this project.
        let builds = crate::db_object::Build::get_by_project_id(self.id).await?;
        Ok(builds
            .iter()
            .map(|build| Build::from(build.clone()))
            .collect())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Build {
    pub id: Uuid,
    pub target_id: Option<Uuid>,
    pub project_id: Option<Uuid>,
    pub compose_id: Option<Uuid>,
    pub status: BuildStatus,
    pub timestamp: chrono::DateTime<Utc>,
    pub build_type: String,
}

impl From<crate::db_object::Build> for Build {
    fn from(build: crate::db_object::Build) -> Self {
        Self {
            id: build.id,
            target_id: build.target_id,
            project_id: build.project_id,
            compose_id: build.compose_id,
            status: num::FromPrimitive::from_i32(build.status).unwrap(),
            timestamp: build.timestamp,
            build_type: build.build_type,
        }
    }
}

impl Build {
    pub fn new(
        target_id: Option<Uuid>,
        project_id: Option<Uuid>,
        compose_id: Option<Uuid>,
        build_type: String,
    ) -> Self {
        dotenv::dotenv().ok();
        Self {
            id: Uuid::new_v4(),
            target_id,
            project_id,
            compose_id,
            status: BuildStatus::Pending,
            timestamp: Utc::now(),
            build_type,
        }
    }

    pub async fn get(uuid: Uuid) -> Result<Self>
    where
        Self: Sized,
    {
        // Query the database for the build with the given uuid.
        let build_meta = crate::db_object::Build::get(uuid).await?;
        Ok(Self::from(build_meta))
    }

    pub async fn list(limit: usize, page: usize) -> Result<Vec<Self>>
    where
        Self: Sized,
    {
        // Query the database for the builds.
        let builds = crate::db_object::Build::list(limit, page).await?;
        Ok(builds
            .iter()
            .map(|build| Self::from(build.clone()))
            .collect())
    }

    pub async fn get_by_target_id(target_id: Uuid) -> Result<Vec<Self>>
    where
        Self: Sized,
    {
        // Query the database for the builds with the given target_id.
        let builds = crate::db_object::Build::get_by_target_id(target_id).await?;
        Ok(builds
            .iter()
            .map(|build| Self::from(build.clone()))
            .collect())
    }

    pub async fn get_by_project_id(project_id: Uuid) -> Result<Vec<Self>>
    where
        Self: Sized,
    {
        // Query the database for the builds with the given project_id.
        let builds = crate::db_object::Build::get_by_project_id(project_id).await?;
        Ok(builds
            .iter()
            .map(|build| Self::from(build.clone()))
            .collect())
    }

    pub async fn add(self) -> Result<Self>
    where
        Self: Sized,
    {
        // Add the build to the database.
        let build_meta = crate::db_object::Build::from(self).add().await?;

        Ok(Self::from(build_meta))
    }

    pub async fn update_status(self, status: BuildStatus) -> Result<Self>
    where
        Self: Sized,
    {
        // Update the build status in the database.
        let build_meta = crate::db_object::Build::from(self)
            .update_status(status as i32)
            .await?;
        Ok(Self::from(build_meta))
    }

    pub async fn tag_compose(self, compose_id: Uuid) -> Result<Self>
    where
        Self: Sized,
    {
        // Tag the build with the given compose id.
        let build_meta = crate::db_object::Build::from(self)
            .tag_compose(compose_id)
            .await?;
        Ok(Self::from(build_meta))
    }

    pub async fn tag(self, target_id: Uuid) -> Result<Self>
    where
        Self: Sized,
    {
        // Tags a build with the given target id.
        // if there's already a build with the project id tagged to the target id, the old build will be untagged.
        // this ensures that when building a compose, the process will not try to pull all old builds from the target, but only the latest one.
        if self.project_id.is_some() {
            // find a build for the project with the given target_id
            let build = Self::get_by_target_id(target_id)
                .await?
                .iter().cloned().find(|b| b.project_id == self.project_id);
            if let Some(build) = build {
                // untag the build
                build.untag().await?;
            }
        }

        // Tag the build with the given target id.
        let build_meta = crate::db_object::Build::from(self)
            .tag_target(target_id)
            .await?;
        Ok(Self::from(build_meta))
    }

    pub async fn untag(self) -> Result<Self>
    where
        Self: Sized,
    {
        // Untag the build.
        let build_meta = crate::db_object::Build::from(self).untag_target().await?;
        Ok(Self::from(build_meta))
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
