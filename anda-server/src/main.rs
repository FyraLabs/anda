#[macro_use]
extern crate rocket;
use rocket::{serde::{json::Json, Deserialize}, fs::FileServer, fs::{relative, Options}};

mod pkgs;
mod builds;
mod repos;
mod auth;
mod db;
use db::Db;
use sea_orm_rocket::Database;
mod entity;

#[get("/")]
fn index() -> &'static str {
    "Hello, world!"
}

#[derive(Deserialize)]
#[serde(crate = "rocket::serde")]
struct PkgReq {
    pkgs: Vec<String>,
}

#[get("/<repo>", data = "<pkgreq>")]
async fn process_pkgs(repo: &str, pkgreq: Json<PkgReq>) -> &'static str {
    if pkgs::repo_exists(repo).await {
        let mut reponame = String::from(repo);
        reponame.push_str(".yml");
        let repo = pkgs::Repo::load_from_yaml(reponame.as_str()).await;
        let size: i16 = 0; // size in MiB
        let paths: Vec<String> = Vec::new();
        let mut packages: Vec<pkgs::Package> = Vec::new();
        for pkg in &pkgreq.pkgs {
            packages.push(repo.get_pkg(pkg.as_str()).await);
        }
    }
    "hai"
}

#[get("/<repo>/<pkg>")]
async fn process_pkgs_browser(repo: &str, pkg: &str) -> &'static str {
    "you are accessing this from your browser I guess"
}

#[launch]
fn rocket() -> _ {
    rocket::build()
        .attach(Db::init())
        .mount("/", routes![index])
        .mount("/repos", routes![process_pkgs])
        .mount("/repos", routes![process_pkgs_browser])
}
