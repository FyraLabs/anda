use crate::backend::Project;
use rocket::form::Form;
use rocket::fs::TempFile;
use rocket::http::Status;
use rocket::serde::json::Json;
use rocket::serde::uuid::Uuid;
use rocket::Route;

pub(crate) fn routes() -> Vec<Route> {
    routes![
        index,
        get,
    ]
}

#[get("/?<limit>&<page>")]
async fn index(page: Option<usize>, limit: Option<usize>) -> Json<Vec<Project>> {
    let projects = Project::list(limit.unwrap_or(100), page.unwrap_or(0)).await;
    Json(projects.unwrap())
}

#[get("/<id>")]
async fn get(id: Uuid) -> Option<Json<Project>> {
    match Project::get(id).await {
        Ok(project) => Some(Json(project)),
        Err(_) => None,
    }
}