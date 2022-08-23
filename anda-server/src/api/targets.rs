use crate::backend::Target;

use rocket::http::Status;
use rocket::serde::json::Json;
use rocket::serde::uuid::Uuid;
use rocket::Route;
use serde::{Deserialize, Serialize};

pub(crate) fn routes() -> Vec<Route> {
    routes![index, get, get_by_name, new, update, delete]
}

#[get("/?<limit>&<page>&<all>")]
async fn index(
    page: Option<usize>,
    limit: Option<usize>,
    all: Option<bool>,
) -> Result<Json<Vec<Target>>, Status> {
    let targets = if all.unwrap_or(false) {
        Target::list_all()
            .await
            .map_err(|_| Status::InternalServerError)
            .unwrap()
    } else {
        Target::list(limit.unwrap_or(100), page.unwrap_or(0))
            .await
            .map_err(|_| Status::InternalServerError)
            .unwrap()
    };
    Ok(Json(targets))
}

#[get("/<id>")]
async fn get(id: Uuid) -> Option<Json<Target>> {
    Target::get(id).await.map(Json).ok()
}

#[get("/by_name/<name>")]
async fn get_by_name(name: String) -> Option<Json<Target>> {
    Target::get_by_name(name).await.map(Json).ok()
}

#[derive(Serialize, Deserialize)]
pub struct TargetForm {
    name: String,
    arch: String,
    image: Option<String>,
}

// We're gonna use JSON for the data, so we don't need multipart

#[post("/", data = "<data>")]
async fn new(data: Json<TargetForm>) -> Result<Json<Target>, Status> {
    let target = Target::new(data.name.clone(), data.image.clone(), data.arch.clone());

    let t = target
        .add()
        .await
        .map_err(|_| Status::InternalServerError)?;
    Ok(Json(t))
}

#[post("/<id>", data = "<data>")]
async fn update(id: Uuid, data: Json<TargetForm>) -> Result<Json<Target>, Status> {
    let mut target = Target::get(id).await.map_err(|_| Status::BadRequest)?;

    target.name = data.name.clone();
    target.arch = data.arch.clone();
    target.image = data.image.clone();
    let t = target
        .update(id)
        .await
        .map_err(|_| Status::InternalServerError)?;
    Ok(Json(t))
}

#[delete("/<id>")]
async fn delete(id: Uuid) -> Result<(), Status> {
    let target = Target::get(id).await.map_err(|_| Status::BadRequest)?;
    Target::delete(&target)
        .await
        .map_err(|_| Status::InternalServerError)?;
    Ok(())
}
