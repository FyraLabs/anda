//! # rpmspec-rs
//! RPM Spec parser in Rust
//!
//! RPMs are built from sources using a spec file. The spec file
//! contains information on how to build the package, what files to include,
//! and what dependencies are required.
//!
//! RPMs make use of macros, which are evaluated at build time. Macros are
//! defined in the spec files and various other files in the macros directory.
//! They are also picked up from ~/.rpmrc and /etc/rpmrc.
//!

mod error;
mod parse;
// mod rpmio;
// mod spec;
// mod utils;

pub fn add(left: usize, right: usize) -> usize {
	left + right
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn it_works() {
		let result = add(2, 2);
		assert_eq!(result, 4);
	}
}
