//! # Andaman Project
//! Andaman is a package build/CI system written in Rust, powered by
//! Kubernetes, Rocket, and Postgres.
//!
//! It is designed to be simple and easy to set up, but powerful enough to
//! be used for full-scale production-ready deployments.
//!
//! Unlike other common CI systems like Travis or CircleCI,
//! Andaman is not designed to be used to
//! build and test standard code projects using custom scripts. Instead, it is designed
//! to be a package build system that can be used to build and test packages
//! for distribution to production repositories
//!
//! It is created to be a simple and robust alternative to package building systems
//! like Koji, Launchpad, and the Arch Linux build system.
//!
//! # Output
//! Andaman produces artifacts in the form of repositories, as a collection of
//! projects' artifacts collected to make a full repository called a "compose".
//!
//! A compose is a full repository (or repositories) of packages, OS images,
//! and other artifacts organized into a usable repository for package managers.
//!
//! For some cases, certain types of project cannot be included in a compose,
//! as they must be contained in a separate repository of their own
//! (e.g. Docker registry images, OSTree repositories). These will be hosted on a separate managed
//! repository.
//!
//! # Setup
//! To set up Andaman, you need:
//! * A Kubernetes cluster to execute jobs on. (You can combine all of the below to deploy Andaman entirely on your own Kubernetes cluster.)
//! * A Postgres database for storing build data
//! * A S3 bucket for storing artifacts
//!
//! Check out the example environment variables in the `.env.example` file,
//! and then set them up in your `.env` file.

#[macro_use]
extern crate rocket;
use std::{path::PathBuf, borrow::Cow, ffi::OsStr};
use log::info;
use rocket::{Build, Rocket, response::content::RawHtml, http::ContentType};
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

#[get("/callback/<_file..>")]
fn callback(_file: PathBuf) -> Option<RawHtml<Cow<'static, [u8]>>> {
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
    if let Ok(log_config) = std::env::var("ANDA_LOG") {
        std::env::set_var("RUST_LOG", log_config);
    }

    // if RUST_LOG is not set
    if std::env::var("RUST_LOG").is_err() {
        #[cfg(debug_assertions)]
        std::env::set_var("RUST_LOG", "debug,hyper=off");

        #[cfg(not(debug_assertions))]
        std::env::set_var("RUST_LOG", "info,rocket::server=error,_=error,sqlx=error,anda_server=info");
    }

    pretty_env_logger::init();
    info!("Andaman Project Server, version {}", env!("CARGO_PKG_VERSION"));
    info!("Starting up server...");
    rocket::build()
        .attach(db::Db::init())
        .mount("/builds", api::builds_routes())
        .mount("/artifacts", api::artifacts_routes())
        .mount("/projects", api::projects_routes())
        .mount("/app", routes![index])
        //.mount("/assets", FileServer::from("dist/assets"))
        .mount("/", routes![dist, root, callback])
}
