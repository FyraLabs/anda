use crate::repo::Repo;
use super::SELECTION_FLAT;
use super::SELECTION_NAME;
use crate::cstr;
use anyhow::{anyhow, Result};
use crate::solv::Queue;

pub struct Pool {
    pub(crate) pool: *mut libsolv_bind::Pool,
}

impl Pool {
    pub fn new() -> Pool {
        Pool {
            pool: unsafe { libsolv_bind::pool_create() },
        }
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