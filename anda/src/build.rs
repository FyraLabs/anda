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

#[derive(Debug, Clone)]
pub struct ProjectBuilder {
    root: PathBuf,
}

impl ProjectBuilder {
    pub fn new(root: PathBuf) -> Self {
        ProjectBuilder { root }
    }

    ///  Builds an Andaman project.
    pub fn build(&self) -> Result<()> {
        // TODO: Move this to a method called `build_rpm` as we support more project types
        let config = crate::config::load_config(&self.root)?;
        sudo::escalate_if_needed().unwrap();
        let builddep_exit = Command::new("dnf")
            .args(vec![
                "builddep",
                "-y",
                config.package.spec.to_str().unwrap(),
            ])
            .current_dir(&self.root)
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
                format!("_sourcedir {}", fs::canonicalize(&self.root)?.to_str().unwrap()).as_str(),
            ])
            .current_dir(&self.root)
            .status()?;

        rpmbuild_exit.exit_ok_polyfilled()?;

        Ok(())
    }
}
