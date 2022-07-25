#[macro_use]
extern crate rocket;
use std::path::PathBuf;

#[macro_use]
use rocket::fs::FileServer;
use rocket::{Build, Rocket};
use sea_orm_rocket::Database;
use serde::Deserialize;

mod api;
mod auth;
mod backend;
mod db;
mod db_object;
mod entity;
mod kubernetes;
mod pkgs;
mod repos;
mod s3_object;

#[get("/<_file..>")]
async fn index(_file: PathBuf) -> std::io::Result<rocket::fs::NamedFile> {
    rocket::fs::NamedFile::open(std::path::Path::new("dist").join("index.html")).await
}

#[launch]
async fn rocket() -> Rocket<Build> {
    rocket::build()
        .attach(db::Db::init())
        .mount("/builds", api::builds_routes())
        .mount("/artifacts", api::artifacts_routes())
        .mount("/projects", api::projects_routes())
        .mount("/app", routes![index])
        //.mount("/assets", FileServer::from("dist/assets"))
        .mount("/", FileServer::from("dist"))
}
