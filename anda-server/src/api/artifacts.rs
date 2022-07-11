use std::path::PathBuf;

use rocket::{Route};
use rocket::{serde::{json::Json, Deserialize}, fs::FileServer, fs::{relative, Options}, State};
use sea_orm::DatabaseConnection;
use crate::prelude::*;

pub(crate) fn routes() -> Vec<Route> {
    routes![
        index,
        get_by_type,
        get,
    ]
}


#[get("/?<limit>&<offset>")]
async fn index(offset: Option<u64>,limit: Option<u64>) -> Json<Vec<Artifact>> {
    let arts = Artifact::list(limit.unwrap_or(100),offset.unwrap_or(0)).await;
    Json(arts.unwrap())
}

#[get("/by_type/<art_type>?<limit>&<offset>")]
async fn get_by_type(art_type: String, limit: Option<u64>, offset: Option<u64>) -> Json<Vec<Artifact>> {
    let arts = Artifact::get_by_type(art_type.as_str(), limit.unwrap_or(100), offset.unwrap_or(0)).await;
    Json(arts.unwrap())
}

#[get("/<id..>", rank = 5)]
async fn get(id: PathBuf) -> Option<Json<Artifact>> {
    //NOTE: ID is a path string to the file, so we probably need to see if Rocket can handle escaping slashes
    match Artifact::get(&id.to_str().unwrap()).await {
        Ok(art) => Some(Json(art)),
        Err(_) => None,
    }
}

