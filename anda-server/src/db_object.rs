//! Database helper structs and functions.
//! This module wraps over the ORM entites created by SeaORM.
//! This module is used by the server to interact with the database.
//! You should not need to use this module directly.

// This code is licensed under the MIT License.
// Copyright (c) 2022 the Ultramarine Project and Fyra Labs.

use crate::{
    db,
    entity::{artifact, build, project, target},
};
use anyhow::{anyhow, Result};
use chrono::{offset::Utc, DateTime};
use db::DbPool;
use sea_orm::{prelude::Uuid, *};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Artifact {
    pub id: Uuid,
    pub name: String,
    pub url: String,
    pub build_id: Uuid,
    pub timestamp: DateTime<Utc>,
}

impl From<artifact::Model> for Artifact {
    fn from(model: artifact::Model) -> Self {
        Artifact {
            build_id: model.build_id,
            id: model.id,
            name: model.name,
            timestamp: DateTime::from_utc(model.timestamp, Utc),
            url: model.url,
        }
    }
}

impl Artifact {
    pub fn new(art_id: Uuid, build_id: Uuid, name: String, url: String) -> Self {
        Self {
            id: art_id,
            build_id,
            name,
            timestamp: Utc::now(),
            url,
        }
    }

    pub async fn add(&self) -> Result<Artifact> {
        let db = DbPool::get().await;
        let model = artifact::ActiveModel {
            id: ActiveValue::Set(self.id),
            build_id: ActiveValue::Set(self.build_id),
            name: ActiveValue::Set(self.name.clone()),
            timestamp: ActiveValue::Set(self.timestamp.naive_utc()),
            url: ActiveValue::Set(self.url.clone()),
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
            .filter(artifact::Column::Url.like(&format!("%{}%", query)).or(
                artifact::Column::Name.like(&format!("%{}%", query)),
            ))
            // TODO: use ts_query to search for the query in the url and name fields.
            // or write up a good search algorithm.
            /* .from_raw_sql(Statement::from_sql_and_values(DbBackend::Postgres,
                r#"SELECT * FROM artifact where to_tsvector('name') @@ to_tsquery('$1') or to_tsvector('url') @@ to_tsquery('$1')"#,
                vec![query.into()],
                )
            ) */

            .all(db)
            .await
            .unwrap();
        // Marshall the types from our internal representation to the actual DB representation.
        artifact.into_iter().map(Artifact::from).collect()
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Build {
    pub id: Uuid,
    pub status: i32,
    pub target_id: Option<Uuid>,
    pub project_id: Option<Uuid>,
    pub timestamp: DateTime<Utc>,
    pub compose_id: Option<Uuid>,
    pub build_type: String,
}

impl From<crate::backend::Build> for Build {
    fn from(build: crate::backend::Build) -> Self {
        Self {
            id: build.id,
            status: build.status as i32,
            target_id: build.target_id,
            project_id: build.project_id,
            timestamp: build.timestamp,
            compose_id: build.compose_id,
            build_type: build.build_type,
        }
    }
}

impl From<build::Model> for Build {
    fn from(model: build::Model) -> Self {
        Build {
            id: model.id,
            status: model.status,
            target_id: model.target_id,
            project_id: model.project_id,
            timestamp: DateTime::from_utc(model.timestamp, Utc),
            compose_id: model.compose_id,
            build_type: model.build_type,
        }
    }
}

impl Build {
    pub fn new(status: i32, project_id: Option<Uuid>, build_type: &str) -> Self {
        Self {
            id: Uuid::new_v4(),
            status,
            target_id: None,
            project_id,
            timestamp: Utc::now(),
            compose_id: None,
            build_type: build_type.to_string(),
        }
    }

    pub async fn add(&self) -> Result<Build> {
        let db = DbPool::get().await;
        let build = build::ActiveModel {
            id: ActiveValue::Set(self.id),
            status: ActiveValue::Set(self.status),
            target_id: ActiveValue::Set(self.target_id),
            timestamp: ActiveValue::Set(self.timestamp.naive_utc()),
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
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Project {
    pub id: Uuid,
    pub name: String,
    pub description: String,
}

impl From<project::Model> for Project {
    fn from(model: project::Model) -> Self {
        Project {
            id: model.id,
            name: model.name,
            description: model.description,
        }
    }
}

impl Project {
    pub fn new<S: Into<String>>(id: Uuid, name: S, description: Option<S>) -> Project {
        // The resulting project is not ready for use, and does not have an ID.
        Project {
            id,
            name: name.into(),
            description: description.map(|s| s.into()).unwrap_or_default(),
        }
    }

    pub async fn add(&self) -> Result<Project> {
        let db = DbPool::get().await;
        let project = project::ActiveModel {
            id: ActiveValue::Set(self.id),
            name: ActiveValue::Set(self.name.clone()),
            description: ActiveValue::Set(self.description.clone()),
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
            description: ActiveValue::Set(description),
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
    pub r#ref: Option<String>,
    pub project_id: Uuid,
    pub timestamp: DateTime<Utc>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Target {
    pub id: Uuid,
    pub name: String,
    pub image: Option<String>,
    pub arch: String,
}

impl Target {
    pub fn new(name: String, image: Option<String>, arch: String) -> Self {
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
}
