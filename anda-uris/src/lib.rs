use forge::GitForgeUri;
use std::{
    fmt::{Display, Formatter},
    path::{Path, PathBuf},
};
use uri_parser::parse_uri;
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

pub struct PathUri {
    pub path: PathBuf,
}

impl PathUri {
    pub fn from_path(path: &Path) -> PathUri {
        PathUri { path: path.to_path_buf() }
    }

    pub fn from_string(path: &str) -> Result<PathUri, String> {
        // if the URI is already in the file:///path/to/path format, return it as is
        if path.starts_with("file:///") {
            return Ok(PathUri { path: PathBuf::from(&path[7..]) });
        }

        // if can't parse uri then assume it's a path
        let uri = parse_uri(path).map_err(|_| "Invalid URI".to_string());

        match uri {
            Ok(uri) => {
                println!("{:?}", uri);
                if uri.scheme != "file" {
                    return Err("Invalid URI".to_string());
                }
                match uri.path {
                    Some(path) => {
                        // strip the leading /'s

                        println!("path: {:?}", path);
                        Ok(PathUri { path: PathBuf::from(path) })
                    }
                    None => Err("Invalid URI".to_string()),
                }
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
        PathUri::from_string(uri)
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
        GitUri::from_string(uri)
    }
}

impl GitUri {
    /// Parse a string into a GitUri
    pub fn from_string(uri: &str) -> Result<GitUri, String> {
        let uri = parse_uri(uri).map_err(|e| e.to_string())?;
        // Let's check if:
        // the scheme is git+* or git://
        // or, scheme is http(s):// and path ends with .git
        // We can return the whole URI, git2 can handle most of this... I think

        // todo: maybe clean this up?
        // git+* URIs
        if uri.scheme.starts_with("git+") {
            // Check protocol, can now be either ssh or http(s)
            let uri_type = match uri.scheme.split('+').nth(1).unwrap() {
                "ssh" => GitUriType::Ssh,
                "http" | "https" => GitUriType::Http,
                _ => return Err("Invalid git URI".to_string()),
            };

            // I don't know if we can strip the .git suffix here...

            Ok(GitUri { url: uri.to_string(), uri_type })
        }
        // native git protocol
        else if uri.scheme == "git" {
            return Ok(GitUri { url: uri.to_string(), uri_type: GitUriType::Git });
        }
        // .git URIs
        else if (uri.scheme == "http" || uri.scheme == "https" || uri.scheme == "ssh")
            && uri.to_string().ends_with(".git")
        {
            return Ok(GitUri { url: uri.to_string(), uri_type: GitUriType::Http });
        } else {
            return Err("Invalid git URI".to_string());
        }
    }

    pub fn from_forge_uri<F: GitForgeUri>(uri: &F) -> GitUri {
        uri.to_git_uri()
    }
}

impl Display for GitUri {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        write!(f, "{}", self.url)
    }
}
