//! API module for Andaman
//! This is where all the API endpoints are defined.
//! This means that all the code that is exposed to the outside world is here.
//! Manually create a route named hi at path "/world" mounted at base "/hello". Requests to the /hello/world URI will be dispatched to the hi route.
//!
//! use rocket::{Request, Route, Data, route};
//! use rocket::http::Method;
//!
//! fn hi<'r>(req: &'r Request, _: Data<'r>) -> route::BoxFuture<'r> {
//!     route::Outcome::from(req, "Hello!").pin()
//! }
//!
//! #[launch]
//! fn rocket() -> _ {
//!     let hi_route = Route::new(Method::Get, "/world", hi);
//!     rocket::build().mount("/hello", vec![hi_route])
//! }
use crate::db_object;
use rocket::{
    fs::FileServer,
    fs::{relative, Options},
    serde::{json::Json, Deserialize},
    State,
};
use rocket::{route, Data, Request, Route};
use sea_orm::DatabaseConnection;
mod artifacts;
mod builds;

#[derive(Responder)]
#[response(status = 412, content_type = "json")]
pub(crate) struct InvalidPayloadError {
    pub(crate) message: String,
}

pub(crate) use self::artifacts::routes as artifacts_routes;
pub(crate) use self::builds::routes as builds_routes;
