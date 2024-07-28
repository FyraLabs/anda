//! AndaX, an embedded scripting system powered by Rhai.
//!
//! To start running a script, use `run()`.
#![allow(clippy::doc_markdown)]
#![allow(clippy::module_name_repetitions)]
// Since Rhai relies on implicit lifetimes a lot, we are not going to deny rust_2018_idioms.
mod error;
mod fns;
mod run;

pub use fns::rpm::RPMSpec;
pub use rhai::{self, Map};
pub use run::{errhdl, run};

/// The usual Error type returned by the Rhai engine.
/// Alias for `Box<EvalAltResult>`.
pub type RhaiErr = Box<rhai::EvalAltResult>;
