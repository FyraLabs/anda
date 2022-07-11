// This code is licensed under the MIT License.
// Copyright (c) 2022 the Ultramarine Project and Fyra Labs.

use entity::*;
use sea_orm::{*, prelude::{DateTimeUtc, DateTimeLocal, DateTimeWithTimeZone}};
use crate::{entity::{artifacts, builds, projects}, db};

use db::DbPool;

use anyhow::{anyhow, Result};

use futures::future;

use serde_derive::{Deserialize, Serialize};


#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Artifact {
    pub art_type: Option<String>,
    pub id: String,
    pub build_id: Option<i32>,
    pub name: Option<String>,
    pub timestamp: Option<DateTimeWithTimeZone>,
}
impl Artifact {

    pub fn new(id: String, art_type: Option<String>, name: Option<String>) -> Self {
        Self {
            art_type,
            id,
            build_id: None,
            name,
            timestamp: None,
        }
    }

    fn from_model(model: artifacts::Model) -> Result<Artifact> {
        Ok(Artifact {
            art_type: model.art_type,
            id: model.id,
            build_id: model.from_build,
            name: model.name,
            timestamp: model.timestamp,
        })
    }

    pub async fn add(&self) -> Result<Artifact> {
        let db = DbPool::get().await;
        let model = artifacts::ActiveModel {
            art_type: ActiveValue::Set(self.art_type.clone()),
            id: ActiveValue::Set(self.id.clone()),
            from_build: ActiveValue::Set(self.build_id),
            name: ActiveValue::Set(self.name.clone()),
            ..Default::default()
        };
        let ret = artifacts::ActiveModel::insert(model, db).await?;
        Artifact::from_model(ret)
    }

    /// Gets an artifact by ID
    pub async fn get(id: &str) -> Result<Artifact> {
        let db = DbPool::get().await;
        let artifact = artifacts::Entity::find_by_id(id.to_string())
            .one(db)
            .await?
            .ok_or(anyhow!("Artifact not found"))?;
        // Marshall the types from our internal representation to the actual DB representation.
        Ok(Artifact::from_model(artifact).unwrap())
    }

    /// Lists all available artifacts
    pub async fn list(limit: u64, offset: u64) -> Result<Vec<Artifact>> {

        let db = DbPool::get().await;
        let artifacts = artifacts::Entity::find()
            .limit(limit)
            .offset(offset)
            .all(db)
            .await
            .unwrap();
        // Marshall the types from our internal representation to the actual DB representation.
        Ok(artifacts.into_iter().map(|artifact| Artifact::from_model(artifact).unwrap()).collect())
    }

    /// Gets an artifact by the build it was associated with (with Build ID)
    pub async fn get_by_build_id(build_id: i32) -> Result<Vec<Artifact>> {
        let db = DbPool::get().await;
        let artifacts = artifacts::Entity::find()
            .filter(artifacts::Column::FromBuild.eq(build_id))
            .all(db)
            .await
            .unwrap();
        // Marshall the types from our internal representation to the actual DB representation.
        Ok(artifacts.into_iter().map(|artifact| Artifact::from_model(artifact).unwrap()).collect())
    }

    pub async fn get_by_type(art_type: &str, limit: u64, offset: u64) -> Result<Vec<Artifact>> {
        let db = DbPool::get().await;
        let artifacts = artifacts::Entity::find()
            .filter(artifacts::Column::ArtType.eq(art_type))
            .limit(limit)
            .offset(offset)
            .all(db)
            .await
            .unwrap();
        // Marshall the types from our internal representation to the actual DB representation.
        Ok(artifacts.into_iter().map(|artifact| Artifact::from_model(artifact).unwrap()).collect())
    }

    /// Searches for an artifact
    pub async fn search(query: &str) -> Result<Vec<Artifact>> {
        let db = DbPool::get().await;
        let artifacts = artifacts::Entity::find()
            .filter(artifacts::Column::Name.like(query))
            .all(db)
            .await
            .unwrap();
        // Marshall the types from our internal representation to the actual DB representation.
        Ok(artifacts.into_iter().map(|artifact| Artifact::from_model(artifact).unwrap()).collect())
    }

}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Build {
    pub id: i32,
    pub name: String,
    pub package_id: Option<i32>,
    pub build_type: String,
    pub owner: Option<i32>,
    pub version: String,
    pub timestamp: DateTimeWithTimeZone,
    pub target_id: Option<i32>,
    pub status: Option<String>,
    pub worker: Option<i32>,
}

impl Build {
    /// Import from ORM model
    async fn from_model(model: builds::Model) -> Result<Build> {
        Ok(Build {
            id: model.id,
            name: model.name,
            package_id: model.package_id,
            build_type: model.build_type,
            owner: model.owner,
            version: model.version,
            timestamp: model.timestamp,
            target_id: model.for_target,
            status: model.status,
            worker: model.worker,
        })
    }

    pub fn new(name: &str, package_id: Option<i32>, build_type: &str, owner: Option<i32>, version: &str, timestamp: DateTimeWithTimeZone, target_id: Option<i32>) -> Build {
        Build {
            id: 0,
            name: name.to_string(),
            package_id,
            build_type: build_type.to_string(),
            owner,
            version: version.to_string(),
            timestamp,
            target_id,
            status: None,
            worker: None,
        }
    }

    pub async fn add(&self) -> Result<Build> {
        let db = DbPool::get().await;
        let build = builds::ActiveModel {
            id: ActiveValue::Set(self.id),
            name: ActiveValue::Set(self.name.clone()),
            for_target: ActiveValue::Set(self.target_id),
            owner: ActiveValue::Set(self.owner),
            package_id: ActiveValue::Set(self.package_id),
            timestamp: ActiveValue::Set(self.timestamp),
            version: ActiveValue::Set(self.version.clone()),
            build_type: ActiveValue::Set(self.build_type.clone()),
            status: ActiveValue::Set(self.status.clone()),
            ..Default::default()
        };
        let res = builds::ActiveModel::insert(build, db).await?;
        Build::from_model(res).await
    }

    /// Gets a build by ID
    pub async fn get(id: i32) -> Result<Build> {
        let db = DbPool::get().await;
        let build = builds::Entity::find_by_id(id)
            .one(db)
            .await?
            .ok_or(anyhow!("Build not found"))?;
        Ok(Build::from_model(build).await.unwrap())
    }

    pub async fn list(limit: u64, offset: u64) -> Result<Vec<Build>> {
        let db = DbPool::get().await;
        let builds = builds::Entity::find()
            .order_by(builds::Column::Timestamp, Order::Desc)
            .limit(limit)
            .offset(offset)
            .all(db)
            .await?;

        Ok(
            future::try_join_all(
                builds.into_iter().map(|build| {
                    Build::from_model(build)
                })
            ).await.unwrap()
        )
    }

    /// Gets a build by the owner that built it
    pub async fn get_by_owner_id(owner_id: i32) -> Result<Vec<Build>> {
        let db = DbPool::get().await;
        let builds = builds::Entity::find()
            .order_by(builds::Column::Timestamp, Order::Desc)
            .filter(builds::Column::Owner.eq(owner_id))
            .all(db)
            .await?;
        Ok(
            future::try_join_all(
                builds.into_iter().map(|build| {
                    Build::from_model(build)
                })
            ).await.unwrap()
        )
    }

    pub async fn get_by_target_id(target_id: i32) -> Result<Vec<Build>> {
        let db = DbPool::get().await;
        let builds = builds::Entity::find()
            .order_by(builds::Column::Timestamp, Order::Desc)
            .filter(builds::Column::ForTarget.eq(target_id))
            .all(db)
            .await?;
        Ok(
            future::try_join_all(
                builds.into_iter().map(|build| {
                    Build::from_model(build)
                })
            ).await.unwrap()
        )
    }

    pub async fn get_by_project_id(project_id: i32) -> Result<Vec<Build>> {
        let db = DbPool::get().await;
        let builds = builds::Entity::find()
            .order_by(builds::Column::Timestamp, Order::Desc)
            .filter(builds::Column::PackageId.eq(project_id))
            .all(db)
            .await?;
        Ok(
            future::try_join_all(
                builds.into_iter().map(|build| {
                    Build::from_model(build)
                })
            ).await.unwrap()
        )
    }

}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Project {
    pub id: i32,
    pub name: String,
    pub description: Option<String>,
    pub builds: Vec<Build>,
    pub latest_build: Option<Build>,
}

impl Project {

    fn new(name: &str, description: Option<&str>) -> Project {
        // The resulting project is not ready for use, and does not have an ID.
        Project {
            id: 0,
            name: name.to_string(),
            description: description.map(|desc| desc.to_string()),
            builds: Vec::new(),
            latest_build: None,
        }
    }

    async fn add(&self) -> Result<Project> {
        let db = DbPool::get().await;
        let project = projects::ActiveModel {
            name: ActiveValue::Set(self.name.clone()),
            description: ActiveValue::Set(self.description.clone()),
            ..Default::default()
        };
        let res = projects::ActiveModel::insert(project, db).await?;
        Project::from_model(res).await
    }

    /// Import from ORM model
    async fn from_model(model: projects::Model) -> Result<Project> {
        Ok(Project {
            id: model.id,
            name: model.name,
            description: model.description,
            builds: Build::get_by_project_id(model.id).await.unwrap(),
            latest_build: Build::get_by_project_id(model.id).await.unwrap().pop(),
        })
    }

    /// Gets a project by ID
    pub async fn get(id: i32) -> Result<Project> {
        let db = DbPool::get().await;
        let project = projects::Entity::find_by_id(id)
            .one(db)
            .await?
            .ok_or(anyhow!("Project not found"))?;
        Ok(Project::from_model(project).await.unwrap())
    }

    pub async fn list(limit: u64, offset: u64) -> Result<Vec<Project>> {
        let db = DbPool::get().await;
        let projects = projects::Entity::find()
            .limit(limit)
            .offset(offset)
            .all(db)
            .await?;
        Ok(
            future::try_join_all(
                projects.into_iter().map(|project| {
                    Project::from_model(project)
                })
            ).await.unwrap()
        )
    }

}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Compose {
    pub id: String,
    pub target_id: i32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Target {
    pub id: String,
    pub packages: Vec<Project>,
    pub external_repos: Vec<String>,
}