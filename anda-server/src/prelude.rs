// This code is licensed under the MIT License.
// Copyright (c) 2022 the Ultramarine Project and Fyra Labs.

use entity::*;
use sea_orm::{*, prelude::{DateTimeWithTimeZone, DateTime, Uuid}};
use crate::{entity::{artifact, build, project}, db};
use chrono::offset::Utc;

use db::DbPool;

use anyhow::{anyhow, Result};

use futures::future;

use serde_derive::{Deserialize, Serialize};


#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Artifact {
    pub id: Uuid,
    pub name: String,
    pub url: String,
    pub build_id: Uuid,
    pub timestamp: DateTime,
}
impl Artifact {

    pub fn new(id: Uuid,build_id: Uuid, name: String, url: String) -> Self {
        Self {
            id,
            build_id,
            name,
            timestamp: Utc::now().naive_utc(),
            url
        }
    }

    fn from_model(model: artifact::Model) -> Result<Artifact> {
        Ok(Artifact {
            build_id: model.build_id,
            id: model.id,
            name: model.name,
            timestamp: model.timestamp,
            url: model.url,
        })
    }

    pub async fn add(&self) -> Result<Artifact> {
        let db = DbPool::get().await;
        let model = artifact::ActiveModel {
            id: ActiveValue::Set(self.id),
            build_id: ActiveValue::Set(self.build_id),
            name: ActiveValue::Set(self.name.clone()),
            timestamp: ActiveValue::Set(self.timestamp),
            url: ActiveValue::Set(self.url.clone()),
            ..Default::default()
        };
        let ret = artifact::ActiveModel::insert(model, db).await?;
        Artifact::from_model(ret)
    }

    /// Gets an artifact by ID
    pub async fn get(id: Uuid) -> Result<Artifact> {
        let db = DbPool::get().await;
        let artifact = artifact::Entity::find_by_id(id)
            .one(db)
            .await?
            .ok_or(anyhow!("Artifact not found"))?;
        // Marshall the types from our internal representation to the actual DB representation.
        Ok(Artifact::from_model(artifact).unwrap())
    }

    /// Lists all available artifact
    pub async fn list(limit: u64, offset: u64) -> Result<Vec<Artifact>> {

        let db = DbPool::get().await;
        let artifact = artifact::Entity::find()
            .limit(limit)
            .offset(offset)
            .all(db)
            .await
            .unwrap();
        // Marshall the types from our internal representation to the actual DB representation.
        Ok(artifact.into_iter().map(|artifact| Artifact::from_model(artifact).unwrap()).collect())
    }

    /// Gets an artifact by the build it was associated with (with Build ID)
    pub async fn get_by_build_id(build_id: i32) -> Result<Vec<Artifact>> {
        let db = DbPool::get().await;
        let artifact = artifact::Entity::find()
            .filter(artifact::Column::BuildId.eq(build_id))
            .all(db)
            .await
            .unwrap();
        // Marshall the types from our internal representation to the actual DB representation.
        Ok(artifact.into_iter().map(|artifact| Artifact::from_model(artifact).unwrap()).collect())
    }

    /// Searches for an artifact
    pub async fn search(query: &str) -> Result<Vec<Artifact>> {
        let db = DbPool::get().await;
        let artifact = artifact::Entity::find()
            .filter(artifact::Column::Name.like(query))
            .all(db)
            .await
            .unwrap();
        // Marshall the types from our internal representation to the actual DB representation.
        Ok(artifact.into_iter().map(|artifact| Artifact::from_model(artifact).unwrap()).collect())
    }

}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Build {
    pub id: Uuid,
    pub worker: Uuid,
    pub status: i32,
    pub target_id: Uuid,
    pub timestamp: DateTime,
}

impl Build {
    /// Import from ORM model
    async fn from_model(model: build::Model) -> Result<Build> {
        Ok(Build {
            id: model.id,
            worker: model.worker,
            status: model.status,
            target_id: model.target_id,
            timestamp: model.timestamp,
        })
    }

    pub fn new(worker: Uuid, target_id: Uuid, compose_id: Uuid) -> Self {
        Self {
            id: Uuid::new_v4(),
            worker,
            status: 0,
            target_id,
            timestamp: Utc::now().naive_utc(),
        }
    }


    pub async fn add(&self) -> Result<Build> {
        let db = DbPool::get().await;
        let build = build::ActiveModel {
            id: ActiveValue::Set(self.id),
            worker: ActiveValue::Set(self.worker),
            status: ActiveValue::Set(self.status),
            target_id: ActiveValue::Set(self.target_id),
            timestamp: ActiveValue::Set(self.timestamp),
            ..Default::default()
        };
        let res = build::ActiveModel::insert(build, db).await?;
        Build::from_model(res).await
    }

    /// Gets a build by ID
    pub async fn get(id: Uuid) -> Result<Build> {
        let db = DbPool::get().await;
        let build = build::Entity::find_by_id(id)
            .one(db)
            .await?
            .ok_or(anyhow!("Build not found"))?;
        Ok(Build::from_model(build).await.unwrap())
    }

    pub async fn list(limit: u64, offset: u64) -> Result<Vec<Build>> {
        let db = DbPool::get().await;
        let build = build::Entity::find()
            .order_by(build::Column::Timestamp, Order::Desc)
            .limit(limit)
            .offset(offset)
            .all(db)
            .await?;

        Ok(
            future::try_join_all(
                build.into_iter().map(|build| {
                    Build::from_model(build)
                })
            ).await.unwrap()
        )
    }
    pub async fn get_by_target_id(target_id: Uuid) -> Result<Vec<Build>> {
        let db = DbPool::get().await;
        let build = build::Entity::find()
            .order_by(build::Column::Timestamp, Order::Desc)
            .filter(build::Column::TargetId.eq(target_id))
            .all(db)
            .await?;
        Ok(
            future::try_join_all(
                build.into_iter().map(|build| {
                    Build::from_model(build)
                })
            ).await.unwrap()
        )
    }

}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Project {
    pub id: Uuid,
    pub name: String,
    pub description: String,
}

impl Project {

    fn new(name: &str, description: Option<&str>) -> Project {
        // The resulting project is not ready for use, and does not have an ID.
        Project {
            id: Uuid::new_v4(),
            name: name.to_string(),
            description: description.unwrap_or("").to_string(),
        }
    }

    async fn add(&self) -> Result<Project> {
        let db = DbPool::get().await;
        let project = project::ActiveModel {
            id: ActiveValue::Set(self.id),
            name: ActiveValue::Set(self.name.clone()),
            description: ActiveValue::Set(self.description.clone()),
            ..Default::default()
        };
        let res = project::ActiveModel::insert(project, db).await?;
        Project::from_model(res).await
    }

    /// Import from ORM model
    async fn from_model(model: project::Model) -> Result<Project> {
        Ok(Project {
            id: model.id,
            name: model.name,
            description: model.description,
        })
    }

    /// Gets a project by ID
    pub async fn get(id: Uuid) -> Result<Project> {
        let db = DbPool::get().await;
        let project = project::Entity::find_by_id(id)
            .one(db)
            .await?
            .ok_or(anyhow!("Project not found"))?;
        Ok(Project::from_model(project).await.unwrap())
    }

    pub async fn list(limit: u64, offset: u64) -> Result<Vec<Project>> {
        let db = DbPool::get().await;
        let project = project::Entity::find()
            .limit(limit)
            .offset(offset)
            .all(db)
            .await?;
        Ok(
            future::try_join_all(
                project.into_iter().map(|project| {
                    Project::from_model(project)
                })
            ).await.unwrap()
        )
    }

}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Compose {
    pub id: Uuid,
    pub r#ref: Option<String>,
    pub project_id: Uuid,
    pub timestamp: DateTime,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Target {
    pub id: Uuid,
    pub name: String,
    pub image: Option<String>,
    pub arch: i32,
}