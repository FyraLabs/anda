use crate::backend::{AndaBackend, Build, BuildCache, S3Object, Target};

use rocket::{
    form::Form,
    fs::TempFile,
    http::Status,
    serde::{json::Json, uuid::Uuid},
    Route,
};

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
    Build::get(id).await.map(Json).ok()
}

#[get("/by_target/<target_id>")]
async fn get_by_target(target_id: Uuid) -> Option<Json<Vec<Build>>> {
    Build::get_by_target_id(target_id).await.map(Json).ok()
}

#[derive(FromForm)]
pub struct BuildSubmission<'r> {
    project_id: Option<Uuid>,
    target_id: Uuid,
    src_file: TempFile<'r>,
}

#[post("/", data = "<data>")]
async fn submit(data: Form<BuildSubmission<'_>>) -> Result<Json<Build>, Status> {
    debug!("{:?}", data.target_id);
    let target = Target::get(data.target_id).await.map_err(|_| Status::NotFound)?;

    //println!("{:?}", target);
    // src_file build
    //let backend = AndaBackend::new_src_file(data.src_file.as_ref().unwrap(), data.build_type.as_ref().unwrap());
    // upload the file to S3

    //println!("{:?}", data.src_file.name());
    let cache = BuildCache::new(
        data.src_file
            .raw_name()
            .ok_or(Status::BadRequest)?
            .dangerous_unsafe_unsanitized_raw()
            .to_string(),
    )
    .upload_file(
        data.src_file
            .path()
            .ok_or(Status::InternalServerError)?
            .to_path_buf(),
    )
    .await
    .map_err(|_| Status::InternalServerError)?;

    debug!("Generating build");
    let build_id = Uuid::new_v4();

    // process backend request

    let build = AndaBackend::new(
        build_id,
        cache,
        target.image.unwrap_or_else(|| "local-registry:38675/anda/anda-client".to_string()),
    );
    build
        .build()
        .await
        .map_err(|_| Status::InternalServerError)?;

    let build = Build::new(
        Some(target.id),
        data.project_id,
        None,
        "BuildSubmission".to_string(),
    );

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
