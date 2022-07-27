use crate::backend::Project;
use rocket::form::Form;
use rocket::http::Status;
use rocket::serde::json::Json;
use rocket::serde::uuid::Uuid;
use rocket::Route;

pub(crate) fn routes() -> Vec<Route> {
    routes![index, get, new,]
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

#[post("/", data = "<data>")]
async fn new(data: Form<ProjectNew>) -> Result<(), Status> {
    let project = Project::new(data.name.clone(), data.description.clone());
    project
        .add()
        .await
        .map_err(|_| Status::InternalServerError)?;
    Ok(())
}
