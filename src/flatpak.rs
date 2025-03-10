#![allow(dead_code)]
use crate::util::CommandLog;
use color_eyre::Report;
use flatpak::application::FlatpakApplication;
use std::{
    env,
    fmt::Display,
    path::{Path, PathBuf},
};
use tokio::process::Command;
type Result<T> = std::result::Result<T, Report>;

pub enum FlatpakArtifact {
    Ref(String),
    Bundle(PathBuf),
}

impl Display for FlatpakArtifact {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Ref(r) => write!(f, "ref {r}"),
            Self::Bundle(b) => write!(f, "bundle {}", b.display()),
        }
    }
}

pub struct FlatpakBuilder {
    // The output directory for the flatpak build
    output_dir: PathBuf,
    // The output flatpak repository
    output_repo: PathBuf,

    // The bundles directory
    bundles_dir: PathBuf,
    // Extra sources as paths
    extra_sources: Vec<PathBuf>,
    // Extra sources as URLs
    extra_sources_urls: Vec<String>,
    // extra arguments to pass to flatpak-builder
    extra_args: Vec<String>,
}

impl FlatpakBuilder {
    pub const fn new(output_dir: PathBuf, output_repo: PathBuf, bundles_dir: PathBuf) -> Self {
        Self {
            output_dir,
            output_repo,
            bundles_dir,
            extra_sources: Vec::new(),
            extra_sources_urls: Vec::new(),
            extra_args: Vec::new(),
        }
    }

    pub fn add_extra_source(&mut self, source: PathBuf) {
        self.extra_sources.push(source);
    }
    // Add extra sources from an iterator
    pub fn extra_sources_iter<I: IntoIterator<Item = PathBuf>>(&mut self, iter: I) {
        self.extra_sources.extend(iter);
    }

    pub fn extra_args_iter<I: IntoIterator<Item = String>>(&mut self, iter: I) {
        self.extra_args.extend(iter);
    }

    pub fn add_extra_args(&mut self, arg: String) {
        self.extra_args.push(arg);
    }

    pub fn add_extra_source_url(&mut self, source: String) {
        self.extra_sources_urls.push(source);
    }

    // Add extra sources from an iterator
    pub fn extra_sources_urls_iter<I: IntoIterator<Item = String>>(&mut self, iter: I) {
        self.extra_sources_urls.extend(iter);
    }

    pub async fn build(&self, manifest: &Path) -> Result<String> {
        // we parse the flatpak metadata file
        let flatpak_meta = FlatpakApplication::load_from_file(manifest.display().to_string())
            .map_err(color_eyre::Report::msg)?;

        // create the flatpak output folders
        let output_dir =
            env::current_dir()?.join(".flatpak-builder/build").join(&flatpak_meta.app_id);
        std::fs::create_dir_all(&output_dir)?;
        std::fs::create_dir_all(&self.output_repo)?;

        // build the flatpak
        let mut flatpak = Command::new("flatpak-builder");
        flatpak
            .arg(output_dir)
            .arg(manifest)
            .arg("--force-clean")
            .arg("--repo")
            .arg(self.output_repo.canonicalize().unwrap());

        // add extra sources

        for source in &self.extra_sources {
            flatpak.arg("--extra-sources").arg(source);
        }

        for source in &self.extra_sources_urls {
            flatpak.arg("--extra-sources-url").arg(source);
        }

        flatpak.args(&self.extra_args);

        // run the command
        flatpak.log().await?;
        Ok(flatpak_meta.app_id)
    }

    pub async fn bundle(&self, app_id: &str) -> Result<PathBuf> {
        std::fs::create_dir_all(&self.bundles_dir)?;
        let bundle_path = self.bundles_dir.join(format!("{app_id}.flatpak"));

        let mut flatpak = Command::new("flatpak");

        flatpak
            .arg("build-bundle")
            .arg(self.output_repo.canonicalize().unwrap())
            .arg(&bundle_path)
            .arg(app_id);

        flatpak.log().await?;

        Ok(bundle_path)
    }
}

#[cfg(test)]
mod test_super {}
