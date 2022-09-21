//! Andaman client error handler
use anda_types::error::*;
use std::fmt::{Display, Formatter};
// derive macro that implements the From<anyhow::Error> trait

// pub enum ProjectError {
//     NoManifest,
//     InvalidManifest(String),
//     Multiple(Vec<Self>),
//     HclError(hcl::error::Error),
//     Other(String),
// }

// impl From<hcl::error::Error> for ProjectError {
//     fn from(e: hcl::error::Error) -> Self {
//         ProjectError::HclError(e)
//     }
// }

// impl From<anyhow::Error> for ProjectError {
//     fn from(err: anyhow::Error) -> Self {
//         ProjectError::Other(err.to_string())
//     }
// }

// impl std::fmt::Display for ProjectError {
//     fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
//         match self {
//             ProjectError::NoManifest => write!(f, "No manifest found"),
//             ProjectError::InvalidManifest(e) => write!(f, "Invalid manifest: {}", e),
//             ProjectError::Other(msg) => write!(f, "{}", msg),
//             ProjectError::HclError(e) => write!(
//                 f,
//                 "Error parsing HCL: {}{}",
//                 e.to_owned(),
//                 e.to_owned()
//                     .location()
//                     .map(|l| format!(" at {}:{}", l.line, l.col))
//                     .unwrap_or_default()
//             ),
//             ProjectError::Multiple(errors) => {
//                 write!(f, "Multiple errors:")?;
//                 for error in errors {
//                     write!(f, "\n - {}", error)?;
//                 }
//                 Ok(())
//             }
//         }
//     }
// }

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
        match self {
            BuilderError::Project(e) => write!(f, "Project: {}", e),
            BuilderError::Command(e) => write!(f, "Command: {}", e),
            BuilderError::Io(e) => write!(f, "IO: {}", e),
            BuilderError::Other(e) => write!(f, "Other: {}", e),
            BuilderError::Script(e) => write!(f, "Script: {}", e),
        }
    }
}

pub enum PackerError {
    //Project(ProjectError),
    Build(BuilderError),
    Path(String),
    Io(std::io::Error),
    //Other(String),
    Git(git2::Error),
    Other(String),
}

impl Display for PackerError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            PackerError::Build(e) => write!(f, "Build: {}", e),
            PackerError::Io(e) => write!(f, "IO: {}", e),
            PackerError::Path(e) => write!(f, "Path: {}", e),
            PackerError::Git(e) => write!(f, "Git: {}", e),
            PackerError::Other(e) => write!(f, "{}", e),
        }
    }
}

// impl std::error::Error for PackerError {}

impl From<std::io::Error> for PackerError {
    fn from(err: std::io::Error) -> Self {
        PackerError::Io(err)
    }
}

impl From<anyhow::Error> for PackerError {
    fn from(err: anyhow::Error) -> Self {
        PackerError::Other(err.to_string())
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
