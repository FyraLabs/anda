use std::path::PathBuf;

use flatpak::{application::FlatpakApplication, format::FlatpakManifestFormat};

pub struct FlatpakBuilder {
    /// The output directory for the flatpak build
    output_dir: PathBuf,
    /// The output flatpak repository
    output_repo: PathBuf,
    /// Extra sources as paths
    extra_sources: Vec<PathBuf>,
    /// Extra sources as URLs
    extra_sources_urls: Vec<String>,
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
