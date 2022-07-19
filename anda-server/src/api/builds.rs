use rocket::{Route, serde::{json::Json, uuid::Uuid}};
use crate::prelude::*;

pub(crate) fn routes() -> Vec<Route> {
    routes![
        index,
        get,
        get_by_target,
    ]
}

#[get("/?<limit>&<page>")]
async fn index(page: Option<usize>,limit: Option<usize>) -> Json<Vec<Build>> {
    let builds = Build::list(limit.unwrap_or(100),page.unwrap_or(0)).await;
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
