use anyhow::{anyhow, Result};
use std::fs;
use std::path::Path;
use std::{
    path::PathBuf,
    process::{Command, ExitStatus},
};

trait ExitOkPolyfill {
    fn exit_ok_polyfilled(&self) -> Result<()>;
}

impl ExitOkPolyfill for ExitStatus {
    fn exit_ok_polyfilled(&self) -> Result<()> {
        if self.success() {
            Ok(())
        } else {
            Err(anyhow!("process exited with non-zero status"))
        }
    }
}

pub fn start_build(root: &PathBuf) -> Result<()> {
            let config = crate::config::load_config(&root)?;
            let builddep_exit = Command::new("sudo")
                .args(vec![
                    "dnf",
                    "builddep",
                    "-y",
                    config.package.spec.to_str().unwrap(),
                ])
                .current_dir(&root)
                .status()?;

            builddep_exit.exit_ok_polyfilled()?;

            let rpmbuild_exit = Command::new("rpmbuild")
                .args(vec![
                    "-ba",
                    config.package.spec.to_str().unwrap(),
                    "--define",
                    format!("_rpmdir anda-build").as_str(),
                    "--define",
                    format!("_srcrpmdir anda-build").as_str(),
                    "--define",
                    "_disable_source_fetch 0",
                    "--define",
                    format!("_sourcedir {}", fs::canonicalize(&root)?.to_str().unwrap()).as_str(),
                ])
                .current_dir(&root)
                .status()?;

            rpmbuild_exit.exit_ok_polyfilled()?;

    Ok(())
}
