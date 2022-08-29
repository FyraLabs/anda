use crate::backend::{Artifact, Project};
use rocket::form::Form;
use rocket::http::Status;
use rocket::serde::json::Json;
use rocket::serde::uuid::Uuid;
use rocket::Route;
use serde::{Deserialize, Serialize};

pub(crate) fn routes() -> Vec<Route> {
    routes![index, get, get_by_name, new, get_artifacts, set_summary, delete]
}

#[get("/?<limit>&<page>&<all>")]
async fn index(
    page: Option<usize>,
    limit: Option<usize>,
    all: Option<bool>,
) -> Result<Json<Vec<Project>>, Status> {
    let projects = if all.unwrap_or(false) {
        Project::list_all()
            .await
            .map_err(|_| Status::InternalServerError)
            .unwrap()
    } else {
        Project::list(limit.unwrap_or(100), page.unwrap_or(0))
            .await
            .map_err(|_| Status::InternalServerError)
            .unwrap()
    };
    Ok(Json(projects))
}

#[get("/<id>")]
async fn get(id: Uuid) -> Option<Json<Project>> {
    Project::get(id).await.map(Json).ok()
}

#[get("/by_name/<name>", rank = 5)]
async fn get_by_name(name: String) -> Option<Json<Project>> {
    Project::get_by_name(name).await.map(Json).ok()
}

#[derive(FromForm)]
struct ProjectNew {
    name: String,
    description: Option<String>,
}

#[get("/<id>/artifacts")]
async fn get_artifacts(id: Uuid) -> Option<Json<Vec<Artifact>>> {
    let project = Project::get(id).await.ok()?;
    let a = project.list_artifacts().await.ok();
    a.map(Json)
}

#[post("/", data = "<data>")]
async fn new(data: Form<ProjectNew>) -> Result<Json<Project>, Status> {
    let project = Project::new(data.name.clone(), data.description.clone());
    project
        .add()
        .await
        .map_err(|_| Status::InternalServerError)?;
    Ok(Json(project))
}


#[delete("/<id>")]
async fn delete(id: Uuid) -> Result<(), Status> {
    let project = Project::get(id).await.map_err(|_| Status::NotFound)?;
    project.delete().await.map_err(|_| Status::InternalServerError)?;
    Ok(())
}

#[derive(Serialize, Deserialize)]
struct ProjectSummary {
    summary: Option<String>,
}

#[post("/<id>/summary", data = "<data>")]
async fn set_summary(id: Uuid, data: Json<ProjectSummary>) -> Result<(), Status> {
    let project = Project::get(id).await.map_err(|_| Status::NotFound)?;
    project
        .update_summary(data.summary.clone())
        .await
        .map_err(|_| Status::InternalServerError)?;
    Ok(())
}
