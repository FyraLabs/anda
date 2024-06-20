//! Forge URI types
//!
//! This module contains custom URI types for Git forges, such as GitHub, GitLab, and Pagure.
//!
//! These special forge URIs can be converted to normal Git URIs, and can be used in the same way.
//! They may also implement a special forge-specific API for retrieving information about the repository in
//! the future.

// todo: refactor this further into submodules for each forge when more forges are added or forge APIs are implemented

use super::GitUri;
use super::UriSchemeTrait;
use crate::GitUriType;
use uri_parser::parse_uri;

/// A Git forge URI that can be converted to a Git URI
///
/// Any forge-specific URI should implement this trait,
/// and it should be able to convert itself to a Git URI, and somehow
/// also implement a UriSchemeTrait?
pub trait GitForgeUri {
    fn to_git_uri(&self) -> GitUri;
    fn from_string(uri: &str) -> Result<Self, String>
    where
        Self: Sized;
}

// Blanket implementation for all types that implement GitForgeUri
impl<T: GitForgeUri> UriSchemeTrait for T {
    fn to_string_uri(&self) -> String {
        self.to_git_uri().to_string_uri()
    }
    fn from_string(uri: &str) -> Result<Self, String> {
        T::from_string(uri)
    }
}

/// GitHub forge URI
/// - github:user/repo
pub struct GitHubUri {
    pub path: String,
}

impl GitHubUri {
    pub fn from_string(uri: &str) -> Result<GitHubUri, String> {
        let uri = parse_uri(uri).map_err(|e| e.to_string())?;
        if uri.scheme != "github" {
            return Err("Invalid GitHub URI".to_string());
        }
        // unwrap or error out
        match uri.path {
            Some(path) => {
                Ok(GitHubUri { path: path.to_str().expect("Proper parsable path?").to_string() })
            }
            None => Err("Invalid GitHub URI".to_string()),
        }
    }
}

impl GitForgeUri for GitHubUri {
    // We are going to just create a HTTPS URI out of thin air for GitHub
    fn to_git_uri(&self) -> GitUri {
        GitUri { url: format!("git+https://github.com/{}", self.path), uri_type: GitUriType::Http }
    }

    fn from_string(uri: &str) -> Result<GitHubUri, String> {
        GitHubUri::from_string(uri)
    }
}

/// Pagure forge URI, same as GitHub but uses pagure.io
/// - pagure:user/repo
pub struct PagureUri {
    pub path: String,
}

impl PagureUri {
    pub fn from_string(uri: &str) -> Result<PagureUri, String> {
        let uri = parse_uri(uri).map_err(|e| e.to_string())?;
        if uri.scheme != "pagure" {
            return Err("Invalid Pagure URI".to_string());
        }
        // unwrap or error out
        match uri.path {
            Some(path) => {
                Ok(PagureUri { path: path.to_str().expect("Proper parsable path?").to_string() })
            }
            None => Err("Invalid Pagure URI".to_string()),
        }
    }
}

impl GitForgeUri for PagureUri {
    // We are going to just create a HTTPS URI out of thin air for Pagure
    fn to_git_uri(&self) -> GitUri {
        GitUri { url: format!("git+https://pagure.io/{}", self.path), uri_type: GitUriType::Http }
    }

    fn from_string(uri: &str) -> Result<PagureUri, String> {
        PagureUri::from_string(uri)
    }
}

/// GitLab forge URI, same as GitHub but uses gitlab.com
/// - gitlab:user/repo
pub struct GitLabUri {
    pub path: String,
}

impl GitLabUri {
    pub fn from_string(uri: &str) -> Result<GitLabUri, String> {
        let uri = parse_uri(uri).map_err(|e| e.to_string())?;
        if uri.scheme != "gitlab" {
            return Err("Invalid GitLab URI".to_string());
        }
        // unwrap or error out
        match uri.path {
            Some(path) => {
                Ok(GitLabUri { path: path.to_str().expect("Proper parsable path?").to_string() })
            }
            None => Err("Invalid GitLab URI".to_string()),
        }
    }
}

impl GitForgeUri for GitLabUri {
    // We are going to just create a HTTPS URI out of thin air for GitLab
    fn to_git_uri(&self) -> GitUri {
        GitUri { url: format!("git+https://gitlab.com/{}", self.path), uri_type: GitUriType::Http }
    }

    fn from_string(uri: &str) -> Result<GitLabUri, String> {
        GitLabUri::from_string(uri)
    }
}
