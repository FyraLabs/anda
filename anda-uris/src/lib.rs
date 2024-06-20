use std::{
    fmt::{Display, Formatter},
    path::{Path, PathBuf},
};
use uri_parser::parse_uri;

// Would return any object with a certain trait I guess?
/// Parse a URI string into anything that implements UriSchemeTrait
pub fn anda_uri<U>(uri: &str) -> Result<U, String>
where
    U: UriSchemeTrait,
{
    let uri = U::from_string(uri)?;
    Ok(uri)
}

#[test]
fn uri_trait_test() {
    let uri = anda_uri::<PathUri>("/home/user/file.txt").unwrap();
    // println!("{}", uri.to_string_uri());

    assert_eq!(uri.to_string_uri(), "file:///home/user/file.txt");

    // Test if it's already a URI
    let uri = anda_uri::<PathUri>("file:///home/user/file.txt").unwrap();
    // println!("{}", uri.to_string_uri());

    assert_eq!(uri.to_string_uri(), "file:///home/user/file.txt");

    let uri = anda_uri::<GitHubUri>("github:rust-lang/cargo").unwrap();
    println!("{}", uri.to_string_uri());
    assert_eq!(uri.to_string_uri(), "git+https://github.com/rust-lang/cargo");

    let uri = anda_uri::<PagureUri>("pagure:fedora/rust").unwrap();
    println!("{}", uri.to_string_uri());
    assert_eq!(uri.to_string_uri(), "git+https://pagure.io/fedora/rust");

    let uri = anda_uri::<GitLabUri>("gitlab:fedora/rust").unwrap();
    println!("{}", uri.to_string_uri());
    assert_eq!(uri.to_string_uri(), "git+https://gitlab.com/fedora/rust");

    
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

#[test]
fn github_test() {
    let uri = GitHubUri::from_string("github:rust-lang/cargo").unwrap();
    let git_uri = uri.to_git_uri();
    println!("{}", git_uri.url);
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
