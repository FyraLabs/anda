#[macro_use]
extern crate rocket;
#[macro_use]
extern crate rocket_dyn_templates;
use rocket_dyn_templates::Template;
use sea_orm_rocket::Database;
use rocket::fs::FileServer;
use rocket::response::{status};
use rocket::http::ContentType;
mod api;
mod artifacts;
mod auth;
mod db;
mod entity;
mod pkgs;
mod prelude;
mod repos;

#[get("/")]
fn index() -> Template {
    Template::render(
        "index",
        context! {  
            foo: 123,
        },
    )
}

#[get("/favicon.png")]
fn favicon() -> (ContentType, &'static Vec<u8>) {
    (ContentType::PNG, include_bytes!("favicon.png"))
}


#[launch]
async fn rocket() -> _ {
    match db::setup_db().await {
        Ok(db) => db,
        Err(e) => panic!("{}", e)
    };

    rocket::build()
        .attach(db::Db::init())
        .mount("/", routes![index])
        .mount("/static", FileServer::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../static")))
        .mount("/assets", FileServer::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../assets")))
        .attach(Template::fairing())
        .mount("/builds", api::builds_routes())
        .mount("/artifacts", api::artifacts_routes())
}
