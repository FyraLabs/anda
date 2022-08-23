use crate::backend::{Artifact, S3Object};
use crate::{backend::*};
use log::debug;
use rocket::http::Status;
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
    routes![index, get, upload, search, get_file]
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

/// WIP: Directory Listing
#[get("/files/<path..>")]
async fn get_file(path: PathBuf) -> Option<(ContentType, Vec<u8>)> {
    // check if path is a file
    let s = crate::s3_object::S3Artifact::new().unwrap();
    // check if path is folder and get list of files

    let file = s.get_file(path.to_str().unwrap()).await.ok();

    if file.is_none() {
        // This code is in fact, reachable
        //#[allow(unreachable_code)]
        return Some((
            ContentType::Text,
            format!("{:#?}", s.list_files(path.to_str().unwrap()).await.unwrap().contents())
                .as_bytes()
                .to_vec(),
        ));
    } else {
        let file = file.unwrap();
        let mut buf = Vec::new();
        file.into_async_read().read_to_end(&mut buf).await.ok();
        Some((ContentType::Binary, buf))
    }

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
                file.raw_name()
                    .expect("No filename")
                    .dangerous_unsafe_unsanitized_raw()
                    .to_string(),
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
