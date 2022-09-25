use anyhow::{anyhow, Result};
use std::{
    fmt::Display,
    path::{Path, PathBuf},
    process::Command,
};

use flatpak::{application::FlatpakApplication, format::FlatpakManifestFormat};

pub enum FlatpakArtifact {
    Ref(String),
    Bundle(PathBuf),
}

impl Display for FlatpakArtifact {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FlatpakArtifact::Ref(ref r) => write!(f, "ref {}", r),
            FlatpakArtifact::Bundle(ref b) => write!(f, "bundle {}", b.display()),
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
}

impl FlatpakBuilder {
    pub fn new(output_dir: PathBuf, output_repo: PathBuf, bundles_dir: PathBuf) -> Self {
        Self {
            output_dir,
            output_repo,
            bundles_dir,
            extra_sources: Vec::new(),
            extra_sources_urls: Vec::new(),
        }
    }

    pub fn add_extra_source(&mut self, source: PathBuf) {
        self.extra_sources.push(source);
    }
    // Add extra sources from an iterator
    pub fn extra_sources_iter<I: IntoIterator<Item = PathBuf>>(&mut self, iter: I) {
        self.extra_sources.extend(iter);
    }

    pub fn add_extra_source_url(&mut self, source: String) {
        self.extra_sources_urls.push(source);
    }

    // Add extra sources from an iterator
    pub fn extra_sources_urls_iter<I: IntoIterator<Item = String>>(&mut self, iter: I) {
        self.extra_sources_urls.extend(iter);
    }

    pub fn build(&self, manifest: &Path) -> Result<String> {
        // we parse the flatpak metadata file
        let flatpak_meta = FlatpakApplication::load_from_file(manifest.display().to_string())
            .map_err(|e| anyhow!(e))?;

        // create the flatpak output folders
        let output_dir = self.output_dir.join(&flatpak_meta.app_id);
        std::fs::create_dir_all(&output_dir).map_err(|e| anyhow!(e))?;
        std::fs::create_dir_all(&self.output_repo).map_err(|e| anyhow!(e))?;

        // build the flatpak
        let mut flatpak = Command::new("flatpak-builder");
        flatpak
            .arg(output_dir)
            .arg(manifest)
            .arg("--force-clean")
            .arg("--repo")
            .arg(&self.output_repo.canonicalize().unwrap());

        // add extra sources

        for source in &self.extra_sources {
            flatpak.arg("--extra-sources").arg(source);
        }

        for source in &self.extra_sources_urls {
            flatpak.arg("--extra-sources-url").arg(source);
        }

        // run the command
        flatpak.status().map_err(|e| anyhow!(e))?;
        Ok(flatpak_meta.app_id)
    }

    pub fn bundle(&self, app_id: &str) -> Result<PathBuf> {
        std::fs::create_dir_all(&self.bundles_dir).map_err(|e| anyhow!(e))?;
        let bundle_path = self.bundles_dir.join(format!("{}.flatpak", app_id));

        let mut flatpak = Command::new("flatpak");

        flatpak
            .arg("build-bundle")
            .arg(&self.output_repo.canonicalize().unwrap())
            .arg(&bundle_path)
            .arg(app_id);

        flatpak.status().map_err(|e| anyhow!(e))?;

        Ok(bundle_path)
    }
}

pub fn test_flatpak() {
    // test code

    // load file
    let manifest = r###"
    app-id: org.flatpak.Hello
    runtime: org.freedesktop.Platform
    runtime-version: '21.08'
    sdk: org.freedesktop.Sdk
    command: hello.sh
    modules:
      - name: hello
        buildsystem: simple
        build-commands:
          - install -D hello.sh /app/bin/hello.sh
        sources:
          - type: file
            path: hello.sh
    "###;

    println!("manifest: {:?}", manifest);
    let app = FlatpakApplication::parse(FlatpakManifestFormat::YAML, manifest);
    println!("app: {:#?}", app);
}

#[cfg(test)]
mod test_super {
    use super::*;

    #[test]
    fn a() {
        test_flatpak();
    }
}
