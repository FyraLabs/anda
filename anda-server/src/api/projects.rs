use crate::backend::{Project};
use crate::db_object::Artifact;
use rocket::form::Form;
use rocket::http::Status;
use rocket::serde::json::Json;
use rocket::serde::uuid::Uuid;
use rocket::Route;

pub(crate) fn routes() -> Vec<Route> {
    routes![index, get, new, get_artifacts]
}

#[get("/?<limit>&<page>")]
async fn index(page: Option<usize>, limit: Option<usize>) -> Json<Vec<Project>> {
    let projects = Project::list(limit.unwrap_or(100), page.unwrap_or(0)).await;
    Json(projects.unwrap())
}

#[get("/<id>")]
async fn get(id: Uuid) -> Option<Json<Project>> {
    Project::get(id).await.map(Json).ok()
}

#[derive(FromForm)]
struct ProjectNew {
    name: String,
    description: Option<String>,
}


#[get("/<id>/artifacts")]
async fn get_artifacts(id: Uuid) -> Option<Json<Vec<Artifact>>> {
    let project = Project::get(id).await.ok()?;
    let a = project.list_artifacts().await.ok()?.iter().map(|a| Artifact::from(a.clone())).collect();
    Some(Json(a))
}

#[post("/", data = "<data>")]
async fn new(data: Form<ProjectNew>) -> Result<(), Status> {
    let project = Project::new(data.name.clone(), data.description.clone());
    project
        .add()
        .await
        .map_err(|_| Status::InternalServerError)?;
    Ok(())
}
