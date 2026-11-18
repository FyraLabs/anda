use andax::{Failure, ScriptError, SubmoduleChange};
use color_eyre::Result;
use serde::Serialize;
use std::{
    io::Write,
    path::{Path, PathBuf},
};

const FORMAT_VERSION: u32 = 1;

#[derive(Debug, Serialize)]
pub struct Report {
    version: u32,
    projects: Vec<ProjectReport>,
}

impl Report {
    pub fn new(mut projects: Vec<ProjectReport>) -> Self {
        projects.sort_unstable_by(|a, b| a.project.cmp(&b.project));
        Self { version: FORMAT_VERSION, projects }
    }

    pub fn write_to(&self, path: &Path) -> Result<()> {
        let dir = path
            .parent()
            .filter(|dir| !dir.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        std::fs::create_dir_all(dir)?;
        let mut file = tempfile::NamedTempFile::new_in(dir)?;
        serde_json::to_writer_pretty(&mut file, self)?;
        file.write_all(b"\n")?;
        file.persist(path)?;
        Ok(())
    }
}

#[derive(Debug, Serialize)]
pub struct ProjectReport {
    pub project: String,
    pub alias: Option<String>,
    pub script: PathBuf,
    pub spec: Option<PathBuf>,
    pub status: Status,
    pub spec_changed: bool,
    pub duration_ms: Option<u128>,
    pub error: Option<ErrorReport>,
    pub events: Vec<Event>,
}

#[derive(Debug, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum Event {
    SubmoduleAdded { path: String, url: String, commit: String, global: String },
    SubmoduleDropped { global: String, commit: String },
}

impl From<SubmoduleChange> for Event {
    fn from(change: SubmoduleChange) -> Self {
        match change {
            SubmoduleChange::Added { path, url, commit, global } => {
                Self::SubmoduleAdded { path, url, commit, global }
            }
            SubmoduleChange::Dropped { global, commit } => {
                Self::SubmoduleDropped { global, commit }
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Status {
    Ok,
    Failed,
    Terminated,
    Panicked,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ErrorKind {
    Script,
    Write,
    Panic,
}

#[derive(Debug, Serialize)]
pub struct ErrorReport {
    pub kind: ErrorKind,
    pub message: String,
    pub function: Option<String>,
    pub line: Option<usize>,
    pub column: Option<usize>,
}

impl ErrorReport {
    pub const fn plain(kind: ErrorKind, message: String) -> Self {
        Self { kind, message, function: None, line: None, column: None }
    }
}

impl From<ScriptError> for ErrorReport {
    fn from(err: ScriptError) -> Self {
        Self {
            kind: ErrorKind::Script,
            message: err.message,
            function: err.function,
            line: err.line,
            column: err.column,
        }
    }
}

pub fn failure_outcome(failure: Failure) -> (Status, Option<ErrorReport>) {
    match failure {
        Failure::Terminated => (Status::Terminated, None),
        Failure::Error(err) => (Status::Failed, Some(err.into())),
    }
}

pub fn panic_message(payload: &(dyn std::any::Any + Send)) -> String {
    payload
        .downcast_ref::<&str>()
        .map(|s| (*s).to_owned())
        .or_else(|| payload.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "the update thread panicked".to_owned())
}
