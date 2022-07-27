use crate::{backend::S3Object, db_object::*};
use rocket::{
    form::Form,
    fs::TempFile,
    serde::{json::Json, uuid::Uuid},
    Route,
};
use log::debug;
use std::collections::HashMap;

pub(crate) fn routes() -> Vec<Route> {
    routes![index, get, upload, search]
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
    Json(
        Artifact::list(limit.unwrap_or(100), page.unwrap_or(0))
            .await
            .expect("Failed to list artifacts"),
    )
}

#[get("/<id>", rank = 5)]
async fn get(id: Uuid) -> Option<Json<Artifact>> {
    //NOTE: ID is a path string to the file, so we probably need to see if Rocket can handle escaping slashes
    Artifact::get(id).await.ok().map(Json)
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
            .expect("Failed to upload build file to S3")
            .metadata()
            .await
            .expect("Failed to get metadata"),
        );
    }

    Json(results)
}

#[get("/search?<query>")]
async fn search(query: String) -> Json<Vec<Artifact>> {
    Json(
        Artifact::search(&query)
            .await
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_new_artifact() {
        let worker = Uuid::new_v4();
        let target = Target::new("test".to_string(), None, "noarch".to_string());
        Target::add(&target).await.unwrap();
        let build = Build::new(0, None, "test");
        Build::add(&build).await.unwrap();
        let art = Artifact::new(
            Uuid::new_v4(),
            build.id,
            "test".to_string(),
            "url".to_string(),
        );

        let test = Artifact::add(&art).await.unwrap();

        println!("{:?}", test);
    }
}
