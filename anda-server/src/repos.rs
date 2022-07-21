use crate::db_object::Build;

use anyhow::{anyhow, Result};
use serde_derive::{Deserialize, Serialize};
use serde_json::{json, Value};
#[derive(Clone, Debug)]
pub enum Repo {
    RPM {
        id: String,
        builds: Vec<Build>,
    },
    Image {
        id: String,
        builds: Vec<Build>,
    },
    OSTree {
        id: String,
        refs: Vec<String>,
        template: String,
    },
}

impl Repo {
    pub fn new_rpm(id: String, builds: Vec<Build>) -> Repo {
        Repo::RPM { id, builds }
    }
    pub fn new_image(id: String, builds: Vec<Build>) -> Repo {
        Repo::Image { id, builds }
    }
    pub fn new_ostree(id: String, refs: Vec<String>, template: String) -> Repo {
        Repo::OSTree { id, refs, template }
    }
    pub fn generate(&self) {}
}

/// Image repositories
/// These are repositories that contain bootable images,
/// such as ISO images, Disk images, and so on.
/// The images are stored in a directory named after the image type.
#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct ImageRepo {
    pub project: String,
    pub images: Vec<Image>,
}

impl ImageRepo {
    pub fn new(project: String, images: Vec<Image>) -> ImageRepo {
        ImageRepo { project, images }
    }
    pub fn from_json(json: &Value) -> Result<ImageRepo> {
        let repo: ImageRepo =
            serde_json::from_value(json.clone()).expect("Failed to deserialize JSON");
        Ok(repo)
    }
    pub fn from_file(path: &str) -> Result<ImageRepo> {
        let json = std::fs::read_to_string(path).expect("Failed to read file");
        let repo: ImageRepo = serde_json::from_str(&json).expect("Failed to deserialize JSON");
        Ok(repo)
    }
    pub fn from_dir(path: &str) -> Result<ImageRepo> {
        // find a file called repodata.json in the directory
        let repo_path = std::path::Path::new(path);
        let repo_file = repo_path.join("repodata.json");
        let repo_file = repo_file.to_str().expect("Failed to open repo data file");
        let repo = ImageRepo::from_file(repo_file)?;
        Ok(repo)
    }
}

/// Image metadata
/// This is the metadata for an image.
/// Works similarly to what the package metadata in an RPM repository is.
#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct Image {
    // relative path to file
    pub path: String,
    pub variant: String,
    pub arch: String,
    pub checksum: String,
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_imagerepo() {
        //print cwd
        println!("{}", std::env::current_dir().unwrap().display());
        let repo = ImageRepo::from_dir("test/imagerepo").expect("Failed to read repo data");
        assert_eq!(repo.project, "test-project");
    }
}
