use std::collections::HashMap;
use std::path::PathBuf;

use crate::backend::S3Object;
use crate::db_object::*;
use rocket::form::Form;
use rocket::fs::TempFile;
use rocket::serde::json::Json;
use rocket::serde::uuid::Uuid;
use rocket::Route;

pub(crate) fn routes() -> Vec<Route> {
    routes![index, get, upload,]
}

#[derive(FromForm)]
pub struct ArtifactUpload<'r> {
    build_id: Uuid,
    // Dynamic form field for files
    // Can be multiple forms, starts with file/<path>
    files: HashMap<String, TempFile<'r>>,
}

#[get("/?<limit>&<page>")]
async fn index(page: Option<usize>, limit: Option<usize>) -> Json<Vec<Artifact>> {
    let arts = Artifact::list(limit.unwrap_or(100).try_into().unwrap(), page.unwrap_or(0)).await;
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
async fn upload(data: Form<ArtifactUpload<'_>>) -> Json<Vec<Artifact>> {
    // Get the build ID
    let build_id = data.build_id;
    println!("Build ID: {}", build_id);
    // Get the files
    let files = &data.files;

    let mut results = Vec::new();

    // for each file in the hashmap, print the name and path
    for (name, file) in files.iter() {
        println!("{}: {}", name, file.path().unwrap().display());
        let artifact = crate::backend::Artifact::new(
            file.raw_name()
                .unwrap()
                .dangerous_unsafe_unsanitized_raw()
                .to_string(),
            name.to_string(),
            build_id,
        )
        .upload_file(file.path().unwrap().to_path_buf())
        .await
        .unwrap();

        // Upload the file to S3
        // Add the artifact to the results vector
        results.push(artifact.metadata().await.unwrap());
    }

    Json(results)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_new_artifact() {
        let worker = Uuid::new_v4();
        let target = Target::new("test".to_string(), None, "noarch".to_string());
        Target::add(&target).await.unwrap();
        let build = Build::new(worker, 0, None, "test");
        Build::add(&build).await.unwrap();
        /* let art = Artifact::new(build.id, "test".to_string(), "url".to_string());

        let test = Artifact::add(&art).await.unwrap();

        println!("{:?}", test); */
    }
}
