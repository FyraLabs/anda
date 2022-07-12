use rocket::{Route};
use rocket::{serde::{json::Json, Deserialize}, fs::FileServer, fs::{relative, Options}, State};
use sea_orm::DatabaseConnection;
use crate::prelude::*;
use rocket::serde::uuid::Uuid;

pub(crate) fn routes() -> Vec<Route> {
    routes![
        index,
        get,
        get_by_compose,
        get_by_target,
    ]
}

#[get("/?<limit>&<offset>")]
async fn index(offset: Option<u64>,limit: Option<u64>) -> Json<Vec<Build>> {
    let builds = Build::list(limit.unwrap_or(100),offset.unwrap_or(0)).await;
    Json(builds.unwrap())
}

#[get("/<id>")]
async fn get(id: Uuid) -> Option<Json<Build>> {
    match Build::get(id).await {
        Ok(build) => Some(Json(build)),
        Err(_) => None,
    }
}

#[get("/by_compose/<project_id>")]
async fn get_by_compose(project_id: Uuid) -> Option<Json<Vec<Build>>> {
    match Build::get_by_compose_id(project_id).await {
        Ok(builds) => Some(Json(builds)),
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