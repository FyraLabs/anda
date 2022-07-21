use crate::backend::{AndaBackend, UploadCache};
use crate::db_object::*;
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
        get_by_target,
        submit,
        update_status,
        tag_compose
    ]
}

#[get("/?<limit>&<page>")]
async fn index(page: Option<usize>, limit: Option<usize>) -> Json<Vec<Build>> {
    let builds = Build::list(limit.unwrap_or(100), page.unwrap_or(0)).await;
    Json(builds.unwrap())
}

#[get("/<id>")]
async fn get(id: Uuid) -> Option<Json<Build>> {
    match Build::get(id).await {
        Ok(build) => Some(Json(build)),
        Err(_) => None,
    }
}

#[get("/by_target/<target_id>")]
async fn get_by_target(target_id: Uuid) -> Option<Json<Vec<Build>>> {
    match Build::get_by_target_id(target_id).await {
        Ok(builds) => Some(Json(builds)),
        Err(_) => None,
    }
}

#[derive(FromForm)]
pub struct BuildSubmission<'r> {
    worker: Uuid,
    project_id: Option<Uuid>,
    url: Option<String>,
    src_file: Option<TempFile<'r>>,
    build_type: Option<String>,
}

#[post("/", data = "<data>")]
async fn submit(data: Form<BuildSubmission<'_>>) -> Result<Json<Build>, Status> {
    // check if both url and src_file are empty

    if data.url.is_none() && data.src_file.is_none() {
        // return error: invalid form: both url and src_file are empty
        return Err(Status::PreconditionRequired);
    }

    // check if both url and src_file are not empty
    if data.url.is_some() && data.src_file.is_some() {
        // return error: invalid form: both url and src_file are not empty
        return Err(Status::PreconditionRequired);
    }

    // match on url or src_file
    let build_type = if data.url.is_some() {
        0
    } else if data.src_file.is_some() {
        1
    } else {
        // return error: invalid form: neither url nor src_file is not empty
        return Err(Status::PreconditionRequired);
    };

    match build_type {
        0 => {
            let backend = AndaBackend::new_url(data.url.as_ref().unwrap());

            backend.build().await.unwrap();
        }
        1 => {
            // src_file build
            //let backend = AndaBackend::new_src_file(data.src_file.as_ref().unwrap(), data.build_type.as_ref().unwrap());
            // upload the file to S3
            UploadCache::new(
                data.src_file
                    .as_ref()
                    .unwrap()
                    .path()
                    .unwrap()
                    .to_path_buf(),
                data.src_file
                    .as_ref()
                    .unwrap()
                    .raw_name()
                    .unwrap()
                    // Rocket thinks this is unsafe, but for us, this is exactly what we want. We'll be using S3 anyway.
                    .dangerous_unsafe_unsanitized_raw()
                    .to_string(),
            )
            .upload()
            .await
            .unwrap();
        }
        _ => {
            // return error: invalid form: neither url nor src_file is not empty
            return Err(Status::PreconditionRequired);
        }
    }

    // process backend request

    let build = Build::new(data.worker, 0, data.project_id).add().await;
    Ok(Json(build.unwrap()))
}

#[derive(FromForm)]
struct BuildUpdateStatus {
    id: Uuid,
    status: i32,
}

#[post("/update_status", data = "<data>")]
async fn update_status(data: Form<BuildUpdateStatus>) -> Json<Build> {
    let build = Build::get(data.id)
        .await
        .expect("Failed to update build status")
        .update_status(data.status)
        .await;
    Json(build.unwrap())
}

// TODO: Tag target?

#[derive(FromForm)]
struct BuildTagCompose {
    id: Uuid,
    tag: Uuid,
}

#[post("/tag_compose", data = "<data>")]
async fn tag_compose(data: Form<BuildTagCompose>) -> Json<Build> {
    let build = Build::get(data.id)
        .await
        .expect("Failed to update build status")
        .tag_compose(data.tag)
        .await;
    Json(build.unwrap())
}
