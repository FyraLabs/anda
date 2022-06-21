//! Bindings for libsolv in Rust
//! Copyright (C) 2022 The Ultramarine Project
//! Written for use with the Andaman Project
//! Licensed under the MIT license
//!
// Most of this code is actually copied from the ABBS metadata toolkit
// , Adapted for use with RPM packages and more.
// (https://github.com/AOSC-Dev/abbs-meta-rs)

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
    pub fn match_package(&self, name: &str, mut queue: Queue) -> Result<Queue> {
        if unsafe { (*self.pool).whatprovides.is_null() } {
            // we can't call createwhatprovides here because of how libsolv manages internal states
            return Err(anyhow!(
                "internal error: `create_whatprovides` needs to be called first."
            ));
        }
        let ret = unsafe {
            libsolv_bind::selection_make(
                self.pool,
                &mut queue.queue,
                cstr!(name),
                SELECTION_NAME | SELECTION_FLAT,
            )
        };
        if ret < 1 {
            return Err(anyhow!("Error matching the package: {}", name));
        }

        Ok(queue)
    }
    pub fn create_whatprovides(&mut self) {
        unsafe { libsolv_bind::pool_createwhatprovides(self.pool) }
    }
    pub fn set_installed(&mut self, repo: &Repo) {
        unsafe { libsolv_bind::pool_set_installed(self.pool, repo.repo) }
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

        unsafe { libc::fclose(fp);}
        if result != 0 {
            return Err(anyhow!("Failed to add rpm: {}", result));
        }
        Ok(())
    }

    pub fn add_rpmdb(&mut self) -> Result<()> {
        let result = unsafe { libsolv_bind::repo_add_rpmdb(self.repo, self.repo, 0 ) };
        if result != 0 {
            return Err(anyhow!("Failed to add rpmdb: {}", result));
        }
        Ok(())
    }
}

pub struct Queue {
    queue: libsolv_bind::Queue,
}

impl Queue {
    pub fn new() -> Queue {
        Queue {
            queue: libsolv_bind::Queue {
                elements: null_mut(),
                count: 0,
                alloc: null_mut(),
                left: 0,
            },
        }
    }

    pub fn mark_all_as(&mut self, flags: c_int) {
        for item in (0..self.queue.count).step_by(2) {
            unsafe {
                let addr = self.queue.elements.offset(item.try_into().unwrap());
                (*addr) |= flags;
            }
        }
    }

    pub fn push2(&mut self, a: c_int, b: c_int) {
        self.push(a);
        self.push(b);
    }

    pub fn push(&mut self, item: c_int) {
        if self.queue.left < 1 {
            unsafe { libsolv_bind::queue_alloc_one(&mut self.queue) }
        }
        self.queue.count += 1;
        unsafe {
            let elem = self.queue.elements.offset(self.queue.count as isize);
            (*elem) = item;
        }
        self.queue.left -= 1;
    }

    pub fn extend(&mut self, q: &Queue) {
        unsafe {
            libsolv_bind::queue_insertn(
                &mut self.queue,
                self.queue.count,
                q.queue.count,
                q.queue.elements,
            )
        }
    }
}

impl Drop for Queue {
    fn drop(&mut self) {
        unsafe { libsolv_bind::queue_free(&mut self.queue) }
    }
}


pub struct Transaction {
    t: *mut libsolv_bind::Transaction,
}

impl Transaction {
    pub fn get_size_change(&self) -> i64 {
        unsafe { libsolv_bind::transaction_calc_installsizechange(self.t) }
    }

    pub fn order(&self, flags: c_int) {
        unsafe { libsolv_bind::transaction_order(self.t, flags) }
    }

    // todo: create metadata for the transaction
}

impl Drop for Transaction {
    fn drop(&mut self) {
        unsafe { libsolv_bind::transaction_free(self.t) }
    }
}

pub struct Solver {
    solver: *mut libsolv_bind::Solver,
}

impl Solver {
    pub fn new(pool: &Pool) -> Solver {
        Solver {
            solver: unsafe { libsolv_bind::solver_create(pool.pool) },
        }
    }
}

impl Drop for Solver {
    fn drop(&mut self) {
        unsafe { libsolv_bind::solver_free(self.solver) }
    }
}
