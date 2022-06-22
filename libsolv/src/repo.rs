use anyhow::{anyhow, Result};
use libc::{c_char, c_int};
use std::{ffi::CString, os::unix::prelude::OsStrExt, path::Path, ptr::null_mut};
use super::{SELECTION_FLAT, SELECTION_NAME,SOLVER_FLAG_BEST_OBEY_POLICY};

use crate::{pool::Pool, cstr};
pub struct Repo {
    pub(crate) repo: *mut libsolv_bind::Repo,
}

impl Repo {
    pub fn new(pool: &Pool, name: &str) -> Result<Repo> {
        let name = CString::new(name)?;
        Ok(Repo {
            repo: unsafe { libsolv_bind::repo_create(pool.pool, name.as_ptr()) },
        })
    }

    pub fn add_rpm(&mut self, path: &Path) -> Result<()> {
        // open file
        let mut path_buf = path.as_os_str().as_bytes().to_owned();
        path_buf.push(0);
        let fp = unsafe { libc::fopen(path_buf.as_ptr() as *const c_char, cstr!("rb")) };

        // convert fp to *const c_char (i8)
        let fp_ptr = fp as *const c_char;
        if fp_ptr.is_null() {
            return Err(anyhow!("failed to open {}", path.display()));
        }

        let result = unsafe { libsolv_bind::repo_add_rpm(self.repo, fp_ptr, 0) };

        unsafe {
            libc::fclose(fp);
        }
        if result != 0 {
            return Err(anyhow!("Failed to add rpm: {}", result));
        }
        Ok(())
    }

    pub fn add_rpmdb(&mut self) -> Result<()> {
        let result = unsafe { libsolv_bind::repo_add_rpmdb(self.repo, self.repo, 0) };
        if result != 0 {
            return Err(anyhow!("Failed to add rpmdb: {}", result));
        }
        Ok(())
    }
}
