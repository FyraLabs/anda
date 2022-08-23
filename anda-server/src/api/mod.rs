//! API module for Andaman
//! This is where all the API endpoints are defined.
//! This means that all the code that is exposed to the outside world is here.

mod artifacts;
mod builds;
mod projects;
mod targets;
mod composes;

#[derive(Responder)]
#[response(status = 412, content_type = "json")]
pub(crate) struct InvalidPayloadError {
    pub(crate) message: String,
}

pub(crate) use self::artifacts::routes as artifacts_routes;
pub(crate) use self::builds::routes as builds_routes;
pub(crate) use self::projects::routes as projects_routes;
pub(crate) use self::targets::routes as targets_routes;
pub(crate) use self::composes::routes as composes_routes;
