use url::Url;

use crate::{GitUri, GitUriType};

use super::GitForgeUri;
use std::convert::TryFrom;

/// GitLab forge URI, same as GitHub but uses gitlab.com
/// - gitlab:user/repo
pub struct GitLabUri {
    pub path: String,
}

impl TryFrom<&str> for GitLabUri {
    type Error = String;

    fn try_from(uri: &str) -> Result<Self, Self::Error> {

        let uri = Url::parse(uri).map_err(|e| e.to_string())?;
        if uri.scheme() != "gitlab" {
            return Err("Invalid GitLab URI".to_string());
        }
        if !uri.path().is_empty() {
            let path = uri.path().trim_start_matches('/');
            Ok(GitLabUri { path: path.to_string() })
        } else {
            Err("Invalid GitLab URI".to_string())
        }
    }
}


impl GitForgeUri for GitLabUri {
    fn to_git_uri(&self) -> GitUri {
        GitUri { url: format!("git+https://gitlab.com/{}", self.path), uri_type: GitUriType::Http }
    }

    fn from_string(uri: &str) -> Result<GitLabUri, String> {
        GitLabUri::try_from(uri)
    }
}
