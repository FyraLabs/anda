use anyhow::{anyhow, Result};
use libc::{c_char, c_int};
pub const SELECTION_NAME: c_int = 1 << 0;
pub const SELECTION_FLAT: c_int = 1 << 10;
use std::{ffi::CString, path::Path, ptr::null_mut, os::unix::prelude::OsStrExt};
pub const SOLVER_FLAG_BEST_OBEY_POLICY: c_int = 12;

macro_rules! cstr {
    ($s:expr) => {
        CString::new($s).unwrap().as_ptr() as *const c_char
    };
}


pub struct Pool {
    pool: *mut libsolv_bind::Pool,
}

impl Pool {
    pub fn new() -> Pool {
        Pool { pool: unsafe { libsolv_bind::pool_create() } }
    }
    pub fn createwhatprovides(&mut self) {
        unsafe { libsolv_bind::pool_createwhatprovides(self.pool) }
    }

}


impl Drop for Pool {
    fn drop(&mut self) {
        unsafe { libsolv_bind::pool_free(self.pool) }
    }
}


pub struct Repo {
    repo: *mut libsolv_bind::Repo,
}

impl Repo {
    pub fn new(pool: &Pool, name: &str) -> Result<Repo> {
        let name = CString::new(name)?;
        Ok(Repo {
            repo: unsafe { libsolv_bind::repo_create(pool.pool, name.as_ptr()) },
        })
    }

    pub fn add_rpm(&mut self, path: &Path) {
        // open file
        let mut path_buf = path.as_os_str().as_bytes().to_owned();
        path_buf.push(0);
        let fp = unsafe { libc::fopen(path_buf.as_ptr() as *const c_char, cstr!("rb")) };

        //let result = unsafe { libsolv_bind::repo_add_rpm(self.repo, fp as *mut libsolv_bind::_IO_FILE, 0) };

        unsafe { libc::fclose(fp);}
    }
}