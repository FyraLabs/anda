use color_eyre::Result;
use rhai::{CustomType, EvalAltResult};
use std::{
    fs,
    path::{Path, PathBuf},
};
use tracing::info;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RPMSpec {
    pub name: String,
    pub chkupdate: PathBuf,
    pub spec: PathBuf,
    pub f: String,
    pub changed: bool,
}

impl RPMSpec {
    pub fn new<T, U>(name: String, chkupdate: T, spec: U) -> Result<Self>
    where
        T: Into<PathBuf> + AsRef<Path>,
        U: Into<PathBuf> + AsRef<Path>,
    {
        Ok(Self {
            name,
            chkupdate: chkupdate.into(),
            changed: false,
            f: fs::read_to_string(&spec)?,
            spec: spec.into(),
        })
    }
    pub fn version(&mut self, ver: String) -> Result<(), Box<EvalAltResult>> {
        let re = regex::Regex::new(r"Version:(\s+)([\.\d]+)\n").unwrap();
        let m = re.captures(self.f.as_str());
        if m.is_none() {
            return Err("Can't find version preamble in spec".into());
        }
        let m = m.unwrap();
        if ver != m[2] {
            info!("{}: {} —→ {}", self.name, &m[2], ver);
            self.f = re
                .replace(&self.f, format!("Version:{}{ver}\n", &m[1]))
                .to_string();
            self.changed = true;
        }
        Ok(())
    }
    pub fn define(&mut self, name: &str, val: &str) -> Result<(), Box<EvalAltResult>> {
        let re = regex::Regex::new(r"(?m)%define(\s+)(\S+)(\s+)(\S+)$").unwrap();
        for cap in re.captures_iter(self.f.as_str()) {
            if &cap[2] == name {
                self.f = self.f.replace(&cap[0], format!("%define{}{name}{}{val}", &cap[1], &cap[3]).as_str());
                self.changed = true;
                return Ok(());
            }
        }
        Err(format!("Can't find `%define {name}` in spec").into())
    }
    pub fn global(&mut self, name: &str, val: &str) -> Result<(), Box<EvalAltResult>> {
        let re = regex::Regex::new(r"(?m)%global(\s+)(\S+)(\s+)(\S+)$").unwrap();
        for cap in re.captures_iter(self.f.as_str()) {
            if &cap[2] == name {
                self.f = self.f.replace(&cap[0], format!("%global{}{name}{}{val}", &cap[1], &cap[3]).as_str());
                self.changed = true;
                return Ok(());
            }
        }
        Err(format!("Can't find `%global {name}` in spec").into())
    }
    pub fn source(&mut self, i: i64, p: String) -> Result<(), Box<EvalAltResult>> {
        let re = regex::Regex::new(r"Source(\d+):(\s+)([^\n]+)\n").unwrap();
        let mut capw = None;
        for cap in re.captures_iter(self.f.as_str()) {
            if cap[1] != i.to_string() {
                continue;
            }
            info!("{}: Source{i}: {p}", self.name);
            capw = Some(cap);
        }
        if capw.is_none() {
            return Err("Can't find source preamble in spec".into());
        }
        let cap = capw.unwrap();
        self.f = self
            .f
            .replace(&cap[0], format!("Source{i}:{}{p}\n", &cap[2]).as_str());
        self.changed = true;
        Ok(())
    }
    pub fn write(self) -> std::io::Result<()> {
        if self.changed {
            fs::write(self.spec, self.f)?;
        }
        Ok(())
    }
    pub fn get(&mut self) -> String {
        self.f.clone()
    }
    pub fn set(&mut self, ff: String) {
        self.changed = true;
        self.f = ff;
    }
}

impl CustomType for RPMSpec {
    fn build(mut builder: rhai::TypeBuilder<'_, Self>) {
        builder
            .with_name("Rpm")
            .with_fn("version", Self::version)
            .with_fn("source", Self::source)
            .with_get_set("f", Self::get, Self::set);
    }
}
