//! Andaman builder backend
//! This module will be called by the Andaman server to interact with builders
//! It will accept build parameters and return the build object.
//! The builder will then be responsible for building the project.
//! The server will start a Kubernetes job and manage the build process (hopefully).

use std::path::PathBuf;
use std::time::SystemTime;
use crate::{
    db,
    entity::{artifact, build, project, target, compose},
};
use anyhow::{anyhow, Result};
use chrono::{offset::Utc, DateTime};
use aws_sdk_s3::types::{ByteStream, DateTime as AmazonDateTime};
use sea_orm::FromJsonQueryResult;
use tokio::{fs::File, io::{AsyncReadExt, AsyncWriteExt}, process::Command};
use db::DbPool;
use crate::s3_object::{S3Artifact, BUCKET, S3_ENDPOINT};
use sea_orm::{prelude::Uuid, *};
use num_derive::FromPrimitive;
use serde::{Deserialize, Serialize};
//use crate::backend_old::S3Object;

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
            .expires(AmazonDateTime::from(sys_time))
            .send()
            .await?;
        println!("Uploaded {}", dest_path);
        Ok(self)
    }
}



#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Artifact {
    pub id: Uuid,
    pub filename: String,
    pub path: String,
    pub url: String,
    pub build_id: Uuid,
    pub timestamp: DateTime<Utc>,
    pub metadata: Option<serde_json::Value>,
}

/*impl From<crate::backend_old::Artifact> for Artifact {
    fn from(artifact: crate::backend_old::Artifact) -> Self {
        Artifact {
            id: artifact.id,
            filename: artifact.filename,
            url: artifact.get_url(),
            build_id: artifact.build_id,
            timestamp: artifact.timestamp,
            metadata: None,
            // TODO
            path: "".to_string()
        }
    }
}*/

impl From<artifact::Model> for Artifact {
    fn from(model: artifact::Model) -> Self {
        let filepath = model.name.clone();
        let filename = filepath.split('/').last().unwrap().to_string();
        Artifact {
            build_id: model.build_id,
            id: model.id,
            path: filepath,
            filename,
            timestamp: model.timestamp,
            url: model.url,
            metadata: model.metadata,
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
            metadata: None
        }
    }

    pub async fn add(&self) -> Result<Artifact> {
        let db = DbPool::get().await;
        let model = artifact::ActiveModel {
            id: ActiveValue::Set(self.id),
            build_id: ActiveValue::Set(self.build_id),
            name: ActiveValue::Set(self.filename.clone()),
            timestamp: ActiveValue::Set(self.timestamp),
            url: ActiveValue::Set(self.url.clone()),
            metadata: ActiveValue::Set(self.metadata.clone()),
        };
        let ret = artifact::ActiveModel::insert(model, db).await?;
        Ok(Artifact::from(ret))
    }

    /// Gets an artifact by ID
    pub async fn get(id: Uuid) -> Result<Artifact> {
        let db = DbPool::get().await;
        let artifact = artifact::Entity::find_by_id(id)
            .one(db)
            .await?
            .ok_or_else(|| anyhow!("Artifact not found"))?;
        // Marshall the types from our internal representation to the actual DB representation.
        Ok(Artifact::from(artifact))
    }

    /// Lists all available artifact (Paginated)
    pub async fn list(limit: usize, page: usize) -> Result<Vec<Artifact>> {
        let db = DbPool::get().await;
        let artifact = artifact::Entity::find()
            .order_by_desc(artifact::Column::Timestamp)
            .paginate(db, limit)
            .fetch_page(page)
            .await?;
        // Marshall the types from our internal representation to the actual DB representation.
        Ok(artifact.into_iter().map(Artifact::from).collect())
    }

    /// Lists all available artifacts
    pub async fn list_all() -> Result<Vec<Artifact>> {
        let db = DbPool::get().await;
        let artifact = artifact::Entity::find()
            .order_by_desc(artifact::Column::Timestamp)
            .all(db)
            .await?;
        // Marshall the types from our internal representation to the actual DB representation.
        Ok(artifact.into_iter().map(Artifact::from).collect())
    }

    /// Gets an artifact by the build it was associated with (with Build ID)
    pub async fn get_by_build_id(build_id: Uuid) -> Result<Vec<Artifact>> {
        let db = DbPool::get().await;
        let artifact = artifact::Entity::find()
            .filter(artifact::Column::BuildId.eq(build_id))
            .all(db)
            .await?;
        // Marshall the types from our internal representation to the actual DB representation.
        Ok(artifact.into_iter().map(Artifact::from).collect())
    }

    /// Searches for an artifact
    pub async fn search(query: &str) -> Vec<Artifact> {
        let db = DbPool::get().await;
        let artifact = artifact::Entity::find()
            .filter(
                artifact::Column::Url
                    .like(&format!("%{}%", query))
                    .or(artifact::Column::Name.like(&format!("%{}%", query))),
            )
            .all(db)
            .await
            .unwrap();
        // Marshall the types from our internal representation to the actual DB representation.
        artifact.into_iter().map(Artifact::from).collect()
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
        let artifact_meta = crate::backend::Artifact::get(uuid).await?;

        //let filepath = artifact_meta.name.clone();
        //let filename = filepath.strip_prefix("artifacts/").unwrap().to_string();
        //let id = artifact_meta.id;
        Ok(Self::from(artifact_meta))
    }
}


#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Build {
    pub id: Uuid,
    pub status: BuildStatus,
    pub target_id: Option<Uuid>,
    pub project_id: Option<Uuid>,
    pub timestamp: DateTime<Utc>,
    pub compose_id: Option<Uuid>,
    pub build_type: String,
}

impl From<build::Model> for Build {
    fn from(model: build::Model) -> Self {
        Build {
            id: model.id,
            status: num::FromPrimitive::from_i32(model.status).unwrap(),
            target_id: model.target_id,
            project_id: model.project_id,
            timestamp:  model.timestamp,
            compose_id: model.compose_id,
            build_type: model.build_type,
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

    pub async fn add(&self) -> Result<Build> {
        let db = DbPool::get().await;
        let build = build::ActiveModel {
            id: ActiveValue::Set(self.id),
            status: ActiveValue::Set(self.status as i32),
            target_id: ActiveValue::Set(self.target_id),
            timestamp: ActiveValue::Set(self.timestamp),
            build_type: ActiveValue::Set(self.build_type.clone()),
            ..Default::default()
        };
        let res = build::ActiveModel::insert(build, db).await?;
        Ok(Build::from(res))
    }

    pub async fn update_status(&self, status: i32) -> Result<Build> {
        let db = DbPool::get().await;
        let build = build::ActiveModel {
            id: ActiveValue::Set(self.id),
            status: ActiveValue::Set(status),
            ..Default::default()
        };
        let res = build::ActiveModel::update(build, db).await?;
        Ok(Build::from(res))
    }

    pub async fn update_type(&self, build_type: &str) -> Result<Build> {
        let db = DbPool::get().await;
        let build = build::ActiveModel {
            id: ActiveValue::Set(self.id),
            build_type: ActiveValue::Set(build_type.to_string()),
            ..Default::default()
        };
        let res = build::ActiveModel::update(build, db).await?;
        Ok(Build::from(res))
    }

    pub async fn tag_compose(&self, compose_id: Uuid) -> Result<Build> {
        let db = DbPool::get().await;
        let build = build::ActiveModel {
            id: ActiveValue::Set(self.id),
            compose_id: ActiveValue::Set(Some(compose_id)),
            ..Default::default()
        };
        let res = build::ActiveModel::update(build, db).await?;
        Ok(Build::from(res))
    }

    pub async fn tag_target(&self, target_id: Uuid) -> Result<Build> {
        let db = DbPool::get().await;
        let build = build::ActiveModel {
            id: ActiveValue::Set(self.id),
            target_id: ActiveValue::Set(Some(target_id)),
            ..Default::default()
        };
        let res = build::ActiveModel::update(build, db).await?;
        Ok(Build::from(res))
    }

    pub async fn untag_target(&self) -> Result<Build> {
        let db = DbPool::get().await;
        let build = build::ActiveModel {
            id: ActiveValue::Set(self.id),
            target_id: ActiveValue::Set(None),
            ..Default::default()
        };
        let res = build::ActiveModel::update(build, db).await?;
        Ok(Build::from(res))
    }

    pub async fn tag_project(&self, project_id: Uuid) -> Result<Build> {
        let db = DbPool::get().await;
        let build = build::ActiveModel {
            id: ActiveValue::Set(self.id),
            project_id: ActiveValue::Set(Some(project_id)),
            ..Default::default()
        };
        let res = build::ActiveModel::update(build, db).await?;
        Ok(Build::from(res))
    }

    /// Gets a build by ID
    pub async fn get(id: Uuid) -> Result<Build> {
        let db = DbPool::get().await;
        let build = build::Entity::find_by_id(id)
            .one(db)
            .await?
            .ok_or_else(|| anyhow!("Build not found"))?;
        Ok(Build::from(build))
    }

    pub async fn list(limit: usize, page: usize) -> Result<Vec<Build>> {
        let db = DbPool::get().await;
        let build = build::Entity::find()
            .order_by(build::Column::Timestamp, Order::Desc)
            .paginate(db, limit)
            .fetch_page(page)
            .await?;

        Ok(build.into_iter().map(Build::from).collect())
    }

    pub async fn list_all() -> Result<Vec<Build>> {
        let db = DbPool::get().await;
        let build = build::Entity::find()
            .order_by(build::Column::Timestamp, Order::Desc)
            .all(db)
            .await?;
        Ok(build.into_iter().map(Build::from).collect())
    }

    pub async fn get_by_target_id(target_id: Uuid) -> Result<Vec<Build>> {
        let db = DbPool::get().await;
        let build = build::Entity::find()
            .order_by(build::Column::Timestamp, Order::Desc)
            .filter(build::Column::TargetId.eq(target_id))
            .all(db)
            .await?;
        Ok(build.into_iter().map(Build::from).collect())
    }

    pub async fn get_by_project_id(project_id: Uuid) -> Result<Vec<Build>> {
        let db = DbPool::get().await;
        let build = build::Entity::find()
            .order_by(build::Column::Timestamp, Order::Desc)
            .filter(build::Column::ProjectId.eq(project_id))
            .all(db)
            .await?;
        Ok(build.into_iter().map(Build::from).collect())
    }

    pub async fn get_by_compose_id(compose_id: Uuid) -> Result<Vec<Build>> {
        let db = DbPool::get().await;
        let build = build::Entity::find()
            .order_by(build::Column::Timestamp, Order::Desc)
            .filter(build::Column::ComposeId.eq(compose_id))
            .all(db)
            .await?;
        Ok(build.into_iter().map(Build::from).collect())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Project {
    pub id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub summary: Option<String>
}

impl From<project::Model> for Project {
    fn from(model: project::Model) -> Self {
        Project {
            id: model.id,
            name: model.name,
            description: model.description,
            summary: model.summary
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
            summary: None,
        }
    }

    pub async fn add(&self) -> Result<Project> {
        let db = DbPool::get().await;
        let project = project::ActiveModel {
            id: ActiveValue::Set(self.id),
            name: ActiveValue::Set(self.name.clone()),
            description: ActiveValue::Set(self.description.clone()),
            summary: ActiveValue::Set(self.summary.clone()),
        };
        let res = project::ActiveModel::insert(project, db).await?;
        Ok(Project::from(res))
    }

    /// Gets a project by ID
    pub async fn get(id: Uuid) -> Result<Project> {
        let db = DbPool::get().await;
        let project = project::Entity::find_by_id(id)
            .one(db)
            .await?
            .ok_or_else(|| anyhow!("Project not found"))?;
        Ok(Project::from(project))
    }

    pub async fn list(limit: usize, page: usize) -> Result<Vec<Project>> {
        let db = DbPool::get().await;
        let project = project::Entity::find()
            .paginate(db, limit)
            .fetch_page(page)
            .await?;
        Ok(project.into_iter().map(Project::from).collect())
    }

    pub async fn list_all() -> Result<Vec<Project>> {
        let db = DbPool::get().await;
        let project = project::Entity::find()
            .all(db)
            .await?;
        Ok(project.into_iter().map(Project::from).collect())
    }

    pub async fn list_artifacts(&self) -> Result<Vec<Artifact>> {
        let db = DbPool::get().await;
        let builds = Build::get_by_project_id(self.id).await?;

        let mut artifacts = Vec::new();

        for build in builds {
            let art = Artifact::get_by_build_id(build.id).await?;
            artifacts.extend(art)
        }

        artifacts.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
        Ok(artifacts)
    }

    pub async fn update_name(&self, name: String) -> Result<Project> {
        let db = DbPool::get().await;
        let project = project::ActiveModel {
            id: ActiveValue::Set(self.id),
            name: ActiveValue::Set(name),
            ..Default::default()
        };
        let res = project::ActiveModel::update(project, db).await?;
        Ok(Project::from(res))
    }

    pub async fn update_description(&self, description: String) -> Result<Project> {
        let db = DbPool::get().await;
        let project = project::ActiveModel {
            id: ActiveValue::Set(self.id),
            description: ActiveValue::Set(Some(description)),
            ..Default::default()
        };
        let res = project::ActiveModel::update(project, db).await?;
        Ok(Project::from(res))
    }

    pub async fn update_summary(&self, summary: Option<String>) -> Result<Project> {
        let db = DbPool::get().await;
        let project = project::ActiveModel {
            id: ActiveValue::Set(self.id),
            summary: ActiveValue::Set(summary),
            ..Default::default()
        };
        let res = project::ActiveModel::update(project, db).await?;
        Ok(Project::from(res))
    }

    pub async fn delete(&self) -> Result<()> {
        let db = DbPool::get().await;
        // check if project exists
        let _ = project::Entity::find_by_id(self.id)
            .one(db)
            .await?
            .ok_or_else(|| anyhow!("Project not found"))?;
        project::Entity::delete_by_id(self.id).exec(db).await?;
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Compose {
    pub id: Uuid,
    pub compose_ref: Option<String>,
    pub target_id: Uuid,
    pub timestamp: DateTime<Utc>,
}

impl From<compose::Model> for Compose {
    fn from(model: compose::Model) -> Self {
        Compose {
            id: model.id,
            compose_ref: model.compose_ref,
            target_id: model.project_id,
            timestamp: model.timestamp,
        }
    }
}

/*impl From<crate::backend_old::Compose> for Compose {
    fn from(model: crate::backend_old::Compose) -> Self {
        Compose {
            id: model.id,
            compose_ref: model.compose_ref,
            target_id: model.target_id,
            timestamp: model.timestamp,
        }
    }
}*/

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


    pub async fn add(&self) -> Result<Compose> {
        let db = DbPool::get().await;
        let compose = compose::ActiveModel {
            id: ActiveValue::Set(self.id),
            compose_ref: ActiveValue::Set(self.compose_ref.clone()),
            project_id: ActiveValue::Set(self.target_id),
            timestamp: ActiveValue::Set(self.timestamp),
            ..Default::default()
        };
        let res = compose::ActiveModel::insert(compose, db).await?;
        Ok(Compose::from(res))
    }

    /// Get compose by ID
    pub async fn get(id: Uuid) -> Result<Compose> {
        let db = DbPool::get().await;
        let compose = compose::Entity::find_by_id(id)
            .one(db)
            .await?
            .ok_or_else(|| anyhow!("Compose not found"))?;
        Ok(Compose::from(compose))
    }

    pub async fn list(limit: usize, page: usize) -> Result<Vec<Compose>> {
        let db = DbPool::get().await;
        let compose = compose::Entity::find()
            .paginate(db, limit)
            .fetch_page(page)
            .await?;
        Ok(compose.into_iter().map(Compose::from).collect())
    }

    pub async fn list_all() -> Result<Vec<Compose>> {
        let db = DbPool::get().await;
        let compose = compose::Entity::find()
            .all(db)
            .await?;
        Ok(compose.into_iter().map(Compose::from).collect())
    }

    pub async fn update(&self) -> Result<Compose> {
        let db = DbPool::get().await;
        let compose = compose::ActiveModel {
            id: ActiveValue::Set(self.id),
            compose_ref: ActiveValue::Set(self.compose_ref.clone()),
            project_id: ActiveValue::Set(self.target_id),
            timestamp: ActiveValue::Set(self.timestamp),
        };
        let res = compose::ActiveModel::update(compose, db).await?;
        Ok(Compose::from(res))
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
            let artifacts = Artifact::get_by_build_id(build.id).await?;
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

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Target {
    pub id: Uuid,
    pub name: String,
    pub image: Option<String>,
    pub arch: String,
}

/*impl From<crate::backend_old::Target> for Target {
    fn from(model: crate::backend_old::Target) -> Self {
        Target {
            id: model.id,
            name: model.name,
            image: model.image,
            arch: model.arch,
        }
    }
}*/

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

    pub fn from_model(model: target::Model) -> Self {
        Self {
            id: model.id,
            name: model.name,
            image: model.image,
            arch: model.arch,
        }
    }

    pub async fn add(&self) -> Result<Target> {
        let db = DbPool::get().await;
        let target = target::ActiveModel {
            id: ActiveValue::Set(self.id),
            name: ActiveValue::Set(self.name.clone()),
            image: ActiveValue::Set(self.image.clone()),
            arch: ActiveValue::Set(self.arch.clone()),
        };
        let res = target::ActiveModel::insert(target, db).await?;
        Ok(Target::from_model(res))
    }

    pub async fn get(id: Uuid) -> Result<Target> {
        let db = DbPool::get().await;
        let target = target::Entity::find_by_id(id)
            .one(db)
            .await?
            .ok_or_else(|| {
                error!("Target not found");
                anyhow!("Target not found")
            })?;
        Ok(Target::from_model(target))
    }

    pub async fn list(limit: usize, page: usize) -> Result<Vec<Target>> {
        let db = DbPool::get().await;
        let target = target::Entity::find()
            .paginate(db, limit)
            .fetch_page(page)
            .await?;
        Ok(target.into_iter().map(Target::from_model).collect())
    }

    pub async fn list_all() -> Result<Vec<Target>> {
        let db = DbPool::get().await;
        let target = target::Entity::find()
            .all(db)
            .await?;
        Ok(target.into_iter().map(Target::from_model).collect())
    }

    pub async fn update(&self, _id: Uuid) -> Result<Target> {
        let db = DbPool::get().await;
        // get target by id, then update it
        let target = target::ActiveModel {
            id: ActiveValue::Set(self.id),
            name: ActiveValue::Set(self.name.clone()),
            image: ActiveValue::Set(self.image.clone()),
            arch: ActiveValue::Set(self.arch.clone()),
        };
        let res = target::ActiveModel::update(target, db).await?;
        Ok(Target::from_model(res))
    }

    pub async fn delete(&self) -> Result<()> {
        let db = DbPool::get().await;
        // check if target exists
        let _ = target::Entity::find_by_id(self.id)
            .one(db)
            .await?
            .ok_or_else(|| anyhow!("Target not found"))?;
        target::Entity::delete_by_id(self.id).exec(db).await?;
        Ok(())
    }

    pub async fn get_by_name(name: String) -> Result<Target> {
        let db = DbPool::get().await;
        let target = target::Entity::find()
            .filter(target::Column::Name.eq(name))
            .one(db)
            .await?
            .ok_or_else(|| anyhow!("Target not found"))?;
        Ok(Target::from_model(target))
    }
}
