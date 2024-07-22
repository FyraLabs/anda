//! AndaX, an embedded scripting system powered by Rhai.
//!
//! To start running a script, use `run()`.
#![warn(clippy::pedantic)]
#![warn(clippy::nursery)]
#![warn(clippy::disallowed_types)]
#![warn(missing_docs)]
#![allow(clippy::doc_markdown)]
#![allow(clippy::module_name_repetitions)]
mod error;
mod fns;
mod run;

pub use fns::rpm::RPMSpec;
pub use rhai::Map;
pub use run::errhdl;
pub use run::run;

/// The usual Error type returned by the Rhai engine.
/// Alias for `Box<EvalAltResult>`.
pub type RhaiErr = Box<rhai::EvalAltResult>;
