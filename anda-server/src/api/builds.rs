use crate::prelude::*;
use rocket::form::{Form};
use rocket::serde::uuid::Uuid;
use rocket::Route;
use rocket::{
    serde::{json::Json},
};


pub(crate) fn routes() -> Vec<Route> {
    routes![index, get, get_by_target, submit, update_status, tag_compose]
}

#[get("/?<limit>&<page>")]
async fn index(page: Option<usize>, limit: Option<usize>) -> Json<Vec<Build>> {
    let builds = Build::list(limit.unwrap_or(100), page.unwrap_or(0)).await;
    Json(builds.unwrap())
}

#[get("/<id>")]
async fn get(id: Uuid) -> Option<Json<Build>> {
    match Build::get(id).await {
        Ok(build) => Some(Json(build)),
        Err(_) => None,
    }
}

#[get("/by_target/<target_id>")]
async fn get_by_target(target_id: Uuid) -> Option<Json<Vec<Build>>> {
    match Build::get_by_target_id(target_id).await {
        Ok(builds) => Some(Json(builds)),
        Err(_) => None,
    }
}

#[derive(FromForm)]
pub struct BuildSubmission {
    worker: Uuid,
    target_id: Uuid,
    project_id: Option<Uuid>,
}

#[post("/", data = "<data>")]
async fn submit(data: Form<BuildSubmission>) -> Json<Build> {
    let build = Build::new(data.worker, 0, data.target_id, data.project_id)
        .add()
        .await;
    Json(build.unwrap())
}

#[derive(FromForm)]
struct BuildUpdateStatus {
    id: Uuid,
    status: i32,
}

#[post("/update_status", data = "<data>")]
async fn update_status(data: Form<BuildUpdateStatus>) -> Json<Build> {
    let build = Build::get(data.id)
        .await
        .expect("Failed to update build status")
        .update_status(data.status)
        .await;
    Json(build.unwrap())
}


// TODO: Tag target?

#[derive(FromForm)]
struct BuildTagCompose {
    id: Uuid,
    tag: Uuid,
}

#[post("/tag_compose", data = "<data>")]
async fn tag_compose(data: Form<BuildTagCompose>) -> Json<Build> {
    let build = Build::get(data.id)
        .await
        .expect("Failed to update build status")
        .tag_compose(data.tag)
        .await;
    Json(build.unwrap())
}