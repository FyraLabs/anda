//! libsolv bindings for Rust
//! This crate only contains low-level bindings to the libsolv library.
//! You should use the idiomatic Rust bindings (libsolv-rs) instead.

use bindgen::Builder;
use std::{env, path::PathBuf};

fn main() {
    println!("cargo:rustc-link-lib=solv");

    let builder = Builder::default().header("include/wrapper.h");

    let output_path = PathBuf::from(env::var("OUT_DIR").unwrap()).join("binding.rs");

    builder
        .generate()
        .unwrap()
        .write_to_file(output_path)
        .unwrap();
}
