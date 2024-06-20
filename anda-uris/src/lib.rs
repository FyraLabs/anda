//! # Andaman URIs
//!
//! This crate provides a set of URI types that can be used to represent various types of URIs, such as file paths, Git URIs, and forge-specific URIs.
//!
//! The main goal of this crate is to provide URI schemes that provide an interface to refer to various types of resources, such as files, Git repositories
//! and even Git repositories hosted on various forges such as GitHub, GitLab, and Pagure.
//!
//! The URI types provided by this crate implement the [`UriSchemeTrait`] trait, which provides a uniform interface to convert URIs to and from strings.
//!
//! This crate is meant to be used as a way to represent sources of data as a URI string, and then get some kind of usable data from it.
//!
//!
//! ## Examples
//!
//! - `github:user/repo` - GitHub repository, where this converts into `git+https://github.com/user/repo.git`
//! - `file:///path/to/repo` - Local file path, where this converts into `file:///path/to/repo`
//!

// todo: This is kind of a mess, clean this up
use forge::GitForgeUri;
use std::convert::TryFrom;
use std::{
    fmt::{Display, Formatter},
    path::{Path, PathBuf},
};
use url::Url;
pub mod forge;
#[cfg(test)]
mod tests;

/// Parse a URI string into anything that implements UriSchemeTrait
pub fn anda_uri<U>(uri: &str) -> Result<U, String>
where
    U: UriSchemeTrait,
{
    let uri = U::from_string(uri)?;
    Ok(uri)
}

pub trait UriSchemeTrait {
    fn to_string_uri(&self) -> String;
    fn from_string(uri: &str) -> Result<Self, String>
    where
        Self: Sized;
}

/// Simple HTTP URL type
pub struct HttpUrl {
    pub url: Url,
}

impl TryFrom<&Url> for HttpUrl {
    type Error = String;

    fn try_from(url: &Url) -> Result<Self, Self::Error> {
        if url.scheme() != "http" && url.scheme() != "https" {
            return Err("Invalid HTTP URL".to_string());
        }
        Ok(HttpUrl { url: url.clone() })
    }
}

impl TryFrom<&str> for HttpUrl {
    type Error = String;

    fn try_from(url: &str) -> Result<Self, Self::Error> {
        // just use TryFrom<&Url> for this
        HttpUrl::try_from(&Url::parse(url).map_err(|e| e.to_string())?)
    }
}

impl UriSchemeTrait for HttpUrl {
    fn to_string_uri(&self) -> String {
        self.url.to_string()
    }

    fn from_string(uri: &str) -> Result<Self, String> {
        let url = Url::parse(uri).map_err(|e| e.to_string())?;
        if url.scheme() != "http" && url.scheme() != "https" {
            return Err("Invalid HTTP URI".to_string());
        }
        Ok(HttpUrl { url })
    }
}

pub struct PathUri {
    pub path: PathBuf,
}

impl TryFrom<&str> for PathUri {
    type Error = String;
    fn try_from(path: &str) -> Result<Self, Self::Error> {
        // if the URI is already in the file:///path/to/path format, return it as is
        if path.starts_with("file:///") {
            return Ok(PathUri { path: PathBuf::from(&path[7..]) });
        }

        // if can't parse uri then assume it's a path
        let uri = Url::parse(path).map_err(|_| "Invalid URI".to_string());

        match uri {
            Ok(uri) => {
                if uri.scheme() != "file" {
                    return Err("Invalid URI".to_string());
                }
                Ok(PathUri { path: PathBuf::from(uri.path()) })
            }
            Err(_) => Ok(PathUri { path: PathBuf::from(path) }),
        }
    }
}

impl UriSchemeTrait for PathUri {
    fn to_string_uri(&self) -> String {
        format!("file://{}", self.path.to_str().unwrap())
    }

    fn from_string(uri: &str) -> Result<Self, String> {
        PathUri::try_from(uri)
    }
}

impl From<&Path> for PathUri {
    fn from(path: &Path) -> Self {
        PathUri { path: path.to_path_buf() }
    }
}

pub enum GitUriType {
    Git,  // native Git protocol
    Http, // http(s) protocol
    Ssh,  // ssh protocol
}

/// Represents a git URI scheme, can be in many forms:
/// - git://example.com/repo.git
/// - git+ssh://example.com/repo.git
/// - git+http(s)://example.com/repo.git
/// - https://example.com/repo.git (treated as git+https)
///
/// There will be extra structs that supports conversion from itself to this GitUri
pub struct GitUri {
    pub url: String, // todo: convert this to a uniform git uri so that it can be used in git2
    pub uri_type: GitUriType,
}

impl UriSchemeTrait for GitUri {
    fn to_string_uri(&self) -> String {
        self.url.clone()
    }

    fn from_string(uri: &str) -> Result<Self, String> {
        GitUri::try_from(uri)
    }
}

impl TryFrom<&str> for GitUri {
    type Error = String;

    fn try_from(uri: &str) -> Result<Self, Self::Error> {
        let uri = Url::parse(uri).map_err(|e| e.to_string())?;
        // Let's check if:
        // the scheme is git+* or git://
        // or, scheme is http(s):// and path ends with .git
        // We can return the whole URI, git2 can handle most of this... I think

        // todo: maybe clean this up?
        // git+* URIs
        if uri.scheme().starts_with("git+") {
            // Check protocol, can now be either ssh or http(s)
            let uri_type = match uri.scheme().split('+').nth(1).unwrap() {
                "ssh" => GitUriType::Ssh,
                "http" | "https" => GitUriType::Http,
                _ => return Err("Invalid git URI".to_string()),
            };

            // I don't know if we can strip the .git suffix here...

            Ok(GitUri { url: uri.to_string(), uri_type })
        }
        // native git protocol
        else if uri.scheme() == "git" {
            return Ok(GitUri { url: uri.to_string(), uri_type: GitUriType::Git });
        }
        // .git URIs
        else if (uri.scheme() == "http" || uri.scheme() == "https" || uri.scheme() == "ssh")
            && uri.to_string().ends_with(".git")
        {
            return Ok(GitUri { url: uri.to_string(), uri_type: GitUriType::Http });
        } else {
            return Err("Invalid git URI".to_string());
        }
    }
}

impl GitUri {
    pub fn from_forge_uri<F: GitForgeUri>(uri: &F) -> GitUri {
        uri.to_git_uri()
    }
}
impl Display for GitUri {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        write!(f, "{}", self.url)
    }
}
