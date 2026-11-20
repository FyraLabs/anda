use rhai::CustomType;
use std::{
    fs,
    hash::{Hash, Hasher},
    path::{Path, PathBuf},
    sync::LazyLock,
};
use tracing::{error, info, warn};

use super::submodules::{global_name, Submodule};

static RE_RELEASE: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r"Release:([\t ]+)(.+?)\n").unwrap());
static RE_VERSION: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r"Version:([\t ]+)(\S+)\n").unwrap());
static RE_DEFINE: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r"(?m)%define([\t ]+)(\S+)([\t ]+)([^\n]+?)$").unwrap());
static RE_GLOBAL: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r"(?m)%global([\t ]+)(\S+)([\t ]+)([^\n]+?)$").unwrap());
static RE_SOURCE: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r"Source(\d+):([\t ]+)([^\n]+)\n").unwrap());

/// Update RPM spec files
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RPMSpec {
    /// Original spec file content
    original: u64,
    /// Name of project
    pub name: String,
    /// AndaX chkupdate script of project
    pub chkupdate: PathBuf,
    /// Path to spec file
    pub spec: PathBuf,
    /// RPM spec file content
    pub f: String,
    pub submodule_changes: Vec<SubmoduleChange>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SubmoduleChange {
    Added { path: String, url: String, commit: String, global: String },
    Dropped { global: String, commit: String },
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
        let mut hasher = std::hash::DefaultHasher::default();
        f.hash(&mut hasher);
        Self {
            name,
            chkupdate: chkupdate.into(),
            original: hasher.finish(),
            f,
            spec: spec.into(),
            submodule_changes: vec![],
        }
    }
    /// Resets the release number to 1
    pub fn reset_release(&mut self) {
        self.release("1");
    }
    /// Sets the release number in the spec file
    pub fn release_num(&mut self, rel: i64) {
        if !RE_RELEASE.is_match(&self.f) {
            return error!("No `Release:` preamble for {}", self.name);
        }
        match RE_RELEASE.replace(&self.f, format!("Release:${{1}}{rel}%{{?dist}}\n")) {
            std::borrow::Cow::Borrowed(_) => {
                return info!("{}: Release: {rel}%{{?dist}} [UNCHANGED]", self.name)
            }
            std::borrow::Cow::Owned(f) => self.f = f,
        }
        info!("{}: Release: {rel}%{{?dist}}", self.name);
    }
    /// Sets the release number in the spec file
    pub fn release(&mut self, rel: &str) {
        if !RE_RELEASE.is_match(&self.f) {
            return error!("No `Release:` preamble for {}", self.name);
        }
        let rel = rel.trim();
        match RE_RELEASE.replace(&self.f, format!("Release:${{1}}{rel}%{{?dist}}\n")) {
            std::borrow::Cow::Borrowed(_) => {
                return info!("{}: Release: {rel} [UNCHANGED]", self.name)
            }
            std::borrow::Cow::Owned(f) => self.f = f,
        }
        info!("{}: Release: {rel}", self.name);
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
    /// Read the value of a `%global` macro by the name.
    pub fn global_value(&mut self, name: &str) -> String {
        let name = name.trim();
        RE_GLOBAL
            .captures_iter(self.f.as_str())
            .find(|cap| &cap[2] == name)
            .map_or_else(String::new, |cap| cap[4].trim().to_owned())
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
    fn track_submodules(&mut self, upstream: &[Submodule], ignore: &[String]) {
        let tracked: Vec<(String, String)> = RE_GLOBAL
            .captures_iter(&self.f)
            .filter(|cap| cap[2].starts_with("submodule_"))
            .map(|cap| (cap[2].to_owned(), cap[4].trim().to_owned()))
            .collect();

        for submodule in upstream {
            let global = global_name(&submodule.path);
            match tracked.iter().find(|(name, _)| *name == global) {
                Some((_, commit)) if *commit == submodule.commit => {}
                Some((_, commit)) => {
                    info!("{}: {}: {commit} —→ {}", self.name, submodule.path, submodule.commit);
                    self.global(&global, &submodule.commit);
                }
                None if ignore.contains(&submodule.path) => {}
                None => {
                    warn!(
                        "{}: upstream added submodule {} ({}), add `%global {global}` and a source for it, or ignore it",
                        self.name, submodule.path, submodule.url
                    );
                    self.submodule_changes.push(SubmoduleChange::Added {
                        path: submodule.path.clone(),
                        url: submodule.url.clone(),
                        commit: submodule.commit.clone(),
                        global,
                    });
                }
            }
        }

        for (global, commit) in tracked {
            if !upstream.iter().any(|submodule| global_name(&submodule.path) == global) {
                warn!("{}: `%global {global}` no longer matches an upstream submodule", self.name);
                self.submodule_changes.push(SubmoduleChange::Dropped { global, commit });
            }
        }
    }

    fn submodules_with(
        &mut self,
        upstream: &[rhai::Dynamic],
        options: &rhai::Map,
    ) -> Result<(), Box<rhai::EvalAltResult>> {
        let upstream = upstream
            .iter()
            .map(|entry| {
                let map = entry
                    .read_lock::<rhai::Map>()
                    .ok_or("rpm.submodules() takes an array of submodule maps")?;
                Submodule::from_map(&map).map_err(|e| e.to_string().into())
            })
            .collect::<Result<Vec<_>, Box<rhai::EvalAltResult>>>()?;
        let ignore = match options.get("ignore") {
            None => vec![],
            Some(list) => list
                .clone()
                .into_typed_array::<rhai::ImmutableString>()
                .map_err(|_| "rpm.submodules(): `ignore` must be an array of paths")?
                .into_iter()
                .map(|path| path.to_string())
                .collect(),
        };
        self.track_submodules(&upstream, &ignore);
        Ok(())
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
        let mut hasher = std::hash::DefaultHasher::default();
        self.f.hash(&mut hasher);
        hasher.finish() != self.original
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
            .with_fn("global", Self::global_value)
            .with_fn("global_value", Self::global_value)
            .with_fn("release", Self::reset_release)
            .with_fn("release", Self::release)
            .with_fn("release", Self::release_num)
            .with_fn("changed", Self::changed)
            .with_fn("submodules", |rpm: &mut Self, upstream: rhai::Array| {
                rpm.submodules_with(&upstream, &rhai::Map::new())
            })
            .with_fn("submodules", |rpm: &mut Self, upstream: rhai::Array, options: rhai::Map| {
                rpm.submodules_with(&upstream, &options)
            })
            .with_get_set("f", Self::get, Self::set);
    }
}

#[cfg(test)]
mod tests {
    use super::{RPMSpec, Submodule, SubmoduleChange};

    fn submodule(path: &str, commit: &str) -> Submodule {
        Submodule {
            path: path.into(),
            url: format!("https://example.com/{path}"),
            commit: commit.into(),
        }
    }

    #[test]
    fn tracks_upstream_submodules_against_spec_globals() {
        let dir = tempfile::tempdir().expect("create temporary directory");
        let spec = dir.path().join("test.spec");
        std::fs::write(
            &spec,
            "%global submodule_lib_a aaa\n%global submodule_lib_b bbb\n%global submodule_gone ggg\n%global commit_date 20260101\nName: test\n",
        )
        .expect("write spec");
        let mut rpm = RPMSpec::new("test".to_owned(), dir.path().join("update.rhai"), &spec);

        rpm.track_submodules(
            &[
                submodule("lib/a", "aaa2"),
                submodule("lib/b", "bbb"),
                submodule("lib/new", "nnn"),
                submodule("lib/win-only", "www"),
            ],
            &["lib/win-only".to_owned()],
        );

        assert!(rpm.f.contains("%global submodule_lib_a aaa2\n"));
        assert!(rpm.f.contains("%global submodule_lib_b bbb\n"));
        assert_eq!(
            rpm.submodule_changes,
            vec![
                SubmoduleChange::Added {
                    path: "lib/new".into(),
                    url: "https://example.com/lib/new".into(),
                    commit: "nnn".into(),
                    global: "submodule_lib_new".into(),
                },
                SubmoduleChange::Dropped { global: "submodule_gone".into(), commit: "ggg".into() },
            ]
        );
    }
}
