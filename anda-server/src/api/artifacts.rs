use std::collections::HashMap;
use std::path::PathBuf;

use rocket::form::{DataField, Form};
use rocket::fs::TempFile;
use rocket::{Route};
use rocket::{serde::{json::Json, Deserialize}, fs::FileServer, fs::{relative, Options}, State};
use sea_orm::DatabaseConnection;
use crate::prelude::*;
use rocket::serde::uuid::Uuid;

pub(crate) fn routes() -> Vec<Route> {
    routes![
        index,
        get,
        upload,
    ]
}

#[derive(FromForm)]
pub struct ArtifactUpload<'r> {
    build_id: String,
    // Dynamic form field for files
    // Can be multiple forms, starts with file/<path>
    files: HashMap<String, TempFile<'r>>,
}


#[get("/?<limit>&<offset>")]
async fn index(offset: Option<u64>,limit: Option<u64>) -> Json<Vec<Artifact>> {
    let arts = Artifact::list(limit.unwrap_or(100),offset.unwrap_or(0)).await;
    Json(arts.unwrap())
}

#[get("/<id>", rank = 5)]
async fn get(id: Uuid) -> Option<Json<Artifact>> {
    //NOTE: ID is a path string to the file, so we probably need to see if Rocket can handle escaping slashes
    match Artifact::get(id).await {
        Ok(art) => Some(Json(art)),
        Err(_) => None,
    }
}

// Upload artifact (entire folders) with form data
#[post("/", data = "<data>")]
async fn upload(data: Form<ArtifactUpload<'_>>) {

    // Get the build ID
    let build_id = data.build_id.as_str();
    println!("Build ID: {}", build_id);
    // Get the files
    let files = &data.files;

    // for each file in the hashmap, print the name and path
    for (name, file) in files.iter() {
        println!("{}: {:?}", name, file);
    }


}


#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_new_artifact() {
        let worker = Uuid::new_v4();
        let target = Target::new("test".to_string(), None, 0);
        Target::add(&target).await.unwrap();
        let build = Build::new(worker, 0, target.id, None);
        Build::add(&build).await.unwrap();
        let art = Artifact::new(build.id, "test".to_string(), "url".to_string());

        let test = Artifact::add(&art).await.unwrap();

        println!("{:?}", test);
    }
}
