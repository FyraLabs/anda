use serde::{Deserialize, Serialize};
use serde_yaml::Value;
use std::str;
use tokio::fs::{read, read_dir, ReadDir, write};
use serde_xml_rs::from_str;
use reqwest;
mod parsing;

pub async fn repo_exists(name: &str) -> bool {
    let mut entries: ReadDir = read_dir("./anda-pkgs/").await.unwrap();

    while let Some(entry) = entries.next_entry().await.unwrap() {
        if entry.path().to_str().unwrap() == name {
            return true;
        }
    }
    return false;
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Repo {
    pub name: String,
    pub url:  String,
    pub meta: String,
    pub kind: String,
}

impl Repo {
    pub fn new(name: String, url: String, meta: String, kind: String) -> Repo {
        Repo {
            name,
            url,
            meta,
            kind,
        }
    }
    pub async fn load_from_yaml(path: &str) -> Repo {
        let file: std::vec::Vec<u8> = read(path).await.unwrap();
        let val: Value = serde_yaml::from_str(str::from_utf8(&file).unwrap()).unwrap();
        //serde_yaml::from_value(val).unwrap()
        serde_yaml::from_value(val).unwrap()
    }

    pub async fn list_repos() -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        let mut entries: ReadDir = read_dir("./anda-pkgs/").await.unwrap();

        while let Some(entry) = entries.next_entry().await.unwrap() {
            let path = entry.path().display().to_string();
            if path.ends_with(".yml") {
                out.push(path.trim_end_matches(".yml").to_string());
            }
        }
        return out;
    }
    pub async fn get_repos() -> Vec<Repo> {
        let reponames = Repo::list_repos().await;
        let mut repos: Vec<Repo> = Vec::new();
        for name in reponames {
            repos.push(Repo::load_from_yaml(format!("{}.yml", name).as_str()).await);
        }
        repos
    }

    pub async fn load_meta(mut self) {
        let resp = reqwest::get(self.meta)
            .await
            .unwrap()
            .text()
            .await
            .unwrap();
        let metalink: parsing::Metalink = from_str(&resp).unwrap();
        let file = &metalink.files.files[0];
        assert_eq!(file.name, "repomd.xml");
        write("./anda-pkgs/".to_string() + &self.name + "/repomd.xml", resp.as_bytes()).await.unwrap();
/*         let empty = String::new();
        let mut best_url: &String = &empty;
        let mut best_preference = 0;
        for url in &file.resources.urls {
            if url.protocol == parsing::Protocol::https && url.preference > best_preference {
                best_url = &url.location;
                best_preference = url.preference;
            }
        } */

        let (mut best_url, mut best_preference) = (String::new(), 0);
        for url in &file.resources.urls {
            if url.protocol == parsing::Protocol::https && url.preference > best_preference {
                best_url = url.location.clone();
                best_preference = url.preference;
            }
        }

        //WARN assume best_url is not ""
        self.url = best_url.clone().to_string();
    }
    pub async fn update_pkgs() {
        todo!();
    }
    pub async fn list_pkgs(&self) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        let mut entries: ReadDir = read_dir(format!("./anda-pkgs/{}/", self.name))
            .await
            .unwrap();

        while let Some(entry) = entries.next_entry().await.unwrap() {
            out.push(String::from(entry.path().display().to_string()));
        }
        out
    }
    pub async fn get_pkg(&self, name: &str) -> Package {
        let path: String = format!("./anda-pkgs/{}/{}/anda.yml", self.name, name);
        let pkg: Package = Package::load_from_yaml(path.as_str()).await;
        pkg
    }
}
#[derive(Debug, Serialize, Deserialize)]
pub struct Package {
    pub name: String,
    pub version: String,
    pub description: String,
    pub license: String,
    pub build_scripts: Option<Vec<String>>,
}

impl Package {
    pub fn new(name: String, version: String, description: String, license: String) -> Package {
        Package {
            name,
            version,
            description,
            license,
            build_scripts: None,
        }
    }
    pub fn set_build_scripts(&mut self, build_scripts: Vec<String>) {
        self.build_scripts = Some(build_scripts);
    }
    // Load a package from a yaml file
    pub async fn load_from_yaml(path: &str) -> Package {
        let file: std::vec::Vec<u8> = read(path).await.unwrap();
        let val: Value = serde_yaml::from_str(str::from_utf8(&file).unwrap()).unwrap();
        serde_yaml::from_value(val).unwrap()
    }
}
