//! Andaman client error handler

use proc_macro2::TokenStream;
use std::error;
use std::fmt::{Display, Formatter};
// derive macro that implements the From<anyhow::Error> trait

#[derive(Debug)]
pub enum ProjectError {
    NoManifest,
    InvalidManifest,
    Other(String),
}

impl From<toml::de::Error> for ProjectError {
    fn from(_: toml::de::Error) -> Self {
        ProjectError::InvalidManifest
    }
}

impl From<anyhow::Error> for ProjectError {
    fn from(err: anyhow::Error) -> Self {
        ProjectError::Other(err.to_string())
    }
}

impl std::fmt::Display for ProjectError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProjectError::NoManifest => write!(f, "No manifest found"),
            ProjectError::InvalidManifest => write!(f, "Invalid manifest"),
            ProjectError::Other(msg) => write!(f, "{}", msg),
        }
    }
}

#[derive(Debug)]
pub enum BuilderError {
    Project(ProjectError),
    Command(String),
    Io(std::io::Error),
    Other(String),
}

impl From<anyhow::Error> for BuilderError {
    fn from(err: anyhow::Error) -> Self {
        BuilderError::Other(err.to_string())
    }
}

impl From<ProjectError> for BuilderError {
    fn from(err: ProjectError) -> Self {
        BuilderError::Project(err)
    }
}

impl std::fmt::Display for BuilderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

#[derive(Debug)]
pub enum PackerError {
    Project(ProjectError),
    Build(BuilderError),
    Path(String),
    Io(std::io::Error),
    Other(String),
    Git(git2::Error),
}

impl Display for PackerError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl std::error::Error for PackerError {}

impl From<std::io::Error> for PackerError {
    fn from(err: std::io::Error) -> Self {
        PackerError::Io(err)
    }
}

impl From<std::io::Error> for BuilderError {
    fn from(err: std::io::Error) -> Self {
        BuilderError::Io(err)
    }
}

impl From<BuilderError> for PackerError {
    fn from(err: BuilderError) -> Self {
        PackerError::Build(err)
    }
}
