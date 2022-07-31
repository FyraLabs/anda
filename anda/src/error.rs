//! Andaman client error handler
use std::fmt::{Display, Formatter};
// derive macro that implements the From<anyhow::Error> trait

pub enum ProjectError {
    NoManifest,
    InvalidManifest(String),
    HclError(hcl::error::Error),
    Other(String),
}

impl From<hcl::error::Error> for ProjectError {
    fn from(e: hcl::error::Error) -> Self {
        ProjectError::HclError(e)
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
            ProjectError::InvalidManifest(e) => write!(f, "Invalid manifest: {}", e),
            ProjectError::Other(msg) => write!(f, "{}", msg),
            ProjectError::HclError(e) => write!(f, "HCL: {:?}", e),
        }
    }
}

//impl std::error::Error for ProjectError {}

pub enum BuilderError {
    Project(ProjectError),
    Command(String),
    Io(std::io::Error),
    Other(String),
    Script(std::io::Error),
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
        write!(f, "Builder Error")
    }
}

pub enum PackerError {
    //Project(ProjectError),
    Build(BuilderError),
    Path(String),
    Io(std::io::Error),
    //Other(String),
    Git(git2::Error),
}

impl Display for PackerError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "Packer Error")
    }
}

// impl std::error::Error for PackerError {}

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
