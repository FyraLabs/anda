use schemars::JsonSchema;
use uuid::Uuid;

use chrono::{offset::Utc, DateTime};
use num_derive::FromPrimitive;
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Copy, PartialEq, Eq, FromPrimitive, Serialize, Deserialize, JsonSchema)]
pub enum BuildStatus {
    Pending = 0,
    Running = 1,
    Success = 2,
    Failure = 3,
}

impl<S: Into<String>> From<S> for BuildStatus {
    fn from(s: S) -> Self {
        match s.into().to_lowercase().as_str() {
            "pending" => BuildStatus::Pending,
            "running" => BuildStatus::Running,
            "success" => BuildStatus::Success,
            "failure" => BuildStatus::Failure,
            _ => BuildStatus::Pending,
        }
    }
}

impl std::fmt::Display for BuildStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BuildStatus::Pending => write!(f, "Pending"),
            BuildStatus::Running => write!(f, "Running"),
            BuildStatus::Success => write!(f, "Success"),
            BuildStatus::Failure => write!(f, "Failure"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct FileArtifact {
    pub e_tag: Option<String>,
    pub filename: Option<String>,
    pub size: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct RpmArtifact {
    pub name: String,
    pub arch: String,
    pub epoch: Option<String>,
    pub version: String,
    pub release: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct DockerArtifact {
    pub name: String,
    pub tag: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ArtifactMeta {
    pub art_type: String,
    pub file: Option<FileArtifact>,
    pub rpm: Option<RpmArtifact>,
    pub docker: Option<DockerArtifact>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Artifact {
    pub id: Uuid,
    pub filename: String,
    pub path: String,
    pub url: String,
    pub build_id: Uuid,
    pub timestamp: DateTime<Utc>,
    pub metadata: Option<ArtifactMeta>,
}

impl Artifact {
    pub fn new(filename: String, path: String, build_id: Uuid) -> Self {
        Self {
            id: Uuid::new_v4(),
            filename,
            path,
            build_id,
            timestamp: chrono::Utc::now(),
            url: String::new(),
            metadata: None,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct Build {
    pub id: Uuid,
    pub status: BuildStatus,
    pub target_id: Option<Uuid>,
    pub project_id: Option<Uuid>,
    pub timestamp: DateTime<Utc>,
    pub compose_id: Option<Uuid>,
    pub build_type: String,
    #[serde(skip_serializing)]
    pub logs: Option<String>,
    pub metadata: Option<BuildMeta>,
}

impl Build {
    pub fn new(
        target_id: Option<Uuid>,
        project_id: Option<Uuid>,
        compose_id: Option<Uuid>,
        build_type: String,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            target_id,
            project_id,
            compose_id,
            status: BuildStatus::Pending,
            timestamp: Utc::now(),
            build_type,
            logs: None,
            metadata: None,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct BuildMeta {
    pub scope: Option<String>,
    pub source: Option<String>,
    pub config_meta: Option<crate::AndaConfig>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct Project {
    pub id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub summary: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
pub struct Compose {
    pub id: Uuid,
    pub compose_ref: Option<String>,
    pub target_id: Uuid,
    pub timestamp: DateTime<Utc>,
}

impl Compose {
    pub fn new(target_id: Uuid) -> Self {
        Self {
            id: Uuid::new_v4(),
            compose_ref: None,
            target_id,
            timestamp: chrono::Utc::now(),
        }
    }
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
}
