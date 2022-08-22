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
use tokio::{fs::File, io::{AsyncReadExt, AsyncWriteExt}, process::Command};

use crate::s3_object::{S3Artifact, BUCKET, S3_ENDPOINT};
use num_derive::FromPrimitive;
use serde::{Deserialize, Serialize};

use crate::kubernetes::dispatch_build;

#[derive(Debug, Clone, Copy, PartialEq, Eq, FromPrimitive, Serialize, Deserialize)]
pub enum BuildStatus {
    Pending = 0,
    Running = 1,
    Success = 2,
    Failure = 3,
}

pub struct AndaBackend {
    build_id: Uuid,
    pack: BuildCache,
    image: String,
}

impl AndaBackend {
    pub fn new(build_id: Uuid, pack: BuildCache, image: String) -> Self {
        AndaBackend {
            build_id,
            pack,
            image,
        }
    }

    // Proxy function to the actual build method.
    // Matches the method enum and calls the appropriate method.
    pub async fn build(&self, project_scope: Option<&str>) -> Result<()> {
        dispatch_build(
            self.build_id.to_string(),
            self.image.to_string(),
            self.pack.get_url(),
            "owo".to_string(),
            project_scope.map(|s| s.to_string()),
        )
        .await?;
        Ok(())
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

/// Build caches
/// ----------------
/// Build caches are temporary files that are uploaded to S3.
/// They are used when one uploads a build to the server.
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
        let mut bytes = Vec::with_capacity(metadata.len() as usize);
        file.read_to_end(&mut bytes).await?;

        let obj = crate::s3_object::S3Artifact::new()?;
        let dest_path = format!("build_cache/{}/{}", self.id.simple(), self.filename);
        let chrono_time = chrono::Utc::now() + chrono::Duration::days(7);
        //let aws_time = DateTime::from_chrono_utc(chrono_time);
        // convert chrono time to system time
        let sys_time = SystemTime::from(chrono_time);
        obj.connection
            .put_object()
            .body(bytes.into())
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

/// Artifacts API
/// ------------------------------
/// Artifacts are files that are outputted by the build process.
/// They are collected from the build process and automatically uploaded to S3.
/// Special non-file artifacts are also supported. These are:
/// - Docker/OCI images
/// - OSTree composes
/// These will be uploaded in a different way, and will be stored in a different location.
/// The artifacts will have a different kind of path for each type.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Artifact {
    pub id: Uuid,
    pub filename: String,
    pub path: String,
    pub url: String,
    pub build_id: Uuid,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

impl From<crate::db_object::Artifact> for Artifact {
    fn from(art: crate::db_object::Artifact) -> Self {
        let filepath = art.name.clone();
        let filename = filepath.split("/").last().unwrap().to_string();
        //let id = art.id;

        Self {
            id: art.id,
            filename,
            path: filepath,
            url: art.url.clone(),
            build_id: art.build_id,
            timestamp: art.timestamp,
        }
    }
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
            url: String::new(),
        }
    }

    pub async fn get_for_build(build_id: Uuid) -> Result<Vec<Self>> {
        let arts = crate::db_object::Artifact::get_by_build_id(build_id).await?;

        Ok(arts
            .iter()
            .map(|art| Self::from(art.clone()))
            .collect())
    }

    pub async fn metadata(&self) -> Result<crate::db_object::Artifact> {
        crate::db_object::Artifact::get(self.id).await
    }

    pub async fn list(limit: usize, page: usize) -> Result<Vec<Self>>
    where
        Self: Sized,
    {
        // Query the database for the projects.
        let artifacts = crate::db_object::Artifact::list(limit, page).await?;

        Ok(artifacts
            .iter()
            .map(|art| Self::from(art.clone()))
            .collect()
        )
    }

    pub async fn search(query: &str) -> Vec<Self> {
        let artifacts = crate::db_object::Artifact::search(query).await;
        artifacts
            .iter()
            .map(|art| Self::from(art.clone()))
            .collect()
    }

    pub async fn add(&self) -> Result<Self> {
        let a = crate::db_object::Artifact::add(&crate::db_object::Artifact::from(self.clone())).await;
        Ok(Self::from(a?))
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
    async fn upload_file(mut self, path: PathBuf) -> Result<Self> {
        let obj = crate::s3_object::S3Artifact::new()?;
        let dest_path = format!("artifacts/{}/{}", self.id.simple(), self.path);
        let _ = obj.upload_file(&dest_path, path.to_owned()).await?;
        // now update the database
        /* crate::db_object::Artifact::new(
            self.id,
            self.build_id,
            dest_path
                .strip_prefix(&format!("artifacts/{}/", self.id.simple()))
                .unwrap()
                .to_string(),
            self.get_url(),
        )
        .add()
        .await?; */

        self.path = dest_path
        .strip_prefix(&format!("artifacts/{}/", self.id.simple()))
        .unwrap()
        .to_string();

        self.url = self.get_url();

        println!("Uploaded {}", dest_path);
        Ok(self.add().await?)
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

        //let filepath = artifact_meta.name.clone();
        //let filename = filepath.strip_prefix("artifacts/").unwrap().to_string();
        //let id = artifact_meta.id;
        Ok(Self::from(artifact_meta))
    }
}


/// Projects API
/// ------------
/// Projects are the top organizational unit in the system.
/// They represent various projects that will be organized
/// and then collected into a compose.
///
/// A project can have a description, a name, and an ID.
/// You should be able to create a project, and then tag various targets
/// for that project.
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

    pub async fn list_artifacts(&self) -> Result<Vec<Artifact>>
    where
        Self: Sized,
    {
        // get all the builds tagged with this project
        let builds = crate::db_object::Build::get_by_project_id(self.id).await?;
        let mut artifacts = Vec::new();
        for build in builds {
            artifacts.extend(Artifact::get_for_build(build.id).await?);
        }
        Ok(artifacts)

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

/// Builds API
/// ----------
/// Builds are the main entry point for Andaman.
/// They are tasks that are executed by the build system.
/// Its outputs are artifacts that are stored in S3.
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

    pub async fn get_by_compose_id(compose_id: Uuid) -> Result<Vec<Self>>
    where
        Self: Sized,
    {
        // Query the database for the builds with the given compose_id.
        let builds = crate::db_object::Build::get_by_compose_id(compose_id).await?;
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

    pub async fn tag_project(self, project_id: Uuid) -> Result<Self>
    where
        Self: Sized,
    {
        // Tag the build with the given project id.
        let build_meta = crate::db_object::Build::from(self)
            .tag_project(project_id)
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
                .iter()
                .cloned()
                .find(|b| b.project_id == self.project_id);
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

/// Target API
/// ----------
/// Targets are where the builds are targeted for.
/// A target can be a specific distribution version, or platorm
/// with a specific architecture.
/// There can also be a special docker image option for the target,
/// where native packages can be built from.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Target {
    pub id: Uuid,
    pub name: String,
    pub image: Option<String>,
    pub arch: String,
}

impl From<crate::db_object::Target> for Target {
    fn from(target: crate::db_object::Target) -> Self {
        Self {
            id: target.id,
            name: target.name,
            image: target.image,
            arch: target.arch,
        }
    }
}

impl Target {
    pub fn new(name: String, image: Option<String>, arch: String) -> Self {
        dotenv::dotenv().ok();
        Self {
            id: Uuid::new_v4(),
            name,
            image,
            arch,
        }
    }

    pub async fn get(uuid: Uuid) -> Result<Self>
    where
        Self: Sized,
    {
        //debug!("Getting target with uuid: {}", uuid);
        //let uuid_string = uuid.to_string();
        // Query the database for the target with the given uuid.
        let target = crate::db_object::Target::get(uuid).await?;
        Ok(Self::from(target))
    }

    pub async fn list(limit: usize, page: usize) -> Result<Vec<Self>>
    where
        Self: Sized,
    {
        // Query the database for the targets.
        let targets = crate::db_object::Target::list(limit, page).await?;
        Ok(targets
            .iter()
            .map(|target| Self::from(target.clone()))
            .collect())
    }

    pub async fn add(self) -> Result<Self>
    where
        Self: Sized,
    {
        // Add the target to the database.
        let target = crate::db_object::Target::from(self).add().await?;
        Ok(Self::from(target))
    }

    pub async fn get_by_name(name: String) -> Result<Self>
    where
        Self: Sized,
    {
        // Query the database for the target with the given name.
        let target = crate::db_object::Target::get_by_name(name).await?;
        Ok(Self::from(target))
    }

    pub async fn update(self, id: Uuid) -> Result<Self>
    where
        Self: Sized,
    {
        // Update the target in the database.
        let target = crate::db_object::Target::from(self).update(id).await?;
        Ok(Self::from(target))
    }

    pub async fn delete(self) -> Result<()>
    where
        Self: Sized,
    {
        // Delete the target from the database.
        crate::db_object::Target::from(self).delete().await
    }
}

/// Compose API
/// ------------
/// Composes are a way to group build artifacts together.
/// A compose is a collection of builds made for a specific target.
/// Its output is a folder containing all the artifacts from all the tagged builds
/// compiled into a repository.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Compose {
    pub id: Uuid,
    pub compose_ref: Option<String>,
    pub target_id: Uuid,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

impl From<crate::db_object::Compose> for Compose {
    fn from(compose: crate::db_object::Compose) -> Self {
        Self {
            id: compose.id,
            compose_ref: compose.compose_ref,
            target_id: compose.target_id,
            timestamp: compose.timestamp,
        }
    }
}


impl Compose {
    pub fn new(target_id: Uuid) -> Self {
        dotenv::dotenv().ok();
        Self {
            id: Uuid::new_v4(),
            compose_ref: None,
            target_id,
            timestamp: chrono::Utc::now(),
        }
    }
    pub async fn get(uuid: Uuid) -> Result<Self>
    where
        Self: Sized,
    {
        // Query the database for the compose with the given uuid.
        let compose = crate::db_object::Compose::get(uuid).await?;
        Ok(Self::from(compose))
    }
    pub async fn list(limit: usize, page: usize) -> Result<Vec<Self>>
    where
        Self: Sized,
    {
        // Query the database for the composes.
        let composes = crate::db_object::Compose::list(limit, page).await?;
        Ok(composes
            .iter()
            .map(|compose| Self::from(compose.clone()))
            .collect())
    }
    pub async fn add(self) -> Result<Self>
    where
        Self: Sized,
    {
        // Add the compose to the database.
        let compose = crate::db_object::Compose::from(self).add().await?;
        Ok(Self::from(compose))
    }
    pub async fn update(self, id: Uuid) -> Result<Self>
    where
        Self: Sized,
    {
        // Update the compose in the database.
        let compose = crate::db_object::Compose::from(self).update().await?;
        Ok(Self::from(compose))
    }

    pub async fn get_builds(self) -> Result<Vec<Build>>
    where
        Self: Sized,
    {
        // Query the database for the builds tagged with the compose.
        let builds = Build::get_by_compose_id(self.id).await?;
        Ok(builds)
    }

    pub async fn tag_builds(self) -> Result<()>
    where
        Self: Sized,
    {
        // Tag the builds with the compose.
        let builds = Build::get_by_target_id(self.target_id).await?;

        for build in builds {
            build.tag_compose(self.id).await?;
        }
        Ok(())
    }

    pub async fn compose(self) -> Result<()>
    where
        Self: Sized,
    {
        // TODO: probably move this to a dedicated compose function/executable.

        let tmpdir = tempfile::tempdir()?;

        let rpmdir = tmpdir.path().join("rpm");

        let builds = Build::get_by_compose_id(self.id).await?;
        for build in builds {
            let artifacts = Artifact::get_for_build(build.id).await?;
            for artifact in artifacts {
                let filename = &artifact.filename;

                // download the artifacts
                if filename.ends_with(".rpm") {
                    let pkgs_dir = tmpdir.path().join("packages");

                    // create rpmdir if it doesn't exist
                    if !pkgs_dir.exists() {
                        tokio::fs::create_dir_all(&pkgs_dir).await?;
                    }
                    // download the rpm artifact
                    let rpm_path = pkgs_dir.join(filename);
                    // get the file stream from the artifact
                    let stream = artifact.pull_bytes().await?;
                    // write the stream to the rpm file
                    let mut rpm_file = File::create(&rpm_path).await?;
                    let mut buf = vec![];

                    stream.into_async_read().read_to_end(&mut buf).await?;

                    rpm_file.write_all(&buf).await?;

                }

            }
            // Compile the RPMs into a repository
            // check if there is an rpmdir
            if rpmdir.exists() {
                // use createrepo to create the repository
                let mut output = Command::new("createrepo")
                    .arg(".")
                    .current_dir(&rpmdir)
                    .spawn()?;
                let status = output.wait().await?;
            }

        }
        Ok(())
    }
}
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
