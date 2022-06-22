use libsolv_bind;
mod util;
pub mod solv;
pub mod checksum;
pub mod repo;
pub mod pool;
use libc::{c_char, c_int};
pub const SELECTION_NAME: c_int = 1 << 0;
pub const SELECTION_FLAT: c_int = 1 << 10;
pub const SOLVER_FLAG_BEST_OBEY_POLICY: c_int = 12;


#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
