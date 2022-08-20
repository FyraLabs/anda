use crate::backend::Compose;

use rocket::http::Status;
use rocket::serde::json::Json;
use rocket::serde::uuid::Uuid;
use rocket::Route;
use serde::{Deserialize, Serialize};

pub(crate) fn routes() -> Vec<Route> {
    routes![index, get,]
}

#[get("/?<limit>&<page>")]
async fn index(page: Option<usize>, limit: Option<usize>) -> Json<Vec<Compose>> {
    let composes = Compose::list(limit.unwrap_or(100), page.unwrap_or(0)).await;
    Json(composes.unwrap())
}

#[get("/<id>")]
async fn get(id: Uuid) -> Option<Json<Compose>> {
    Compose::get(id).await.map(Json).ok()
}
