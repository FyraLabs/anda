use crate::backend::{AndaBackend, BuildCache};
use crate::backend::{BuildMethod, S3Object, Build};
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
        tag_compose,
        tag,
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

    let build: BuildMethod;
    match build_type {
        0 => {
            build = BuildMethod::Url {
                url: data.url.as_ref().unwrap().to_string(),
            };
        }
        1 => {
            // src_file build
            //let backend = AndaBackend::new_src_file(data.src_file.as_ref().unwrap(), data.build_type.as_ref().unwrap());
            // upload the file to S3
            let cache = BuildCache::new(
                data.src_file
                    .as_ref()
                    .unwrap()
                    .raw_name()
                    .unwrap()
                    .dangerous_unsafe_unsanitized_raw()
                    .to_string(),
            )
            .upload_file(
                data.src_file
                    .as_ref()
                    .unwrap()
                    .path()
                    .unwrap()
                    .to_path_buf(),
            )
            .await
            .unwrap();

            // send file to backend for processing

            build = BuildMethod::SrcFile {
                path: data
                    .src_file
                    .as_ref()
                    .unwrap()
                    .path()
                    .unwrap()
                    .to_path_buf(),
                filename: data
                    .src_file
                    .as_ref()
                    .unwrap()
                    .raw_name()
                    .unwrap()
                    .dangerous_unsafe_unsanitized_raw()
                    .to_string(),
            };
        }
        _ => {
            // return error: invalid form: neither url nor src_file is not empty
            return Err(Status::BadRequest);
        }
    }

    // process backend request

    let build = AndaBackend::new_build(build, data.project_id).await.unwrap();

    Ok(Json(build))
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
        .update_status(num::FromPrimitive::from_i32(data.status).unwrap())
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
        .expect("Failed to tag build to compose")
        .tag_compose(data.tag)
        .await;
    Json(build.unwrap())
}

#[derive(FromForm)]
struct BuildTagTarget {
    id: Uuid,
    tag: Uuid,
}

#[post("/tag", data = "<data>")]
async fn tag(data: Form<BuildTagTarget>) -> Json<Build> {
    let build = Build::get(data.id)
        .await
        .expect("Failed to tag build")
        .tag(data.tag)
        .await;
    Json(build.unwrap())
}
