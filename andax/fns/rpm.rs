use rhai::CustomType;
use std::{
    fs,
    path::{Path, PathBuf},
};
use tracing::{error, info};

lazy_static::lazy_static! {
    static ref RE_RELEASE: regex::Regex = regex::Regex::new(r"Release:(\s+)(.+?)\n").unwrap();
    static ref RE_VERSION: regex::Regex = regex::Regex::new(r"Version:(\s+)(\S+)\n").unwrap();
    static ref RE_DEFINE: regex::Regex = regex::Regex::new(r"(?m)%define(\s+)(\S+)(\s+)([^\n]+?)$").unwrap();
    static ref RE_GLOBAL: regex::Regex = regex::Regex::new(r"(?m)%global(\s+)(\S+)(\s+)([^\n]+?)$").unwrap();
    static ref RE_SOURCE: regex::Regex = regex::Regex::new(r"Source(\d+):(\s+)([^\n]+)\n").unwrap();
}

/// Update RPM spec files
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RPMSpec {
    /// Original spec file content
    original: String,
    /// Name of project
    pub name: String,
    /// AndaX chkupdate script of project
    pub chkupdate: PathBuf,
    /// Path to spec file
    pub spec: PathBuf,
    /// RPM spec file content
    pub f: String,
}

impl RPMSpec {
    /// Creates a new RPMSpec file representation
    ///
    /// # Panics
    /// - spec file does not exist / cannot read spec to string
    pub fn new<T, U>(name: String, chkupdate: T, spec: U) -> Self
    where
        T: Into<PathBuf> + AsRef<Path>,
        U: Into<PathBuf> + AsRef<Path>,
    {
        let f = fs::read_to_string(&spec).expect("Cannot read spec to string");
        Self { name, chkupdate: chkupdate.into(), original: f.clone(), f, spec: spec.into() }
    }
    /// Resets the release number to 1
    pub fn reset_release(&mut self) {
        self.release("1");
    }
    /// Sets the release number in the spec file
    pub fn release(&mut self, rel: &str) {
        let rel = rel.trim();
        let m = RE_RELEASE.captures(self.f.as_str());
        let Some(m) = m else { return error!("No `Release:` preamble for {}", self.name) };
        self.f = RE_RELEASE.replace(&self.f, format!("Release:{}{rel}%?dist\n", &m[1])).to_string();
    }
    /// Sets the version in the spec file
    pub fn version(&mut self, ver: &str) {
        let ver = ver.trim();
        let Some(m) = RE_VERSION.captures(self.f.as_str()) else {
            return error!("No `Version:` preamble for {}", self.name);
        };
        let ver = ver.strip_prefix('v').unwrap_or(ver).replace('-', ".");
        if ver != m[2] {
            info!("{}: {} —→ {ver}", self.name, &m[2]);
            self.f = RE_VERSION.replace(&self.f, format!("Version:{}{ver}\n", &m[1])).to_string();
            self.reset_release();
        }
    }
    /// Change the value of a `%define` macro by the name
    pub fn define(&mut self, name: &str, val: &str) {
        let (name, val) = (name.trim(), val.trim());
        let Some(cap) = RE_DEFINE.captures_iter(self.f.as_str()).find(|cap| &cap[2] == name) else {
            return error!("Cannot find `%define` for {}", self.name);
        };
        self.f = self.f.replace(&cap[0], &format!("%define{}{name}{}{val}", &cap[1], &cap[3]));
    }
    /// Change the value of a `%global` macro by the name
    pub fn global(&mut self, name: &str, val: &str) {
        let (name, val) = (name.trim(), val.trim());
        let Some(cap) = RE_GLOBAL.captures_iter(self.f.as_str()).find(|cap| &cap[2] == name) else {
            return error!("Cannot find `%global` for {}", self.name);
        };
        self.f = self.f.replace(&cap[0], &format!("%global{}{name}{}{val}", &cap[1], &cap[3]));
    }
    /// Change the `SourceN:` preamble value by `N`
    pub fn source(&mut self, i: i64, p: &str) {
        let p = p.trim();
        let si = i.to_string();
        let Some(cap) = RE_SOURCE.captures_iter(self.f.as_str()).find(|cap| cap[1] == si) else {
            return error!("No `Source{i}:` preamble for {}", self.name);
        };
        info!("{}: Source{i}: {p}", self.name);
        self.f = self.f.replace(&cap[0], &format!("Source{i}:{}{p}\n", &cap[2]));
    }
    /// Write the updated spec file content
    ///
    /// # Errors
    /// - happens only if the writing part failed :3
    pub fn write(mut self) -> std::io::Result<()> {
        if self.changed() {
            fs::write(self.spec, self.f)?;
        }
        Ok(())
    }
    /// Get the spec file content
    pub fn get(&mut self) -> String {
        self.f.clone()
    }
    /// Override the spec file content manually
    pub fn set(&mut self, ff: String) {
        self.f = ff;
    }
    /// Check if file has been changed
    #[must_use]
    pub fn changed(&mut self) -> bool {
        self.f != self.original
    }
}

impl CustomType for RPMSpec {
    fn build(mut builder: rhai::TypeBuilder<'_, Self>) {
        builder
            .with_name("Rpm")
            .with_fn("version", Self::version)
            .with_fn("source", Self::source)
            .with_fn("define", Self::define)
            .with_fn("global", Self::global)
            .with_fn("release", Self::reset_release)
            .with_fn("release", Self::release)
            .with_fn("changed", Self::changed)
            .with_get_set("f", Self::get, Self::set);
    }
}
