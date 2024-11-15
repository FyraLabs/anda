//! RPM spec building backend for Andaman
//! This modules provides the RPM spec builder backend, which builds RPMs
//! from a spec file.

use clap::clap_derive::ValueEnum;
use tempfile::TempDir;

use crate::util::CommandLog;
use async_trait::async_trait;
use color_eyre::{eyre::eyre, Report, Result};
use std::mem::take;
use std::path::{Path, PathBuf};
use std::{collections::BTreeMap, str::FromStr};
use tokio::process::Command;
use tracing::{debug, info};

#[derive(Clone, Debug, Default)]
pub struct RPMOptions {
    /// Mock config, only used if backend is mock
    pub mock_config: Option<String>,
    /// With flags
    pub with: Vec<String>,
    /// Without flags
    pub without: Vec<String>,
    /// Build target, used for cross-compile
    pub target: Option<String>,
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
    /// Config options for Mock
    pub config_opts: Vec<String>,
    /// Enable SCM support
    pub scm_enable: bool,
    /// SCM Options (mock)
    pub scm_opts: Vec<String>,
    /// Plugin Options (mock)
    pub plugin_opts: Vec<String>,
}

impl RPMExtraOptions for RPMOptions {
    fn with_flags_mut(&mut self) -> &mut Vec<String> {
        &mut self.with
    }
    fn without_flags_mut(&mut self) -> &mut Vec<String> {
        &mut self.without
    }
    fn macros_mut(&mut self) -> &mut BTreeMap<String, String> {
        &mut self.macros
    }
    fn set_target(&mut self, target: Option<String>) {
        self.target = target;
    }
}

#[derive(ValueEnum, Debug, Clone, Copy)]
pub enum RPMBuilder {
    Mock,
    Rpmbuild,
}

impl FromStr for RPMBuilder {
    type Err = Report;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "mock" => Ok(Self::Mock),
            "rpmbuild" => Ok(Self::Rpmbuild),
            _ => Err(eyre!("Invalid RPM builder: {s}")),
        }
    }
}

impl From<crate::cli::RPMBuilder> for RPMBuilder {
    fn from(builder: crate::cli::RPMBuilder) -> Self {
        match builder {
            crate::cli::RPMBuilder::Mock => Self::Mock,
            crate::cli::RPMBuilder::Rpmbuild => Self::Rpmbuild,
        }
    }
}

impl RPMBuilder {
    /// Build the RPMs.
    ///
    /// # Errors
    /// This inherits errors from `RPMSpecBackend::build()`.
    pub async fn build(&self, spec: &Path, options: &mut RPMOptions) -> Result<Vec<PathBuf>> {
        // TODO: take ownership of `options`
        if matches!(self, Self::Mock) {
            let mut mock = MockBackend::new(
                take(&mut options.mock_config),
                take(&mut options.sources),
                take(&mut options.resultdir),
            );
            if let Some(extra_repos) = options.extra_repos.take() {
                for extra_repo in extra_repos {
                    mock.add_extra_repo(extra_repo);
                }
            }
            options.macros.iter().for_each(|(k, v)| {
                mock.def_macro(k, v);
            });
            mock.target(take(&mut options.target));
            mock.with_flags_mut().extend(take(&mut options.with));
            mock.without_flags_mut().extend(take(&mut options.without));
            mock.extend_config_opts(take(&mut options.config_opts));
            mock.no_mirror(options.no_mirror);
            mock.enable_scm(options.scm_enable);
            mock.extend_scm_opts(take(&mut options.scm_opts));
            mock.plugin_opts(take(&mut options.plugin_opts));

            mock.build(spec).await
        } else {
            let mut rpmbuild =
                RPMBuildBackend::new(take(&mut options.sources), take(&mut options.resultdir));

            options.macros.iter().for_each(|(k, v)| {
                rpmbuild.def_macro(k, v);
            });

            rpmbuild.set_target(take(&mut options.target));
            rpmbuild.with_flags_mut().extend(take(&mut options.with));
            rpmbuild.without_flags_mut().extend(take(&mut options.without));

            rpmbuild.build(spec).await
        }
    }
}

#[async_trait::async_trait]
pub trait RPMSpecBackend {
    async fn build_srpm(&self, spec: &Path) -> Result<PathBuf>;
    async fn build_rpm(&self, spec: &Path) -> Result<Vec<PathBuf>>;

    async fn build(&self, spec: &Path) -> Result<Vec<PathBuf>> {
        self.build_rpm(&self.build_srpm(spec).await?).await
    }
}

pub trait RPMExtraOptions {
    /// Returns macros as a mutable reference
    /// This is useful for advanced macro manipulation
    fn macros_mut(&mut self) -> &mut BTreeMap<String, String>;

    /// Set target, used for cross-compile
    fn set_target(&mut self, target: Option<String>);

    /// Defines a macro
    fn def_macro(&mut self, name: &str, value: &str) {
        self.macros_mut().insert(name.to_owned(), value.to_owned());
    }

    /// Returns a mutable reference to the `with` flags
    fn with_flags_mut(&mut self) -> &mut Vec<String>;

    /// Returns a mutable reference to the `without` flags
    fn without_flags_mut(&mut self) -> &mut Vec<String>;
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
    config_opts: Vec<String>,
    scm_enable: bool,
    scm_opts: Vec<String>,
    plugin_opts: Vec<String>,
    target: Option<String>,
}

impl RPMExtraOptions for MockBackend {
    fn with_flags_mut(&mut self) -> &mut Vec<String> {
        &mut self.with
    }
    fn without_flags_mut(&mut self) -> &mut Vec<String> {
        &mut self.without
    }
    fn macros_mut(&mut self) -> &mut BTreeMap<String, String> {
        &mut self.macros
    }
    fn set_target(&mut self, target: Option<String>) {
        self.target = target;
    }
}

impl MockBackend {
    pub const fn new(mock_config: Option<String>, sources: PathBuf, resultdir: PathBuf) -> Self {
        Self {
            mock_config,
            with: Vec::new(),
            without: Vec::new(),
            sources,
            resultdir,
            extra_repos: Vec::new(),
            no_mirror: false,
            macros: BTreeMap::new(),
            config_opts: Vec::new(),
            scm_enable: false,
            scm_opts: Vec::new(),
            plugin_opts: Vec::new(),
            target: None,
        }
    }

    pub fn extend_config_opts(&mut self, opts: Vec<String>) {
        self.config_opts.extend(opts);
    }

    pub fn add_extra_repo(&mut self, repo: String) {
        self.extra_repos.push(repo);
    }
    pub fn no_mirror(&mut self, no_mirror: bool) {
        self.no_mirror = no_mirror;
    }

    pub fn enable_scm(&mut self, enable: bool) {
        self.scm_enable = enable;
    }

    pub fn extend_scm_opts(&mut self, opts: Vec<String>) {
        self.scm_opts.extend(opts);
    }

    pub fn plugin_opts(&mut self, opts: Vec<String>) {
        self.plugin_opts.extend(opts);
    }

    pub fn target(&mut self, target: Option<String>) {
        self.target = target;
    }

    pub fn mock(&self) -> Command {
        let mut cmd = Command::new("mock");

        if let Some(config) = &self.mock_config {
            cmd.arg("-r").arg(config);
        }

        // cmd.arg("--verbose");

        if let Some(target) = &self.target {
            cmd.arg("--target").arg(target);
        }

        self.extra_repos.iter().for_each(|repo| {
            cmd.arg("-a").arg(repo);
        });

        self.with.iter().for_each(|with| {
            cmd.arg("--with").arg(with);
        });

        self.without.iter().for_each(|without| {
            cmd.arg("--without").arg(without);
        });

        self.macros.iter().for_each(|(name, value)| {
            cmd.arg("-D").arg(format!("'{name} {value}'"));
        });

        if self.no_mirror {
            cmd.arg("--config-opts").arg("mirrored=False");
        }

        self.config_opts.iter().for_each(|opt| {
            cmd.arg("--config-opts").arg(opt);
        });

        if self.scm_enable {
            cmd.arg("--scm-enable");
        }

        self.scm_opts.iter().for_each(|scm| {
            cmd.arg("--scm-option").arg(scm);
        });

        cmd
    }
}

#[async_trait]
impl RPMSpecBackend for MockBackend {
    async fn build_srpm(&self, spec: &Path) -> Result<PathBuf> {
        let mut cmd = self.mock();
        let tmp = tempfile::Builder::new().prefix("anda-srpm").tempdir()?;

        // todo: Probably copy the spec file and the sources to rpmbuild/SOURCES or some kind of temp dir instead
        // of building everything in the specfile's directory.

        cmd.arg("--buildsrpm")
            .arg("--spec")
            .arg(spec)
            .arg("--sources")
            .arg(&self.sources)
            .arg("--resultdir")
            .arg(tmp.path())
            .arg("--enable-network");

        // cmd.status()?;

        cmd.log().await?;

        // find srpm in resultdir using walkdir

        // let mut srpm = None;

        for entry in walkdir::WalkDir::new(tmp.path()) {
            let entry = entry?;
            debug!("entry: {:?}", entry.file_name());
            if entry.file_name().to_string_lossy().ends_with(".src.rpm") {
                // srpm = Some(entry.path().to_path_buf());
                // eprintln!("found srpm: {:?}", srpm);

                info!("Moving srpm to resultdir...");
                // create srpm dir if it doesnt exist
                let srpm_dir = self.resultdir.join("rpm/srpm");
                std::fs::create_dir_all(&srpm_dir)?;
                let dest = srpm_dir.join(entry.file_name());
                std::fs::copy(entry.path(), &dest)?;
                return Ok(dest);
            }
        }

        Err(eyre!("Failed to find srpm"))
    }
    async fn build_rpm(&self, spec: &Path) -> Result<Vec<PathBuf>> {
        let mut cmd = self.mock();
        let tmp = tempfile::Builder::new().prefix("anda-rpm").tempdir()?;
        cmd.arg("--rebuild").arg(spec).arg("--enable-network").arg("--resultdir").arg(tmp.path());

        cmd.log().await?;

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
    target: Option<String>,
    macros: BTreeMap<String, String>,
}

impl RPMExtraOptions for RPMBuildBackend {
    fn with_flags_mut(&mut self) -> &mut Vec<String> {
        &mut self.with
    }
    fn without_flags_mut(&mut self) -> &mut Vec<String> {
        &mut self.without
    }
    fn macros_mut(&mut self) -> &mut BTreeMap<String, String> {
        &mut self.macros
    }
    fn set_target(&mut self, target: Option<String>) {
        self.target = target;
    }
}

impl RPMBuildBackend {
    pub const fn new(sources: PathBuf, resultdir: PathBuf) -> Self {
        Self {
            sources,
            resultdir,
            with: Vec::new(),
            without: Vec::new(),
            macros: BTreeMap::new(),
            target: None,
        }
    }

    pub fn rpmbuild(&self) -> Command {
        let mut cmd = Command::new("rpmbuild");

        for with in &self.with {
            cmd.arg("--with").arg(with);
        }

        for without in &self.without {
            cmd.arg("--without").arg(without);
        }

        for (name, value) in &self.macros {
            cmd.arg("-D").arg(format!("'{name} {value}'"));
        }

        cmd
    }
}

#[async_trait]
impl RPMSpecBackend for RPMBuildBackend {
    async fn build_srpm(&self, spec: &Path) -> Result<PathBuf> {
        let mut cmd = self.rpmbuild();
        let tmp = tempfile::Builder::new().prefix("anda-srpm").tempdir()?;

        cmd.arg("-br")
            .arg(spec)
            .arg("--define")
            .arg(format!("_sourcedir {}", self.sources.canonicalize()?.display()))
            .arg("--define")
            .arg(format!("_srcrpmdir {}", tmp.path().display()));

        cmd.log().await?;

        // find srpm in resultdir using walkdir

        for entry in walkdir::WalkDir::new(tmp.path()) {
            let entry = entry?;
            debug!("entry: {:?}", entry.file_name());
            if entry.file_name().to_string_lossy().ends_with(".src.rpm") {
                // srpm = Some(entry.path().to_path_buf());
                // eprintln!("found srpm: {:?}", srpm);

                info!("Moving srpm to resultdir...");
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

    async fn build_rpm(&self, spec: &Path) -> Result<Vec<PathBuf>> {
        let mut cmd = self.rpmbuild();
        let tmp = tempfile::Builder::new().prefix("anda-rpm").tempdir()?;

        cmd.arg("-bb")
            .arg(spec)
            .arg("--define")
            .arg(format!("_sourcedir {}", self.sources.canonicalize()?.display()))
            .arg("--define")
            .arg(format!("_rpmdir {}", tmp.path().display()));

        cmd.log().await?;

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
                std::fs::copy(entry.path(), dest)?;
                rpms.push(rpms_dir.join(entry.file_name()));
            }
        }

        //println!("rpms: {:?}", rpms);
        Ok(rpms)
    }

    async fn build(&self, spec: &Path) -> Result<Vec<PathBuf>> {
        let mut cmd = self.rpmbuild();
        let tmp = TempDir::with_prefix("anda-rpmbuild")?;
        cmd.arg("-ba")
            .arg(spec)
            .arg("--define")
            .arg(format!("_sourcedir {}", self.sources.canonicalize()?.display()))
            .arg("--define")
            .arg(format!("_srcrpmdir {}", tmp.path().display()))
            .arg("--define")
            .arg(format!("_rpmdir {}", tmp.path().display()));
        cmd.log().await?;

        let mut rpms = Vec::new();

        // find rpms in resultdir using walkdir

        for entry in walkdir::WalkDir::new(tmp.path()) {
            let entry = entry?;
            let entry_filename = entry.file_name().to_string_lossy();

            let (subdir, is_rpm) = if entry_filename.ends_with(".src.rpm") {
                ("rpm/srpm", false)
            } else if entry_filename.ends_with(".rpm") {
                ("rpm/rpms", true)
            } else {
                continue;
            };

            let target_dir = self.resultdir.join(subdir);
            std::fs::create_dir_all(&target_dir)?;
            let dest = target_dir.join(entry.file_name());
            std::fs::copy(entry.path(), &dest)?;

            if is_rpm {
                rpms.push(dest);
            }
        }

        //println!("rpms: {:?}", rpms);
        Ok(rpms)
    }
}
