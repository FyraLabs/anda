use url::Url;

use crate::{GitUri, GitUriType};

use super::GitForgeUri;
use std::convert::TryFrom;

/// GitHub forge URI
/// - github:user/repo
pub struct GitHubUri {
    pub path: String,
}

impl TryFrom<&str> for GitHubUri {
    type Error = String;

    fn try_from(uri: &str) -> Result<Self, Self::Error> {

        let uri = Url::parse(uri).map_err(|e| e.to_string())?;
        if uri.scheme() != "github" {
            return Err("Invalid GitHub URI".to_string());
        }

        if !uri.path().is_empty() {
            let path = uri.path().trim_start_matches('/');
            Ok(GitHubUri { path: path.to_string() })
        } else {
            Err("Invalid GitHub URI".to_string())
        }
    }
}

impl GitForgeUri for GitHubUri {
    // We are going to just create a HTTPS URI out of thin air for GitHub
    fn to_git_uri(&self) -> GitUri {
        GitUri { url: format!("git+https://github.com/{}", self.path), uri_type: GitUriType::Http }
    }

    fn from_string(uri: &str) -> Result<GitHubUri, String> {
        GitHubUri::try_from(uri)
    }
}
