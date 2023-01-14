/// This file contains functions for andax
/// which implements procedures from building RPMs
/// see anda rpm_spec.rs
use rhai::plugin::*;

// 正にこうです。 :3
macro_rules! rpmargs {
    ($a:expr, $spec:expr, $sources:expr) => {
        [
            "mock",
            $a,
            "--spec",
            $spec,
            "--sources",
            $sources.unwrap_or(""),
            "--resultdir",
            &format!(
                "{:?}",
                tempfile::Builder::new()
                    .prefix("anda-srpm")
                    .tempdir()
                    .expect("Cannot make dir?")
                    .path()
            ),
            "--enable-network",
            "--verbose",
        ]
        .into_iter()
        .map(|s| s.to_string())
        .collect()
    };
}

#[export_module]
pub mod ar {
    pub(crate) fn cmd_srpm(spec: &str, sources: Option<&str>) -> Vec<String> {
        rpmargs!("--buildsrpm", spec, sources)
    }
    pub(crate) fn cmd_rpm(spec: &str, sources: Option<&str>) -> Vec<String> {
        rpmargs!("--rebuild", spec, sources)
    }
}
