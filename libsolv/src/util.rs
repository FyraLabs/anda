use anyhow::{anyhow, Result};
use libc::{c_char, c_int};
use std::{ffi::CString, os::unix::prelude::OsStrExt, path::Path, ptr::null_mut};

#[macro_export]
macro_rules! cstr {
    ($s:expr) => {
        std::ffi::CString::new($s).unwrap().as_ptr() as *const std::os::raw::c_char
    };
}