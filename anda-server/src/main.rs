#[macro_use]
extern crate rocket;
#[macro_use]
use rocket::fs::FileServer;
use sea_orm_rocket::Database;
use serde::Deserialize;

mod api;
mod artifacts;
mod auth;
mod backend;
mod db;
mod db_object;
mod entity;
mod kubernetes;
mod pkgs;
mod repos;

#[launch]
async fn rocket() -> _ {
    rocket::build()
        .attach(db::Db::init())
        .mount("/builds", api::builds_routes())
        .mount("/artifacts", api::artifacts_routes())
}
