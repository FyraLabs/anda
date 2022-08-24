use crate::backend::{Artifact, S3Object};
use crate::{backend::*};
use log::debug;
use rocket::fs::NamedFile;
use rocket::http::Status;
use rocket::response::Redirect;
use rocket::{
    form::Form,
    fs::TempFile,
    http::ContentType,
    serde::{json::Json, uuid::Uuid},
    Route,
};
use std::{collections::HashMap, path::PathBuf};
use tokio::io::AsyncReadExt;

pub(crate) fn routes() -> Vec<Route> {
    routes![index, get, upload, search, get_raw_file, get_file]
}

#[derive(FromForm)]
pub struct ArtifactUpload<'r> {
    build_id: Uuid,
    // Dynamic form field for files
    // Can be multiple forms, starts with file/<path>
    files: HashMap<String, TempFile<'r>>,
}
#[get("/?<limit>&<page>&<all>")]
async fn index(
    page: Option<usize>,
    limit: Option<usize>,
    all: Option<bool>,
) -> Result<Json<Vec<Artifact>>, Status> {
    let artifacts = if all.unwrap_or(false) {
        Artifact::list_all()
            .await
            .map_err(|_| Status::InternalServerError)
            .unwrap()
    } else {
        Artifact::list(limit.unwrap_or(100), page.unwrap_or(0))
            .await
            .map_err(|_| Status::InternalServerError)
            .unwrap()
    };
    Ok(Json(artifacts))
}

#[get("/<id>", rank = 5)]
async fn get(id: Uuid) -> Option<Json<Artifact>> {
    //NOTE: ID is a path string to the file, so we probably need to see if Rocket can handle escaping slashes
    Artifact::get(id).await.ok().map(Json)
}

#[get("/<id>/file", rank = 5)]
async fn get_raw_file(id: Uuid) -> Result<Redirect, Status> {
    // Gets file name, then redirects to the file
    let artifact = Artifact::get(id).await.map_err(|_| Status::NotFound)?;
    // redirect to the file
    let redirect = Redirect::to(format!("/artifacts/{}/file/{}", artifact.id, artifact.path));
    Ok(redirect)
}

/// WIP: Directory Listing
#[get("/<id>/file/<path..>", rank = 6)]
async fn get_file(id: Uuid, path: PathBuf) -> Result<(ContentType, Vec<u8>), Status> {

    let artifact = Artifact::get(id).await.unwrap();

    let data = artifact.pull_bytes().await.unwrap();
    // turn bytestream into vec of bytes
    let data = data.collect().await.ok().unwrap().into_bytes();
    // bytes to vec of bytes
    let buf = data.to_vec();
    Ok((ContentType::Binary, buf))
    //todo!()
}

// Upload artifact (entire folders) with form data
#[post("/", data = "<data>")]
async fn upload(data: Form<ArtifactUpload<'_>>) -> Json<Vec<Artifact>> {
    debug!("Build ID: {}", data.build_id);
    let mut results = Vec::new();

    // for each file in the hashmap, print the name and path
    for (name, file) in data.files.iter() {
        debug!("{}: {}", name, file.path().expect("No file path").display());
        results.push(
            crate::backend::Artifact::new(
                name.split('/').last().unwrap().to_string(),
                name.to_string(),
                data.build_id,
            )
                .upload_file(file.path().unwrap().to_path_buf())
                .await
                .expect("Failed to upload build file to S3"),
        );
    }

    Json(results)
}

#[get("/search?<query>")]
async fn search(query: String) -> Json<Vec<Artifact>> {
    Json(Artifact::search(&query).await)
}
