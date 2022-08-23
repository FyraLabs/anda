use crate::backend::Compose;

use rocket::serde::json::Json;
use rocket::serde::uuid::Uuid;
use rocket::Route;

pub(crate) fn routes() -> Vec<Route> {
    routes![index, get,]
}

#[get("/?<limit>&<page>&<all>")]
async fn index(page: Option<usize>, limit: Option<usize>, all: Option<bool>) -> Json<Vec<Compose>> {
    let composes = if all.unwrap_or(false) {
        Compose::list_all().await
    } else {
        Compose::list(limit.unwrap_or(100), page.unwrap_or(0)).await
    };
    Json(composes.unwrap())
}

#[get("/<id>")]
async fn get(id: Uuid) -> Option<Json<Compose>> {
    Compose::get(id).await.map(Json).ok()
}
