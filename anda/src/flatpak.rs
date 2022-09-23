use std::path::PathBuf;

use flatpak::{application::FlatpakApplication, format::FlatpakManifestFormat};


pub struct FlatpakBuilder {
    output_dir: PathBuf,
}

pub fn build_flatpak() {
    let app = FlatpakApplication::parse(FlatpakManifestFormat::YAML, "");

}
