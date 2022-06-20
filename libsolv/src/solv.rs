pub struct Solv {
    pub pool: *mut libsolv_bind::Pool,
    pub solv: *mut libsolv_bind::Solver,
}
