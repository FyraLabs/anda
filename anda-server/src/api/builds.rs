use crate::{
    backend::{
        AndaBackend, Artifact, ArtifactDb, Build, BuildCache, BuildDb, BuildMeta, BuildStatus,
        DatabaseEntity, S3Object, Target,
    },
    tasks::{format_actual_stream, format_stream, full_logs_db},
};

use futures::StreamExt;
use rocket::{
    form::Form,
    fs::TempFile,
    http::Status,
    response::stream::Event,
    response::stream::EventStream,
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
        tag_project,
        get_log,
        get_artifacts,
        update_metadata,
    ]
}

#[get("/?<limit>&<page>&<all>")]
async fn index(
    page: Option<usize>,
    limit: Option<usize>,
    all: Option<bool>,
) -> Result<Json<Vec<Build>>, Status> {
    let builds = if all.unwrap_or(false) {
        Build::list_all()
            .await
            .map_err(|_| Status::InternalServerError)
            .unwrap()
    } else {
        Build::list(limit.unwrap_or(100), page.unwrap_or(0))
            .await
            .map_err(|_| Status::InternalServerError)
            .unwrap()
    };
    Ok(Json(builds))
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
    src_file: Option<TempFile<'r>>,
    url: Option<String>,
    project: Option<String>,
}

#[post("/", data = "<data>")]
async fn submit(data: Form<BuildSubmission<'_>>) -> Result<Json<Build>, Status> {
    let default_image = "local-registry:5050/anda/anda-client".to_string();

    debug!("{:?}", data.target_id);
    let target = Target::get(data.target_id)
        .await
        .map_err(|_| Status::NotFound)?;

    //println!("{:?}", target);
    // src_file build
    //let backend = AndaBackend::new_src_file(data.src_file.as_ref().unwrap(), data.build_type.as_ref().unwrap());
    // upload the file to S3

    //println!("{:?}", data.src_file.name());

    let mut int_build: Option<Build> = None;

    if let Some(src_file) = data.src_file.as_ref() {
        let cache = BuildCache::new(
            src_file
                .raw_name()
                .ok_or(Status::BadRequest)?
                .dangerous_unsafe_unsanitized_raw()
                .to_string(),
        )
        .upload_file(
            src_file
                .path()
                .ok_or(Status::InternalServerError)?
                .to_path_buf(),
        )
        .await
        .map_err(|_| Status::InternalServerError)?;

        debug!("Generating build");
        // process backend request
        int_build = Some(
            Build::new(
                Some(target.id),
                data.project_id,
                None,
                "BuildFromPack".to_string(),
            )
            .add()
            .await
            .map_err(|_| Status::InternalServerError)?,
        );
        let build = AndaBackend::new(
            int_build.clone().unwrap().id,
            Some(cache),
            None,
            target.image.unwrap_or(default_image),
        );
        //debug!("{:?}", int_build);
        // actually add build to database
        build
            .build(data.project.as_deref())
            .await
            .map_err(|_| Status::InternalServerError)?;
    } else if let Some(url) = data.url.as_ref() {
        // we skip the uploading process
        int_build = Some(
            Build::new(
                Some(target.id),
                data.project_id,
                None,
                "BuildFromUrl".to_string(),
            )
            .add()
            .await
            .map_err(|_| Status::InternalServerError)?,
        );

        let build = AndaBackend::new(
            int_build.clone().unwrap().id,
            None,
            Some(url.to_string()),
            target.image.unwrap_or(default_image),
        );

        build
            .build(data.project.as_deref())
            .await
            .map_err(|_| Status::InternalServerError)?;
    } else {
        return Err(Status::BadRequest);
    }

    Ok(Json(int_build.unwrap()))
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

#[derive(FromForm)]
struct BuildTag {
    id: Uuid,
    tag: Uuid,
}

#[post("/tag_compose", data = "<data>")]
async fn tag_compose(data: Form<BuildTag>) -> Json<Build> {
    let build = Build::get(data.id)
        .await
        .expect("Failed to tag build to compose")
        .tag_compose(data.tag)
        .await;
    Json(build.unwrap())
}

#[post("/tag", data = "<data>")]
async fn tag(data: Form<BuildTag>) -> Json<Build> {
    let build = Build::get(data.id)
        .await
        .expect("Failed to tag build")
        .tag_target(data.tag)
        .await;
    Json(build.unwrap())
}

#[post("/tag_project", data = "<data>")]
async fn tag_project(data: Form<BuildTag>) -> Json<Build> {
    let build = Build::get(data.id)
        .await
        .expect("Failed to tag build")
        .tag_project(data.tag)
        .await;
    Json(build.unwrap())
}

// Log streaming using server sent events
#[get("/<id>/log", rank = 5)]
async fn get_log(id: Uuid) -> Result<EventStream![], Status> {
    let build = Build::get(id).await.map_err(|_| Status::NotFound)?;

    let mut logstream =
        if build.status != BuildStatus::Running && build.status != BuildStatus::Pending {
            // get full logs
            let logs = full_logs_db(build.id.to_string()).await.unwrap();

            //println!("{:?}", logs);
            format_stream(logs).await.unwrap().boxed()
        } else {
            let logstream = crate::tasks::stream_logs(id.to_string()).await;
            format_actual_stream(logstream.unwrap())
                .await
                .unwrap()
                .boxed()
        };
    Ok(EventStream! {
        // TODO: catch errors
        while let Some(log) = logstream.next().await {
            yield Event::data(log);
        }
        yield Event::data("Log stream ended".to_string()).event("end");
    })
}

#[get("/<id>/artifacts", rank = 5)]
async fn get_artifacts(id: Uuid) -> Option<Json<Vec<Artifact>>> {
    Artifact::get_by_build_id(id).await.map(Json).ok()
}

#[post("/<id>/metadata", data = "<data>")]
async fn update_metadata(id: Uuid, data: Json<BuildMeta>) -> Json<Build> {
    let build = Build::get(id)
        .await
        .expect("Failed to update build metadata")
        .update_metadata(data.0)
        .await;
    Json(build.unwrap())
}
