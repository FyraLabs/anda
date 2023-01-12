/// This file contains functions for andax
/// which implements procedures from building RPMs
/// see anda rpm_spec.rs
use rhai::plugin::*;

#[export_module]
pub mod anda_rhai {
    fn cmd_srpm(spec: &str, sources: Option<&str>) -> Vec<String> {
        [
            "mock",
            "--buildsrpm",
            "--spec",
            spec,
            "--sources",
            sources.unwrap_or(""),
            "--resultdir",
            format!(
                "{:?}",
                tempfile::Builder::new()
                    .prefix("anda-srpm")
                    .tempdir()
                    .expect("Cannot make dir?")
                    .path()
            )
            .as_str(),
            "--enable-network",
            "--verbose",
            ]
        .into_iter()
        .map(|s| s.to_string())
        .collect()
    }
}
