//! Builder backends for Anda

use std::process::Command;

use std::path::{Path, PathBuf};

use anyhow::{anyhow, Result};

pub trait RPMSpecBackend {
    fn build_srpm(&self, spec: &Path) -> Result<PathBuf>;
    fn build_rpm(&self, spec: &Path) -> Result<PathBuf>;

    fn build(&self, spec: &Path) -> Result<PathBuf> {
        self.build_rpm(&self.build_srpm(spec)?)
    }
}

/// An RPM spec backend that uses Mock to build RPMs
pub struct MockBackend {
    mock_config: Option<String>,
    with: Option<Vec<String>>,
    without: Option<Vec<String>>,
    sources: PathBuf,
    resultdir: PathBuf,
    extra_repos: Vec<String>,
    no_mirror: bool,
}

impl MockBackend {
    pub fn new(
        mock_config: Option<String>,
        sources: PathBuf,
        resultdir: PathBuf,
    ) -> Self {
        Self {
            mock_config,
            with: None,
            without: None,
            sources,
            resultdir,
            extra_repos: Vec::new(),
            no_mirror: false,
        }
    }

    pub fn add_extra_repo(&mut self, repo: String) {
        self.extra_repos.push(repo);
    }

    pub fn with(&mut self, with: Vec<String>) {
        self.with = Some(with);
    }

    pub fn without(&mut self, without: Vec<String>) {
        self.without = Some(without);
    }

    pub fn no_mirror(&mut self, no_mirror: bool) {
        self.no_mirror = no_mirror;
    }

    pub fn mock(&self) -> Command {
        let mut cmd = Command::new("mock");

        if let Some(config) = self.mock_config.as_ref() {
            cmd.arg("-r").arg(config);
        }

        for repo in self.extra_repos.iter() {
            cmd.arg("-a").arg(repo);
        }

        for with in self.with.as_ref().unwrap_or(&Vec::new()).iter() {
            cmd.arg("--with").arg(with);
        }

        for without in self.without.as_ref().unwrap_or(&Vec::new()).iter() {
            cmd.arg("--without").arg(without);
        }

        if self.no_mirror {
            cmd.arg("--config-opts").arg("mirrored=False");
        }
        cmd
    }
}
