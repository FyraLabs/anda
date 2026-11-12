//! RPM spec building backend for Andaman
//! This modules provides the RPM spec builder backend, which builds RPMs
//! from a spec file.

#![allow(dead_code)]

use clap::clap_derive::ValueEnum;
use tempfile::TempDir;

use crate::process_group::{CancelOnDrop, InterruptWatch, TeardownFailed};
use crate::pt::PseudoTerminalCtl;
use crate::publish::{Artifact, ArtifactSet};
use crate::util::CommandLog;
use async_trait::async_trait;
use color_eyre::{eyre::eyre, Report, Result};
use std::mem::take;
use std::path::{Path, PathBuf};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::{collections::BTreeMap, str::FromStr};
use tokio::process::Command;
use tracing::{debug, info};

#[allow(clippy::struct_excessive_bools)]
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
    /// Extra Arguments to the respective builder
    pub args: Vec<String>,
    /// Build the SRPM and RPMs in one Mock process (experimental)
    pub mock_single_process: bool,
    /// Cleanup Options (mock)
    pub no_cleanup_before: bool,
    pub no_cleanup_after: bool,
    /// Disable networking (mock)
    pub no_network: bool,
}

impl RPMOptions {
    pub fn new(
        mock_config: Option<String>,
        sources: PathBuf,
        resultdir: PathBuf,
        args: Vec<String>,
    ) -> Self {
        Self { mock_config, sources, resultdir, args, ..Self::default() }
    }
    pub fn add_extra_repo(&mut self, repo: String) {
        if let Some(ref mut repos) = self.extra_repos {
            repos.push(repo);
        } else {
            self.extra_repos = Some(vec![repo]);
        }
    }

    pub const fn no_mirror(&mut self, no_mirror: bool) {
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
                take(&mut options.args),
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
            mock.enable_no_cleanup_before(options.no_cleanup_before);
            mock.enable_no_cleanup_after(options.no_cleanup_after);
            mock.enable_no_network(options.no_network);

            if options.mock_single_process {
                mock.build_single_process(spec).await
            } else {
                mock.build(spec).await
            }
        } else {
            let mut rpmbuild = RPMBuildBackend::new(
                take(&mut options.sources),
                take(&mut options.resultdir),
                take(&mut options.args),
            );

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
    /// Lists all macros
    fn macros(&self) -> BTreeMap<String, String>;
    /// Returns macros as a mutable reference
    /// This is useful for advanced macro manipulation
    fn macros_mut(&mut self) -> &mut BTreeMap<String, String>;

    /// Set target, used for cross-compile
    fn set_target(&mut self, target: Option<String>);

    /// Adds a list of macros from an iterator
    fn macros_iter<I>(&mut self, iter: I)
    where
        I: IntoIterator<Item = (String, String)>,
    {
        self.macros_mut().extend(iter);
    }

    /// Defines a macro
    fn def_macro(&mut self, name: &str, value: &str) {
        self.macros_mut().insert(name.to_owned(), value.to_owned());
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
#[allow(clippy::struct_excessive_bools)]
#[derive(Default)]
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
    args: Vec<String>,
    no_cleanup_before: bool,
    no_cleanup_after: bool,
    no_network: bool,
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
    fn set_target(&mut self, target: Option<String>) {
        self.target = target;
    }
}

impl MockBackend {
    pub fn new(
        mock_config: Option<String>,
        sources: PathBuf,
        resultdir: PathBuf,
        args: Vec<String>,
    ) -> Self {
        Self { mock_config, sources, resultdir, args, ..Self::default() }
    }
    pub fn extend_config_opts(&mut self, opts: Vec<String>) {
        self.config_opts.extend(opts);
    }

    pub fn add_config_opt(&mut self, opt: String) {
        self.config_opts.push(opt);
    }

    pub fn add_extra_repo(&mut self, repo: String) {
        self.extra_repos.push(repo);
    }
    pub const fn no_mirror(&mut self, no_mirror: bool) {
        self.no_mirror = no_mirror;
    }

    pub const fn enable_scm(&mut self, enable: bool) {
        self.scm_enable = enable;
    }

    pub fn extend_scm_opts(&mut self, opts: Vec<String>) {
        self.scm_opts.extend(opts);
    }

    pub fn add_scm_opt(&mut self, opt: String) {
        self.scm_opts.push(opt);
    }

    pub fn plugin_opts(&mut self, opts: Vec<String>) {
        self.plugin_opts.extend(opts);
    }

    pub const fn enable_no_cleanup_before(&mut self, enable: bool) {
        self.no_cleanup_before = enable;
    }

    pub const fn enable_no_cleanup_after(&mut self, enable: bool) {
        self.no_cleanup_after = enable;
    }

    pub const fn enable_no_network(&mut self, enable: bool) {
        self.no_network = enable;
    }

    pub fn target(&mut self, target: Option<String>) {
        self.target = target;
    }

    fn base_command(&self) -> Command {
        let mut cmd = Command::new("mock");
        if let Some(config) = &self.mock_config {
            cmd.arg("-r").arg(config);
        }
        if let Some(target) = &self.target {
            cmd.arg("--target").arg(target);
        }
        cmd
    }

    pub fn mock(&self) -> Command {
        let mut cmd = self.base_command();

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
            cmd.arg("-D").arg(format!("{name} {value}"));
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

        self.plugin_opts.iter().for_each(|option| {
            cmd.arg("--plugin-option").arg(option);
        });

        cmd.args(self.forwarded_args());

        if self.no_cleanup_before {
            cmd.arg("--no-clean");
        }

        if self.no_cleanup_after {
            cmd.arg("--no-cleanup-after");
        }

        if self.no_network {
            cmd.arg("--config-opts=rpmbuild_networking=False");
        } else {
            cmd.arg("--enable-network");
        }

        cmd
    }

    fn forwarded_args(&self) -> impl Iterator<Item = &String> {
        // Mock applies --enable-network after every config override, so it has to be dropped
        let enables_network =
            |arg: &&String| *arg == "--enable-network" || arg.starts_with("--enable-network=");
        self.args.iter().filter(move |arg| !(self.no_network && enables_network(arg)))
    }

    fn single_process_command(&self, spec: &Path, resultdir: &Path) -> Command {
        let mut cmd = self.mock();
        cmd.arg("--no-cleanup-after")
            .arg("--plugin-option=tmpfs:keep_mounted=True")
            .arg("--rebuild")
            .arg("--spec")
            .arg(spec)
            .arg("--sources")
            .arg(&self.sources)
            .arg("--resultdir")
            .arg(resultdir);
        cmd
    }

    fn cleanup_command(&self) -> Command {
        let mut cmd = self.base_command();
        for opt in &self.config_opts {
            cmd.arg("--config-opts").arg(opt);
        }
        for opt in &self.plugin_opts {
            cmd.arg("--plugin-option").arg(opt);
        }
        cmd.args(root_selector_args(&self.args)).arg("--clean");
        cmd
    }

    async fn build_single_process(&self, spec: &Path) -> Result<Vec<PathBuf>> {
        if self.scm_enable {
            return Err(eyre!("--mock-single-process is not supported with SCM builds yet"));
        }
        let workdir = tempfile::Builder::new().prefix("anda-mock").tempdir()?;
        CombinedMockRun {
            build: self.single_process_command(spec, workdir.path()),
            cleanup: (!self.no_cleanup_after).then(|| self.cleanup_command()),
            workdir,
            output: RpmResultDir(self.resultdir.clone()),
        }
        .run()
        .await
    }
}

const ROOT_SELECTOR_OPTIONS: &[&str] = &[
    "-r",
    "--root",
    "--rootdir",
    "--uniqueext",
    "--configdir",
    "--config-opts",
    "--plugin-option",
    "--enable-plugin",
    "--disable-plugin",
    "--isolation",
    "--arch",
    "--target",
    "--forcearch",
    "--buildroot-image",
];

const ROOT_SELECTOR_SWITCHES: &[&str] = &[
    "--old-chroot",
    "--new-chroot",
    "--bootstrap-chroot",
    "--no-bootstrap-chroot",
    "--use-bootstrap-image",
    "--no-bootstrap-image",
];

fn is_attached_short_root(arg: &str) -> bool {
    arg.starts_with("-r") && !arg.starts_with("--") && arg.len() > 2
}

fn root_selector_args(args: &[String]) -> Vec<&String> {
    let mut selected = Vec::new();
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        let (flag, has_inline_value) =
            arg.split_once('=').map_or((arg.as_str(), false), |(flag, _)| (flag, true));
        if ROOT_SELECTOR_OPTIONS.contains(&flag) {
            selected.push(arg);
            if !has_inline_value {
                selected.extend(args.next());
            }
        } else if ROOT_SELECTOR_SWITCHES.contains(&flag) || is_attached_short_root(arg) {
            selected.push(arg);
        }
    }
    selected
}

const CLEANUP_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(60);

struct CombinedMockRun {
    build: Command,
    cleanup: Option<Command>,
    workdir: TempDir,
    output: RpmResultDir,
}

impl CombinedMockRun {
    async fn run(self) -> Result<Vec<PathBuf>> {
        let cancelled = Arc::new(AtomicBool::new(false));
        let (mut guard, dropped) = CancelOnDrop::new(Arc::clone(&cancelled));
        let result = tokio::spawn(self.supervise(dropped, cancelled)).await?;
        guard.disarm();
        result
    }

    async fn supervise(
        mut self,
        dropped: tokio::sync::oneshot::Receiver<()>,
        cancelled: Arc<AtomicBool>,
    ) -> Result<Vec<PathBuf>> {
        let (_interrupt_watch, interrupted) = InterruptWatch::spawn(Arc::clone(&cancelled));
        let outcome = Self::execute(&mut self.build, async {
            tokio::select! {
                _ = dropped => {},
                _ = interrupted => {},
            }
        })
        .await;
        if outcome.as_ref().is_err_and(Report::is::<TeardownFailed>) {
            self.retain_workdir("build processes could not be stopped; root cleanup skipped");
            return outcome.map(|()| Vec::new());
        }
        self.clean_up(outcome.is_ok()).await?;
        outcome?;
        if cancelled.load(Ordering::Relaxed) {
            return Err(eyre!("Mock build cancelled"));
        }
        self.output.publish_from(self.workdir.path())
    }

    async fn clean_up(&mut self, build_succeeded: bool) -> Result<()> {
        let Some(cleanup) = &mut self.cleanup else { return Ok(()) };
        match Self::execute(cleanup, tokio::time::sleep(CLEANUP_TIMEOUT)).await {
            Err(error) if error.is::<TeardownFailed>() => {
                self.retain_workdir("cleanup processes could not be stopped");
                Err(error)
            }
            Err(error) if build_succeeded => Err(error),
            Err(error) => {
                tracing::warn!(%error, "Mock cleanup failed after build failure");
                Ok(())
            }
            Ok(()) => Ok(()),
        }
    }

    async fn execute<F>(cmd: &mut Command, cancellation: F) -> Result<()>
    where
        F: std::future::Future<Output = ()> + Send + 'static,
    {
        PseudoTerminalCtl::new(cmd.as_std().get_program())
            .run_with_cancellation(cmd, cancellation, false)
            .await
    }

    fn retain_workdir(&mut self, reason: &str) {
        self.workdir.disable_cleanup(true);
        tracing::error!(path = %self.workdir.path().display(), "Retaining Mock results: {reason}");
    }
}

struct RpmResultDir(PathBuf);

impl RpmResultDir {
    fn srpm_dir(&self) -> PathBuf {
        self.0.join("rpm/srpm")
    }

    fn rpm_dir(&self) -> PathBuf {
        self.0.join("rpm/rpms")
    }

    fn directory_for(&self, entry: &walkdir::DirEntry) -> Option<PathBuf> {
        let name = entry.file_name().to_string_lossy();
        if !entry.file_type().is_file() {
            None
        } else if name.ends_with(".src.rpm") {
            Some(self.srpm_dir())
        } else if name.ends_with(".rpm") {
            Some(self.rpm_dir())
        } else {
            None
        }
    }

    fn publish_from(&self, built: &Path) -> Result<Vec<PathBuf>> {
        let (srpm_dir, rpm_dir) = (self.srpm_dir(), self.rpm_dir());
        let mut artifacts = Vec::new();
        let mut destinations = std::collections::HashSet::new();
        let mut rpms = Vec::new();
        let mut found_srpm = false;
        for entry in walkdir::WalkDir::new(built) {
            let entry = entry?;
            let Some(directory) = self.directory_for(&entry) else { continue };
            let destination = directory.join(entry.file_name());
            if !destinations.insert(destination.clone()) {
                return Err(eyre!("Duplicate artifact destination: {}", destination.display()));
            }
            if directory == rpm_dir {
                rpms.push(destination.clone());
            } else {
                found_srpm = true;
            }
            artifacts.push(Artifact::new(entry.into_path(), destination));
        }
        if !found_srpm || rpms.is_empty() {
            return Err(eyre!("Mock did not produce both source and binary RPMs"));
        }
        std::fs::create_dir_all(&srpm_dir)?;
        std::fs::create_dir_all(&rpm_dir)?;
        ArtifactSet::new(artifacts).publish()?;
        Ok(rpms)
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
            .arg(tmp.path());

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
                Artifact::new(entry.path().to_path_buf(), dest.clone()).publish()?;
                return Ok(dest);
            }
        }

        Err(eyre!("Failed to find srpm"))
    }
    async fn build_rpm(&self, spec: &Path) -> Result<Vec<PathBuf>> {
        let mut cmd = self.mock();
        let tmp = tempfile::Builder::new().prefix("anda-rpm").tempdir()?;
        cmd.arg("--rebuild").arg(spec).arg("--resultdir").arg(tmp.path());

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
                Artifact::new(entry.path().to_path_buf(), dest.clone()).publish()?;
                rpms.push(dest);
            }
        }
        //println!("rpms: {:?}", rpms);
        Ok(rpms)
    }
}

#[cfg(test)]
mod mock_tests {
    use super::{CombinedMockRun, MockBackend, RPMExtraOptions, RpmResultDir};
    use std::{ffi::OsStr, path::Path};

    fn arguments(command: &tokio::process::Command) -> Vec<String> {
        command.as_std().get_args().map(|arg| arg.to_string_lossy().into_owned()).collect()
    }

    #[test]
    fn combined_build_keeps_clean_start_and_one_environment() {
        let backend = MockBackend::new(
            Some("terra-44-x86_64".into()),
            "sources with spaces".into(),
            "output".into(),
            vec![],
        );
        let cmd = backend.single_process_command(Path::new("package.spec"), Path::new("results"));
        assert_eq!(cmd.as_std().get_program(), OsStr::new("mock"));
        let args = arguments(&cmd);
        assert!(!args.contains(&"--no-clean".to_owned()));
        for required in [
            "--rebuild",
            "--spec",
            "package.spec",
            "--sources",
            "sources with spaces",
            "--no-cleanup-after",
            "--plugin-option=tmpfs:keep_mounted=True",
        ] {
            assert!(args.contains(&required.to_owned()), "Missing {required}");
        }
        assert!(!args.contains(&"--buildsrpm".to_owned()));
    }

    #[test]
    fn combined_build_preserves_custom_options_and_offline_build() {
        let mut backend = MockBackend::default();
        backend.enable_no_network(true);
        backend.enable_no_cleanup_before(true);
        backend.enable_no_cleanup_after(true);
        backend.def_macro("vendor", "Terra");
        backend.add_extra_repo("file:///packages".into());
        backend.plugin_opts(vec!["ccache:max_cache_size=10G".into()]);
        let args =
            arguments(&backend.single_process_command(Path::new("test.spec"), Path::new("out")));
        assert!(!args.contains(&"--enable-network".to_owned()));
        assert!(args.contains(&"--config-opts=rpmbuild_networking=False".to_owned()));
        assert!(args.contains(&"--no-clean".to_owned()));
        assert!(args.contains(&"vendor Terra".to_owned()));
        assert!(args.contains(&"file:///packages".to_owned()));
        assert!(args.contains(&"ccache:max_cache_size=10G".to_owned()));
    }

    #[test]
    fn legacy_command_also_honors_network_and_plugin_options() {
        let mut backend = MockBackend::default();
        backend.enable_no_network(true);
        backend.plugin_opts(vec!["tmpfs:max_fs_size=50%".into()]);
        let args = arguments(&backend.mock());
        assert!(!args.contains(&"--enable-network".to_owned()));
        assert!(args.contains(&"--config-opts=rpmbuild_networking=False".to_owned()));
        assert!(args.contains(&"tmpfs:max_fs_size=50%".to_owned()));
    }

    #[test]
    fn combined_results_preserve_source_and_binary_artifacts() {
        let input = tempfile::tempdir().unwrap();
        let output = tempfile::tempdir().unwrap();
        for name in ["test.src.rpm", "test.noarch.rpm", "test-debuginfo.x86_64.rpm", "state.log"] {
            std::fs::write(input.path().join(name), name).unwrap();
        }
        let rpms = RpmResultDir(output.path().into()).publish_from(input.path()).unwrap();
        assert_eq!(rpms.len(), 2);
        assert!(output.path().join("rpm/srpm/test.src.rpm").is_file());
        assert!(output.path().join("rpm/rpms/test.noarch.rpm").is_file());
        assert!(output.path().join("rpm/rpms/test-debuginfo.x86_64.rpm").is_file());
        assert!(!output.path().join("rpm/rpms/state.log").exists());
    }

    #[test]
    fn missing_binary_artifacts_are_an_error() {
        let input = tempfile::tempdir().unwrap();
        let output = tempfile::tempdir().unwrap();
        std::fs::write(input.path().join("test.src.rpm"), "source").unwrap();
        let old = output.path().join("rpm/srpm/test.src.rpm");
        std::fs::create_dir_all(old.parent().unwrap()).unwrap();
        std::fs::write(&old, "old source").unwrap();
        let error = RpmResultDir(output.path().into()).publish_from(input.path()).unwrap_err();
        assert!(error.to_string().contains("source and binary RPMs"));
        assert_eq!(std::fs::read_to_string(old).unwrap(), "old source");
        assert!(input.path().join("test.src.rpm").exists());
        assert!(!output.path().join("rpm/rpms").exists());
    }

    #[test]
    fn offline_precedence_filters_forwarded_enable_network_in_any_order() {
        for forwarded in [
            vec!["--enable-network", "--config-opts=rpmbuild_networking=True"],
            vec!["--config-opts=rpmbuild_networking=True", "--enable-network"],
        ] {
            let mut backend = MockBackend::new(
                None,
                "sources".into(),
                "out".into(),
                forwarded.into_iter().map(str::to_owned).collect(),
            );
            backend.enable_no_network(true);
            for cmd in [
                backend.mock(),
                backend.single_process_command(Path::new("test.spec"), Path::new("results")),
            ] {
                let args = arguments(&cmd);
                assert!(!args.iter().any(|arg| arg.starts_with("--enable-network")));
                let overrides: Vec<_> =
                    args.iter().filter(|arg| arg.contains("rpmbuild_networking=")).collect();
                assert_eq!(
                    overrides.last().unwrap().as_str(),
                    "--config-opts=rpmbuild_networking=False"
                );
            }
        }
    }

    #[test]
    fn cleanup_forwards_selectors_not_build_only_arguments() {
        let mut backend = MockBackend::new(
            Some("initial".into()),
            "src".into(),
            "out".into(),
            [
                "--additional-package",
                "gcc",
                "--with=feature",
                "-a",
                "file:///repo",
                "--configdir",
                "/configs",
                "--root=chosen",
                "--uniqueext",
                "ci-123",
                "--rootdir=/roots/chosen",
                "--enable-plugin=tmpfs",
                "--disable-plugin",
                "ccache",
                "--plugin-option=tmpfs:max_fs_size=50%",
                "--config-opts=basedir=/roots",
                "--isolation=simple",
                "--no-clean",
                "--no-cleanup-after",
            ]
            .into_iter()
            .map(str::to_owned)
            .collect(),
        );
        backend.plugin_opts(vec!["root_cache:dir=/cache".into()]);
        backend.add_config_opt("root=custom".into());
        let args = arguments(&backend.cleanup_command());
        for value in [
            "--clean",
            "initial",
            "root=custom",
            "--root=chosen",
            "--configdir",
            "/configs",
            "--uniqueext",
            "ci-123",
            "--rootdir=/roots/chosen",
            "--enable-plugin=tmpfs",
            "--disable-plugin",
            "ccache",
            "--plugin-option=tmpfs:max_fs_size=50%",
            "--config-opts=basedir=/roots",
            "root_cache:dir=/cache",
            "--isolation=simple",
        ] {
            assert!(args.contains(&value.to_owned()), "Missing {value}");
        }
        for value in [
            "--additional-package",
            "gcc",
            "--with=feature",
            "-a",
            "file:///repo",
            "--no-clean",
            "--no-cleanup-after",
            "--enable-network",
        ] {
            assert!(!args.contains(&value.to_owned()), "Unexpected {value}");
        }
    }

    #[test]
    fn staging_failure_does_not_replace_earlier_output() {
        let input = tempfile::tempdir().unwrap();
        let output = tempfile::tempdir().unwrap();
        for name in ["test.src.rpm", "test.noarch.rpm"] {
            std::fs::write(input.path().join(name), "new").unwrap();
        }
        std::fs::create_dir_all(output.path().join("rpm/srpm")).unwrap();
        std::fs::create_dir_all(output.path().join("rpm/rpms/test.noarch.rpm")).unwrap();
        let old = output.path().join("rpm/srpm/test.src.rpm");
        std::fs::write(&old, "old").unwrap();
        let error = RpmResultDir(output.path().into()).publish_from(input.path()).unwrap_err();
        assert!(error.to_string().contains("not a regular file"));
        assert_eq!(std::fs::read_to_string(old).unwrap(), "old");
        assert!(input.path().join("test.src.rpm").exists());
    }

    fn python(script: &str, paths: &[&Path]) -> tokio::process::Command {
        let mut command = tokio::process::Command::new("python3");
        command.arg("-c").arg(script).args(paths);
        command
    }

    fn combined_run(
        build: tokio::process::Command,
        cleanup: tokio::process::Command,
        workdir: tempfile::TempDir,
        output: &Path,
    ) -> CombinedMockRun {
        CombinedMockRun {
            build,
            cleanup: Some(cleanup),
            workdir,
            output: RpmResultDir(output.to_path_buf()),
        }
    }

    async fn wait_for(path: &Path) {
        tokio::time::timeout(std::time::Duration::from_secs(8), async {
            while !path.exists() {
                tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            }
        })
        .await
        .expect("synthetic tool did not finish in time");
    }

    #[tokio::test]
    async fn cancelled_build_is_reaped_before_cleanup_and_tempdir_drop() {
        let control = tempfile::tempdir().unwrap();
        let output = tempfile::tempdir().unwrap();
        let tmp = tempfile::tempdir().unwrap();
        let results = tmp.path().to_path_buf();
        let ready = control.path().join("ready");
        let stopped = control.path().join("stopped");
        let cleaned = control.path().join("cleaned");
        let build = python(
            "
import os, signal, sys, time
from pathlib import Path
ready, stopped, results = map(Path, sys.argv[1:])
def stop(*_):
    assert results.is_dir()
    stopped.write_text('stopped')
    sys.exit(0)
signal.signal(signal.SIGINT, stop)
ready.write_text(str(os.getpid()))
while True: time.sleep(.05)
",
            &[&ready, &stopped, &results],
        );
        let cleanup = python(
            "
import sys
from pathlib import Path
ready, stopped, results, cleaned = map(Path, sys.argv[1:])
assert stopped.is_file()
assert not Path('/proc/' + ready.read_text()).exists()
assert results.is_dir()
cleaned.write_text('cleaned')
",
            &[&ready, &stopped, &results, &cleaned],
        );
        let run = combined_run(build, cleanup, tmp, output.path());
        let caller = tokio::spawn(run.run());
        wait_for(&ready).await;
        caller.abort();
        assert!(caller.await.unwrap_err().is_cancelled());
        wait_for(&cleaned).await;
        tokio::time::timeout(std::time::Duration::from_secs(2), async {
            while results.exists() {
                tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            }
        })
        .await
        .unwrap();
        assert!(!output.path().join("rpm").exists());
    }

    #[tokio::test]
    async fn failed_build_still_cleans_and_returns_original_error() {
        let control = tempfile::tempdir().unwrap();
        let tmp = tempfile::tempdir().unwrap();
        let cleaned = control.path().join("cleaned");
        let build = python("import sys; sys.exit(23)", &[]);
        let cleanup = python("import sys; from pathlib import Path; Path(sys.argv[1]).write_text('cleaned'); sys.exit(24)", &[&cleaned]);
        let result = tokio::time::timeout(
            std::time::Duration::from_secs(8),
            combined_run(build, cleanup, tmp, Path::new("out")).run(),
        )
        .await
        .unwrap();
        assert!(result.unwrap_err().to_string().contains("23"));
        assert!(cleaned.exists());
    }

    #[tokio::test]
    async fn cancellation_during_cleanup_does_not_publish_results() {
        let control = tempfile::tempdir().unwrap();
        let output = tempfile::tempdir().unwrap();
        let tmp = tempfile::tempdir().unwrap();
        let ready = control.path().join("clean-ready");
        let cleaned = control.path().join("cleaned");
        let build = python("import sys; from pathlib import Path; p=Path(sys.argv[1]); (p/'test.src.rpm').write_text('src'); (p/'test.rpm').write_text('rpm')", &[tmp.path()]);
        let cleanup = python("import sys, time; from pathlib import Path; Path(sys.argv[1]).write_text('ready'); time.sleep(.3); Path(sys.argv[2]).write_text('done')", &[&ready, &cleaned]);
        let run = combined_run(build, cleanup, tmp, output.path());
        let caller = tokio::spawn(run.run());
        wait_for(&ready).await;
        caller.abort();
        assert!(caller.await.unwrap_err().is_cancelled());
        wait_for(&cleaned).await;
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        assert!(!output.path().join("rpm").exists());
    }

    #[tokio::test]
    async fn combined_scm_is_rejected_before_spawning_mock() {
        let mut backend = MockBackend::default();
        backend.enable_scm(true);
        let error = backend.build_single_process(Path::new("test.spec")).await.unwrap_err();
        assert!(error.to_string().contains("SCM"));
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
#[derive(Default)]
pub struct RPMBuildBackend {
    sources: PathBuf,
    resultdir: PathBuf,
    with: Vec<String>,
    without: Vec<String>,
    target: Option<String>,
    macros: BTreeMap<String, String>,
    args: Vec<String>,
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
    fn set_target(&mut self, target: Option<String>) {
        self.target = target;
    }
}

impl RPMBuildBackend {
    pub fn new(sources: PathBuf, resultdir: PathBuf, args: Vec<String>) -> Self {
        Self { sources, resultdir, args, ..Self::default() }
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
            cmd.arg("-D").arg(format!("{name} {value}"));
        }

        cmd.args(&self.args);

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
                Artifact::new(entry.path().to_path_buf(), dest.clone()).publish()?;
                return Ok(dest);
            }
        }

        Err(eyre!("rpmbuild did not produce a source RPM"))
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
                Artifact::new(entry.path().to_path_buf(), dest.clone()).publish()?;
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
            .arg(format!("_rpmdir {}", tmp.path().display()))
            .args(self.target.as_ref().map(|target| format!("--target={target}")));
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
            Artifact::new(entry.path().to_path_buf(), dest.clone()).publish()?;

            if is_rpm {
                rpms.push(dest);
            }
        }

        //println!("rpms: {:?}", rpms);
        Ok(rpms)
    }
}
