//! Bindings for libsolv in Rust
//! Copyright (C) 2022 The Ultramarine Project
//! Written for use with the Andaman Project
//! Licensed under the MIT license
//!
// Most of this code is actually copied from the ABBS metadata toolkit
// , Adapted for use with RPM packages and more.
// (https://github.com/AOSC-Dev/abbs-meta-rs)

use anyhow::{anyhow};
use libc::{c_char, c_int};
use std::{ffi::CString, os::unix::prelude::OsStrExt, path::Path, ptr::null_mut};

use crate::pool::Pool;
use crate::repo::Repo;

// import cstr macro from util.rs
use crate::cstr;

pub struct Queue {
    pub queue: libsolv_bind::Queue,
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
    pub fn delete(&mut self, item: c_int) {
        unsafe {
            libsolv_bind::queue_delete(&mut self.queue, item);
        }
    }
    pub fn delete2(&mut self, item: c_int) {
        unsafe {
            libsolv_bind::queue_delete2(&mut self.queue, item);
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
