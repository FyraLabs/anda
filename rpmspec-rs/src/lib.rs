//! # rpmspec-rs
//! RPM Spec parser in Rust
//!
//! RPMs are built from sources using a spec file. The spec file
//! contains information on how to build the package, what files to include,
//! and what dependencies are required.
//!
//! RPMs make use of macros, which are evaluated at build time. Macros are
//! defined in the spec files and various other files in the macros directory.
//! They are also picked up from ~/.rpmrc and /etc/rpmrc.
//!
#![warn(clippy::pedantic)]
#![warn(clippy::nursery)]
#![warn(clippy::disallowed_types)]
#![warn(clippy::all)]

pub mod error;
pub mod parse;
#[macro_use]
mod util;
pub mod lua;
