#[macro_use]
extern crate rocket;
#[macro_use]
extern crate rocket_dyn_templates;
use rocket::fs::FileServer;
use rocket_dyn_templates::Template;
use sea_orm_rocket::Database;
use serde::Deserialize;
mod api;
mod artifacts;
mod auth;
mod backend;
mod db;
mod db_object;
mod entity;
mod pkgs;
mod repos;

// #[derive(Serialize, Deserialize)]
// struct BuildDisplay {
//     id: String,
//     proj: String,
//     target: String,
//     comp: String,
//     status: i32
// }

#[get("/")]
async fn index() -> Template {
    let builds = db_object::Build::list(10, 0).await.unwrap_or(vec![]);
    let artifacts = db_object::Artifact::list(10, 0).await.unwrap_or(vec![]);
    let projects = db_object::Project::list(10, 0).await.unwrap_or(vec![]);
    // for build in &builds {
    //     let id = build.id.simple().to_string();
    // }
/*     let builds = builds.iter().map(|build| {
        BuildDisplay {
            id: build.id.simple().to_string(),
            proj: build.project_id.map_or("".to_string(), |uuid| uuid.simple().to_string()),
            target: build.target_id.map_or("".to_string(), |target| target.simple().to_string()),
            comp: build.compose_id.map_or("".to_string(), |compose| compose.simple().to_string()),
            status: build.status
        }
    }); */
    Template::render(
        "index",
        context! {
            builds,
            artifacts,
            projects
        },
    )
}

#[launch]
async fn rocket() -> _ {
    match db::setup_db().await {
        Ok(db) => db,
        Err(e) => panic!("{}", e),
    };

    rocket::build()
        .attach(db::Db::init())
        .mount("/", routes![index])
        .mount(
            "/static",
            FileServer::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../static")),
        )
        .mount(
            "/assets",
            FileServer::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../assets")),
        )
        .attach(Template::fairing())
        .mount("/builds", api::builds_routes())
        .mount("/artifacts", api::artifacts_routes())
}
