pub struct Pool {
    pool: *mut libsolv_bind::Pool,
    pub repos: Vec<libsolv_bind::Repo>,
}

impl Pool {
    pub fn new() -> Pool {
        Pool {
            pool: unsafe { libsolv_bind::pool_create() },
            repos: Vec::new(),
        }
    }
}