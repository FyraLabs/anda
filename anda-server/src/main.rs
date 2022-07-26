#[macro_use]
extern crate rocket;
use std::{path::PathBuf, borrow::Cow, ffi::OsStr};

use rocket::{Build, Rocket, fs::FileServer, response::content::RawHtml, http::ContentType};
use sea_orm_rocket::Database;

mod api;
mod auth;
mod backend;
mod db;
mod db_object;
mod entity;
mod kubernetes;
mod s3_object;

use rust_embed::RustEmbed;

#[derive(RustEmbed)]
#[folder = "dist/"]
struct Asset;

#[get("/<_file..>")]
fn index(_file: PathBuf) -> Option<RawHtml<Cow<'static, [u8]>>> {
  let asset = Asset::get("index.html")?;
  Some(RawHtml(Cow::from(asset)))
}

#[get("/")]
fn root() -> Option<RawHtml<Cow<'static, [u8]>>> {
  let asset = Asset::get("index.html")?;
  Some(RawHtml(Cow::from(asset)))
}

#[get("/<file..>" , rank = 10)]
fn dist(file: PathBuf) -> Option<(ContentType, Cow<'static, [u8]>)> {
    let filename = file.display().to_string();
    let asset = Asset::get(&filename)?;
    let content_type = file
      .extension()
      .and_then(OsStr::to_str)
      .and_then(ContentType::from_extension)
      .unwrap_or(ContentType::Bytes);

    Some((content_type, Cow::from(asset)))
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
        .mount("/", routes![dist, root])
}
