#![warn(clippy::pedantic)]
#![warn(clippy::nursery)]
#![warn(clippy::disallowed_types)]
#![allow(clippy::doc_markdown)]
#![allow(clippy::module_name_repetitions)]
pub mod config;
pub mod context;
pub mod error;
pub mod template;
pub use config::*;
