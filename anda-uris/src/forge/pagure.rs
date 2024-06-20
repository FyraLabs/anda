use super::GitForgeUri;
use crate::{GitUri, GitUriType};
use url::Url;

/// Pagure forge URI, same as GitHub but uses pagure.io
/// - pagure:user/repo
pub struct PagureUri {
    pub path: String,
}

impl TryFrom<&str> for PagureUri {
    type Error = String;

    fn try_from(uri: &str) -> Result<Self, Self::Error> {

        let uri = Url::parse(uri).map_err(|e| e.to_string())?;
        if uri.scheme() != "pagure" {
            return Err("Invalid Pagure URI".to_string());
        }

        if !uri.path().is_empty() {
            let path = uri.path().trim_start_matches('/');
            Ok(PagureUri { path: path.to_string() })
        } else {
            Err("Invalid Pagure URI".to_string())
        }
    }
}

impl GitForgeUri for PagureUri {
    fn to_git_uri(&self) -> GitUri {
        GitUri { url: format!("git+https://pagure.io/{}", self.path), uri_type: GitUriType::Http }
    }

    fn from_string(uri: &str) -> Result<PagureUri, String> {
        PagureUri::try_from(uri)
    }
}
