//! RPM spec building backend for Andaman
//! This modules provides the RPM spec builder backend, which builds RPMs
//! from a spec file.
use clap::clap_derive::ArgEnum;
use tempfile::TempDir;


use anyhow::{anyhow, Result};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::str::FromStr;

pub struct RPMOptions {
    /// Mock config, only used if backend is mock
    pub mock_config: Option<String>,
    /// With flags
    pub with: Vec<String>,
    /// Without flags
    pub without: Vec<String>,
    /// Path to sources
    pub sources: PathBuf,
    /// Output directory
    pub resultdir: PathBuf,
    /// Extra repos
    /// Only used if backend is mock
    pub extra_repos: Option<Vec<String>>,
    /// Do not use mirrors
    /// Only used if backend is mock
    pub no_mirror: bool,
    /// Custom RPM macros to define
    pub macros: BTreeMap<String, String>,
}

impl RPMOptions {
    pub fn new(mock_config: Option<String>, sources: PathBuf, resultdir: PathBuf) -> Self {
        Self {
            mock_config,
            with: Vec::new(),
            without: Vec::new(),
            sources,
            resultdir,
            extra_repos: None,
            no_mirror: false,
            macros: BTreeMap::new(),
        }
    }
    pub fn add_extra_repo(&mut self, repo: String) {
        if let Some(ref mut repos) = self.extra_repos {
            repos.push(repo);
        } else {
            self.extra_repos = Some(vec![repo]);
        }
    }

    pub fn no_mirror(&mut self, no_mirror: bool) {
        self.no_mirror = no_mirror;
    }
}

impl RPMExtraOptions for RPMOptions {
    fn with_flags(&self) -> Vec<String> {
        self.with.clone()
    }
    fn with_flags_mut(&mut self) -> &mut Vec<String> {
        &mut self.with
    }
    fn without_flags(&self) -> Vec<String> {
        self.without.clone()
    }
    fn without_flags_mut(&mut self) -> &mut Vec<String> {
        &mut self.without
    }
    fn macros(&self) -> BTreeMap<String, String> {
        self.macros.clone()
    }
    fn macros_mut(&mut self) -> &mut BTreeMap<String, String> {
        &mut self.macros
    }
}

#[derive(ArgEnum, Debug, Clone, Copy)]
pub enum RPMBuilder {
    Mock,
    Rpmbuild,
}

impl FromStr for RPMBuilder {
    type Err = anyhow::Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "mock" => Ok(RPMBuilder::Mock),
            "rpmbuild" => Ok(RPMBuilder::Rpmbuild),
            _ => Err(anyhow!("Invalid RPM builder: {}", s)),
        }
    }
}

impl RPMBuilder {
    pub fn build(&self, spec: &Path, options: &RPMOptions) -> Result<Vec<PathBuf>> {
        match self {
            RPMBuilder::Mock => {
                let mock = MockBackend::new(
                    options.mock_config.clone(),
                    options.sources.clone(),
                    options.resultdir.clone(),
                );

                mock.build(spec)
            }
            RPMBuilder::Rpmbuild => {
                let rpmbuild =
                    RPMBuildBackend::new(options.sources.clone(), options.resultdir.clone());

                rpmbuild.build(spec)
            }
        }
    }
}

pub trait RPMSpecBackend {
    fn build_srpm(&self, spec: &Path) -> Result<PathBuf>;
    fn build_rpm(&self, spec: &Path) -> Result<Vec<PathBuf>>;

    fn build(&self, spec: &Path) -> Result<Vec<PathBuf>> {
        self.build_rpm(&self.build_srpm(spec)?)
    }
}

pub trait RPMExtraOptions {
    /// Lists all macros
    fn macros(&self) -> BTreeMap<String, String>;
    /// Returns macros as a mutable reference
    /// This is useful for advanced macro manipulation
    fn macros_mut(&mut self) -> &mut BTreeMap<String, String>;

    /// Adds a list of macros from an iterator
    fn macros_iter<I>(&mut self, iter: I)
    where
        I: IntoIterator<Item = (String, String)>,
    {
        self.macros_mut().extend(iter);
    }

    /// Defines a macro
    fn def_macro(&mut self, name: &str, value: &str) {
        self.macros_mut()
            .insert(name.to_string(), value.to_string());
    }
    /// Undefines a macro
    fn undef_macro(&mut self, name: &str) {
        self.macros_mut().remove(name);
    }

    // Configuration flags
    // === with flags ===
    /// Returns a list of `with` flags
    fn with_flags(&self) -> Vec<String>;

    /// Returns a mutable reference to the `with` flags
    fn with_flags_mut(&mut self) -> &mut Vec<String>;

    /// Sets a `with` flag for the build from an iterator
    fn with_flags_iter<I>(&mut self, iter: I)
    where
        I: IntoIterator<Item = String>,
    {
        self.with_flags_mut().extend(iter);
    }

    // === without flags ===
    /// Returns a list of `without` flags
    fn without_flags(&self) -> Vec<String>;

    /// Returns a mutable reference to the `without` flags
    fn without_flags_mut(&mut self) -> &mut Vec<String>;

    /// Sets a `without` flag for the build from an iterator
    fn without_flags_iter<I>(&mut self, iter: I)
    where
        I: IntoIterator<Item = String>,
    {
        self.without_flags_mut().extend(iter);
    }
}

/// An RPM spec backend that uses Mock to build RPMs
pub struct MockBackend {
    mock_config: Option<String>,
    with: Vec<String>,
    without: Vec<String>,
    sources: PathBuf,
    resultdir: PathBuf,
    extra_repos: Vec<String>,
    no_mirror: bool,
    macros: BTreeMap<String, String>,
}

impl RPMExtraOptions for MockBackend {
    fn with_flags(&self) -> Vec<String> {
        self.with.clone()
    }
    fn with_flags_mut(&mut self) -> &mut Vec<String> {
        &mut self.with
    }
    fn without_flags(&self) -> Vec<String> {
        self.without.clone()
    }
    fn without_flags_mut(&mut self) -> &mut Vec<String> {
        &mut self.without
    }
    fn macros(&self) -> BTreeMap<String, String> {
        self.macros.clone()
    }
    fn macros_mut(&mut self) -> &mut BTreeMap<String, String> {
        &mut self.macros
    }
}

impl MockBackend {
    pub fn new(mock_config: Option<String>, sources: PathBuf, resultdir: PathBuf) -> Self {
        Self {
            mock_config,
            with: Vec::new(),
            without: Vec::new(),
            sources,
            resultdir,
            extra_repos: Vec::new(),
            no_mirror: false,
            macros: BTreeMap::new(),
        }
    }

    pub fn add_extra_repo(&mut self, repo: String) {
        self.extra_repos.push(repo);
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

        for with in self.with.iter() {
            cmd.arg("--with").arg(with);
        }

        for without in self.without.iter() {
            cmd.arg("--without").arg(without);
        }

        for (name, value) in self.macros.iter() {
            cmd.arg("-D").arg(format!("{} {}", name, value));
        }

        if self.no_mirror {
            cmd.arg("--config-opts").arg("mirrored=False");
        }
        cmd
    }
}

impl RPMSpecBackend for MockBackend {
    fn build_srpm(&self, spec: &Path) -> Result<PathBuf> {
        let mut cmd = self.mock();
        let tmp = TempDir::new()?;

        cmd.arg("--buildsrpm")
            .arg("--spec")
            .arg(spec)
            .arg("--sources")
            .arg(&self.sources)
            .arg("--resultdir")
            .arg(tmp.path());

        cmd.status()?;

        // find srpm in resultdir using walkdir

        // let mut srpm = None;

        for entry in walkdir::WalkDir::new(tmp.path()) {
            let entry = entry?;
            eprintln!("entry: {:?}", entry.file_name());
            if entry.file_name().to_string_lossy().ends_with(".src.rpm") {
                // srpm = Some(entry.path().to_path_buf());
                // eprintln!("found srpm: {:?}", srpm);

                println!("Moving srpm to resultdir...");
                // create srpm dir if it doesnt exist
                let srpm_dir = self.resultdir.join("rpm/srpm");
                std::fs::create_dir_all(&srpm_dir)?;
                let dest = srpm_dir.join(entry.file_name());
                std::fs::copy(entry.path(), &dest)?;
                return Ok(dest);
            }
        }

        Err(anyhow!("Failed to find srpm"))
    }
    fn build_rpm(&self, spec: &Path) -> Result<Vec<PathBuf>> {
        let mut cmd = self.mock();
        let tmp = TempDir::new()?;
        cmd.arg("--rebuild")
            .arg(spec)
            .arg("--resultdir")
            .arg(tmp.path());

        cmd.status()?;

        // find rpms in resultdir using walkdir

        let mut rpms = Vec::new();

        for entry in walkdir::WalkDir::new(tmp.path()) {
            let entry = entry?;
            //eprintln!("entry: {:?}", entry.file_name());

            if entry.file_name().to_string_lossy().ends_with(".src.rpm") {
            } else if entry.file_name().to_string_lossy().ends_with(".rpm") {
                //rpms.push(entry.path().to_path_buf());
                //eprintln!("found rpm: {:?}", rpms);

                let rpms_dir = self.resultdir.join("rpm/rpms");
                std::fs::create_dir_all(&rpms_dir)?;
                let dest = rpms_dir.join(entry.file_name());
                std::fs::copy(entry.path(), &dest)?;
                rpms.push(dest);
            }
        }
        //println!("rpms: {:?}", rpms);
        Ok(rpms)
    }
}

/// Pure rpmbuild backend for building inside host
///
/// This is faster than mock due to not having to spin up a chroot, but
/// it requires the host to have all the dependencies instead.
/// It is also useful when building in unprivileged containers, as mock requires some
/// privileges to run a chroot.
///
/// This backend is not recommended when building distros, as all changes will not
/// be reflected for every package.
pub struct RPMBuildBackend {
    sources: PathBuf,
    resultdir: PathBuf,
    with: Vec<String>,
    without: Vec<String>,
    macros: BTreeMap<String, String>,
}

impl RPMExtraOptions for RPMBuildBackend {
    fn with_flags(&self) -> Vec<String> {
        self.with.clone()
    }
    fn with_flags_mut(&mut self) -> &mut Vec<String> {
        &mut self.with
    }
    fn without_flags(&self) -> Vec<String> {
        self.without.clone()
    }
    fn without_flags_mut(&mut self) -> &mut Vec<String> {
        &mut self.without
    }
    fn macros(&self) -> BTreeMap<String, String> {
        self.macros.clone()
    }
    fn macros_mut(&mut self) -> &mut BTreeMap<String, String> {
        &mut self.macros
    }
}

impl RPMBuildBackend {
    pub fn new(sources: PathBuf, resultdir: PathBuf) -> Self {
        Self {
            sources,
            resultdir,
            with: Vec::new(),
            without: Vec::new(),
            macros: BTreeMap::new(),
        }
    }

    pub fn rpmbuild(&self) -> Command {
        let mut cmd = Command::new("rpmbuild");

        for with in self.with.iter() {
            cmd.arg("--with").arg(with);
        }

        for without in self.without.iter() {
            cmd.arg("--without").arg(without);
        }

        for (name, value) in self.macros.iter() {
            cmd.arg("-D").arg(format!("{} {}", name, value));
        }

        cmd
    }
}

impl RPMSpecBackend for RPMBuildBackend {
    fn build_srpm(&self, spec: &Path) -> Result<PathBuf> {
        let mut cmd = self.rpmbuild();
        let tmp = TempDir::new()?;

        cmd.arg("-br")
            .arg(spec)
            .arg("--define")
            .arg(format!("_sourcedir {}", self.sources.display()))
            .arg("--define")
            .arg(format!("_srcrpmdir {}", tmp.path().display()));

        cmd.status()?;

        // find srpm in resultdir using walkdir

        for entry in walkdir::WalkDir::new(tmp.path()) {
            let entry = entry?;
            eprintln!("entry: {:?}", entry.file_name());
            if entry.file_name().to_string_lossy().ends_with(".src.rpm") {
                // srpm = Some(entry.path().to_path_buf());
                // eprintln!("found srpm: {:?}", srpm);

                println!("Moving srpm to resultdir...");
                // create srpm dir if it doesnt exist
                let srpm_dir = self.resultdir.join("rpm/srpm");
                std::fs::create_dir_all(&srpm_dir)?;
                let dest = srpm_dir.join(entry.file_name());
                std::fs::copy(entry.path(), &dest)?;
                return Ok(dest);
            }
        }

        todo!()
    }

    fn build_rpm(&self, spec: &Path) -> Result<Vec<PathBuf>> {
        let mut cmd = self.rpmbuild();
        let tmp = TempDir::new()?;

        cmd.arg("-bb")
            .arg(spec)
            .arg("--define")
            .arg(format!("_sourcedir {}", self.sources.display()))
            .arg("--define")
            .arg(format!("_rpmdir {}", tmp.path().display()));

        cmd.status()?;

        let mut rpms = Vec::new();

        // find rpms in resultdir using walkdir

        for entry in walkdir::WalkDir::new(tmp.path()) {
            let entry = entry?;
            //eprintln!("entry: {:?}", entry.file_name());
            if entry.file_name().to_string_lossy().ends_with(".rpm") {
                //rpms.push(entry.path().to_path_buf());
                // eprintln!("found rpm: {:?}", rpms);

                let rpms_dir = self.resultdir.join("rpm/rpms");
                std::fs::create_dir_all(&rpms_dir)?;
                let dest = rpms_dir.join(entry.file_name());
                std::fs::copy(entry.path(), &dest)?;
                rpms.push(rpms_dir.join(entry.file_name()));
            }
        }

        //println!("rpms: {:?}", rpms);
        Ok(rpms)
    }

    fn build(&self, spec: &Path) -> Result<Vec<PathBuf>> {
        let mut cmd = self.rpmbuild();
        let tmp = TempDir::new()?;
        cmd.arg("-ba")
            .arg(spec)
            .arg("--define")
            .arg(format!("_sourcedir {}", self.sources.display()))
            .arg("--define")
            .arg(format!("_srcrpmdir {}", tmp.path().display()))
            .arg("--define")
            .arg(format!("_rpmdir {}", tmp.path().display()));
        cmd.status()?;

        let mut rpms = Vec::new();

        // find rpms in resultdir using walkdir

        for entry in walkdir::WalkDir::new(tmp.path()) {
            let entry = entry?;
            //eprintln!("entry: {:?}", entry.file_name());

            if entry.file_name().to_string_lossy().ends_with(".src.rpm") {
                //rpms.push(entry.path().to_path_buf());
                eprintln!("found srpm: {:?}", rpms);

                let srpm_dir = self.resultdir.join("rpm/srpm");
                std::fs::create_dir_all(&srpm_dir)?;
                let dest = srpm_dir.join(entry.file_name());
                std::fs::copy(entry.path(), &dest)?;
                //rpms.push(srpm_dir.join(entry.file_name()));
            } else if entry.file_name().to_string_lossy().ends_with(".rpm") {
                //rpms.push(entry.path().to_path_buf());
                // eprintln!("found rpm: {:?}", rpms);

                let rpms_dir = self.resultdir.join("rpm/rpms");
                std::fs::create_dir_all(&rpms_dir)?;
                let dest = rpms_dir.join(entry.file_name());
                std::fs::copy(entry.path(), &dest)?;
                rpms.push(rpms_dir.join(entry.file_name()));
            }
        }

        //println!("rpms: {:?}", rpms);
        Ok(rpms)
    }
}
