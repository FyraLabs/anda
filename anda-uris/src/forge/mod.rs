//! Forge URI types
//!
//! This module contains custom URI types for Git forges, such as GitHub, GitLab, and Pagure.
//!
//! These special forge URIs can be converted to normal Git URIs, and can be used in the same way.
//! They may also implement a special forge-specific API for retrieving information about the repository in
//! the future.
//!
//! A Git forge URI that can be converted to a Git URI
//!
//! Any forge-specific URI should implement this trait,
//! and it should be able to convert itself to a Git URI, and somehow
//! also implement a UriSchemeTrait?

// use all submodules in here
pub mod github;
pub mod gitlab;
pub mod pagure;

// Re-export all forge types

pub use github::GitHubUri;
pub use gitlab::GitLabUri;
pub use pagure::PagureUri;

use super::GitUri;
use super::UriSchemeTrait;

pub trait GitForgeUri {
    fn to_git_uri(&self) -> GitUri;
    fn from_string(uri: &str) -> Result<Self, String>
    where
        Self: Sized;
}

// Blanket implementation for all types that implement GitForgeUri
impl<T: GitForgeUri + Sized> UriSchemeTrait for T {
    fn to_string_uri(&self) -> String {
        self.to_git_uri().to_string_uri()
    }
    fn from_string(uri: &str) -> Result<Self, String> {
        T::from_string(uri)
    }
}
