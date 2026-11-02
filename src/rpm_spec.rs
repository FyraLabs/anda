//! RPM spec building backend for Andaman
//! This modules provides the RPM spec builder backend, which builds RPMs
//! from a spec file.

#![allow(dead_code)]

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

/// Read the label that a destination-copy would preserve. Unknown metadata
/// must not silently enable the inode-preserving fast path.
fn file_label(path: &Path) -> std::io::Result<Option<Vec<u8>>> {
    use std::os::unix::ffi::OsStrExt;
    let path = std::ffi::CString::new(path.as_os_str().as_bytes())?;
    let mut label = vec![0u8; 4096];
    // SAFETY: valid NUL-terminated path/name and a writable buffer of this size.
    let size = unsafe {
        nix::libc::getxattr(
            path.as_ptr(),
            c"security.selinux".as_ptr(),
            label.as_mut_ptr().cast(),
            label.len(),
        )
    };
    if size >= 0 {
        label.truncate(usize::try_from(size).map_err(std::io::Error::other)?);
        Ok(Some(label))
    } else {
        let error = std::io::Error::last_os_error();
        if matches!(error.raw_os_error(), Some(nix::libc::ENODATA | nix::libc::ENOTSUP)) {
            Ok(None)
        } else {
            Err(error)
        }
    }
}

fn stage_artifact(source: &Path, destination: &Path) -> Result<tempfile::NamedTempFile> {
    use std::os::{fd::AsRawFd, unix::fs::MetadataExt};
    let parent = destination.parent().ok_or_else(|| eyre!("Invalid artifact destination"))?;
    let staged = tempfile::NamedTempFile::new_in(parent)?;
    let old = match std::fs::symlink_metadata(destination) {
        Ok(metadata) if metadata.file_type().is_file() => Some(metadata),
        Ok(_) => {
            return Err(eyre!(
                "Artifact destination is not a regular file: {}",
                destination.display()
            ))
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => return Err(error.into()),
    };
    let expected = old
        .as_ref()
        .map_or_else(|| staged.as_file().metadata(), |metadata| Ok(metadata.clone()))?;
    let expected_label = file_label(if old.is_some() { destination } else { staged.path() })?;
    let source_metadata = std::fs::metadata(source)?;
    if !source_metadata.is_file() {
        return Err(eyre!("Artifact source is not a regular file"));
    }
    // Hard-link staging leaves the original in place until the whole set is
    // published. This retains the no-byte-copy benefit and makes rollback safe.
    if source_metadata.uid() == expected.uid()
        && source_metadata.gid() == expected.gid()
        && file_label(source)? == expected_label
    {
        let link = tempfile::NamedTempFile::new_in(parent)?;
        std::fs::remove_file(link.path())?;
        match std::fs::hard_link(source, link.path()) {
            Ok(()) => return Ok(link),
            Err(error)
                if matches!(
                    error.raw_os_error(),
                    Some(nix::libc::EXDEV | nix::libc::EPERM | nix::libc::EOPNOTSUPP)
                ) => {}
            Err(error) => return Err(error.into()),
        }
    }
    std::fs::copy(source, staged.path())?;
    let metadata = staged.as_file().metadata()?;
    if metadata.uid() != expected.uid() || metadata.gid() != expected.gid() {
        // SAFETY: owned open descriptor and UID/GID from destination metadata.
        if unsafe {
            nix::libc::fchown(staged.as_file().as_raw_fd(), expected.uid(), expected.gid())
        } != 0
        {
            return Err(std::io::Error::last_os_error().into());
        }
        // chown can clear set-id bits; restore std::fs::copy's permissions.
        std::fs::set_permissions(staged.path(), source_metadata.permissions())?;
    }
    if file_label(staged.path())? != expected_label {
        if let Some(label) = expected_label {
            // SAFETY: owned descriptor, valid attribute name and buffer/size.
            if unsafe {
                nix::libc::fsetxattr(
                    staged.as_file().as_raw_fd(),
                    c"security.selinux".as_ptr(),
                    label.as_ptr().cast(),
                    label.len(),
                    0,
                )
            } != 0
            {
                return Err(std::io::Error::last_os_error().into());
            }
        } else {
            // SAFETY: owned descriptor and valid attribute name.
            if unsafe {
                nix::libc::fremovexattr(staged.as_file().as_raw_fd(), c"security.selinux".as_ptr())
            } != 0
            {
                return Err(std::io::Error::last_os_error().into());
            }
        }
    }
    Ok(staged)
}

fn publish_artifacts_with<F>(artifacts: &[(PathBuf, PathBuf)], mut publish: F) -> Result<()>
where
    F: FnMut(&Path, &Path) -> std::io::Result<()>,
{
    // Prepare the full set before changing any public output. Backups are hard
    // links, so all old destination metadata and bytes survive a later failure.
    let mut prepared = Vec::with_capacity(artifacts.len());
    for (source, destination) in artifacts {
        let staged = stage_artifact(source, destination)?;
        let backup = if destination.exists() {
            let parent =
                destination.parent().ok_or_else(|| eyre!("Invalid artifact destination"))?;
            let backup = tempfile::NamedTempFile::new_in(parent)?;
            std::fs::remove_file(backup.path())?;
            std::fs::hard_link(destination, backup.path())?;
            Some(backup)
        } else {
            None
        };
        prepared.push((staged, backup));
    }
    for (index, ((_, destination), (staged, _))) in artifacts.iter().zip(&prepared).enumerate() {
        if let Err(error) = publish(staged.path(), destination) {
            let mut rollback_errors = Vec::new();
            for ((_, destination), (_, backup)) in artifacts.iter().zip(&prepared).take(index).rev()
            {
                let rollback = backup.as_ref().map_or_else(
                    || std::fs::remove_file(destination),
                    |backup| std::fs::rename(backup.path(), destination),
                );
                if let Err(error) = rollback {
                    rollback_errors.push(format!("{}: {error}", destination.display()));
                }
            }
            if !rollback_errors.is_empty() {
                // Keep backups for manual recovery rather than unlinking them.
                retain_backups(prepared.into_iter().filter_map(|(_, backup)| backup));
                return Err(eyre!(
                    "{error}; artifact rollback failed: {}",
                    rollback_errors.join(", ")
                ));
            }
            return Err(error.into());
        }
    }
    for (source, _) in artifacts {
        // Publication already succeeded; a leftover temporary source must not
        // turn success into failure after overwriting the user's output.
        if let Err(error) = std::fs::remove_file(source) {
            tracing::warn!(%error, path = %source.display(), "Cannot remove published artifact source");
        }
    }
    Ok(())
}

fn retain_backups(backups: impl Iterator<Item = tempfile::NamedTempFile>) {
    for backup in backups {
        match backup.keep() {
            Ok((_, path)) => {
                tracing::error!(path = %path.display(), "Artifact backup retained for recovery");
            }
            Err(error) => tracing::error!(%error, "Cannot retain artifact backup"),
        }
    }
}

fn move_artifact(source: &Path, destination: &Path) -> Result<()> {
    publish_artifacts_with(&[(source.to_path_buf(), destination.to_path_buf())], |from, to| {
        std::fs::rename(from, to)
    })
}

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

        // Mock applies --enable-network after config overrides, irrespective
        // of their order. --no-network takes precedence over forwarded flags.
        cmd.args(self.args.iter().filter(|arg| {
            !self.no_network
                || !(arg.as_str() == "--enable-network" || arg.starts_with("--enable-network="))
        }));

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

    fn single_process_command(&self, spec: &Path, resultdir: &Path) -> Command {
        let mut cmd = self.mock();
        // Mock's combined mode internally runs buildsrpm, then rebuild without
        // cleaning in between. Prevent buildsrpm's success cleanup from deleting
        // that environment. Explicit --no-clean/--no-cleanup-after are preserved.
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
        let mut cmd = Command::new("mock");
        if let Some(config) = &self.mock_config {
            cmd.arg("-r").arg(config);
        }
        if let Some(target) = &self.target {
            cmd.arg("--target").arg(target);
        }
        // Config overrides may determine the root through templates as well as
        // root/basedir/rootdir directly. They are valid for --clean too.
        for opt in &self.config_opts {
            cmd.arg("--config-opts").arg(opt);
        }
        for opt in &self.plugin_opts {
            cmd.arg("--plugin-option").arg(opt);
        }
        // Do not replay build-only -A options such as --additional-package,
        // --with, or --rebuild. Only forward root/config/plugin selectors.
        let mut args = self.args.iter();
        while let Some(arg) = args.next() {
            let flag = arg.split('=').next().unwrap_or(arg);
            if matches!(
                flag,
                "-r" | "--root"
                    | "--rootdir"
                    | "--uniqueext"
                    | "--configdir"
                    | "--config-opts"
                    | "--plugin-option"
                    | "--enable-plugin"
                    | "--disable-plugin"
                    | "--isolation"
                    | "--arch"
                    | "--target"
                    | "--forcearch"
                    | "--buildroot-image"
            ) {
                cmd.arg(arg);
                if !arg.contains('=') {
                    cmd.args(args.next());
                }
            } else if matches!(
                flag,
                "--old-chroot"
                    | "--new-chroot"
                    | "--bootstrap-chroot"
                    | "--no-bootstrap-chroot"
                    | "--use-bootstrap-image"
                    | "--no-bootstrap-image"
            ) || (arg.starts_with("-r") && !arg.starts_with("--") && arg.len() > 2)
            {
                cmd.arg(arg);
            }
        }
        cmd.arg("--clean");
        cmd
    }

    async fn build_single_process(&self, spec: &Path) -> Result<Vec<PathBuf>> {
        // SCM's combined mode prepares its own sources; retain its existing path.
        if self.scm_enable {
            return Err(eyre!("--mock-single-process is not supported with SCM builds yet"));
        }
        let tmp = tempfile::Builder::new().prefix("anda-mock").tempdir()?;
        self.run_single_process(
            self.single_process_command(spec, tmp.path()),
            (!self.no_cleanup_after).then(|| self.cleanup_command()),
            tmp,
        )
        .await
    }

    async fn run_single_process(
        &self,
        mut build: Command,
        cleanup: Option<Command>,
        tmp: TempDir,
    ) -> Result<Vec<PathBuf>> {
        let (sender, receiver) = tokio::sync::oneshot::channel();
        let cancelled = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let mut guard = crate::pt::CancelOnDrop {
            sender: Some(sender),
            cancelled: std::sync::Arc::clone(&cancelled),
        };
        let resultdir = self.resultdir.clone();
        let (interrupt_sender, interrupt_receiver) = tokio::sync::oneshot::channel();
        // Observe Ctrl-C for the *whole* lifecycle, including cleanup. The clean
        // command itself must finish, but cancellation must suppress publication.
        let interrupt_flag = std::sync::Arc::clone(&cancelled);
        let interrupt = crate::pt::AbortOnDrop(tokio::spawn(async move {
            if tokio::signal::ctrl_c().await.is_ok() {
                interrupt_flag.store(true, std::sync::atomic::Ordering::Relaxed);
                interrupt_sender.send(()).ok();
            }
        }));
        // This task owns the entire lifecycle and the temporary artifacts. A
        // dropped caller cannot skip cleanup or delete results under live Mock.
        let lifecycle = tokio::spawn(async move {
            let _interrupt = interrupt;
            let outcome = crate::pt::PseudoTerminalCtl::new(build.as_std().get_program())
                .run_with_cancellation(
                    &mut build,
                    async {
                        tokio::select! {
                            _ = receiver => {},
                            _ = interrupt_receiver => {},
                        }
                    },
                    false,
                )
                .await;
            if outcome.as_ref().is_err_and(Report::is::<crate::pt::TeardownFailed>) {
                let retained = tmp.keep();
                tracing::error!(path = %retained.display(), "Retaining Mock results: build processes could not be stopped; root cleanup skipped");
                return outcome.map(|()| Vec::new());
            }
            if let Some(mut cleanup) = cleanup {
                // A second Ctrl-C must not race teardown. Still bound the clean
                // attempt, then terminate/reap it before releasing the tempdir.
                let clean = crate::pt::PseudoTerminalCtl::new(cleanup.as_std().get_program())
                    .run_with_cancellation(
                        &mut cleanup,
                        async {
                            tokio::time::sleep(std::time::Duration::from_secs(60)).await;
                        },
                        false,
                    )
                    .await;
                match clean {
                    Err(error) if error.is::<crate::pt::TeardownFailed>() => {
                        let retained = tmp.keep();
                        tracing::error!(path = %retained.display(), "Retaining Mock results: cleanup processes could not be stopped");
                        return Err(error);
                    }
                    Err(error) if outcome.is_ok() => return Err(error),
                    Err(error) => tracing::warn!(%error, "Mock cleanup failed after build failure"),
                    Ok(()) => {}
                }
            }
            outcome?;
            if cancelled.load(std::sync::atomic::Ordering::Relaxed) {
                return Err(eyre!("Mock build cancelled"));
            }
            Self { resultdir, ..Self::default() }.collect_results(tmp.path())
        });
        let result = lifecycle.await?;
        guard.sender.take();
        result
    }

    fn collect_results(&self, resultdir: &Path) -> Result<Vec<PathBuf>> {
        let mut rpms = Vec::new();
        let mut found_srpm = false;
        let source_dir = self.resultdir.join("rpm/srpm");
        let binary_dir = self.resultdir.join("rpm/rpms");
        let mut artifacts = Vec::new();
        let mut destinations = std::collections::HashSet::new();
        for entry in walkdir::WalkDir::new(resultdir) {
            let entry = entry?;
            if !entry.file_type().is_file() {
                continue;
            }
            let name = entry.file_name().to_string_lossy();
            let directory = if name.ends_with(".src.rpm") {
                found_srpm = true;
                &source_dir
            } else if name.ends_with(".rpm") {
                &binary_dir
            } else {
                continue;
            };
            let destination = directory.join(entry.file_name());
            if !destinations.insert(destination.clone()) {
                return Err(eyre!("Duplicate artifact destination: {}", destination.display()));
            }
            artifacts.push((entry.path().to_path_buf(), destination.clone()));
            if directory == &binary_dir {
                rpms.push(destination);
            }
        }
        if !found_srpm || rpms.is_empty() {
            return Err(eyre!("Mock did not produce both source and binary RPMs"));
        }
        std::fs::create_dir_all(&source_dir)?;
        std::fs::create_dir_all(&binary_dir)?;
        publish_artifacts_with(&artifacts, |from, to| std::fs::rename(from, to))?;
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
                move_artifact(entry.path(), &dest)?;
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
                move_artifact(entry.path(), &dest)?;
                rpms.push(dest);
            }
        }
        //println!("rpms: {:?}", rpms);
        Ok(rpms)
    }
}

#[cfg(test)]
mod mock_tests {
    use super::{move_artifact, publish_artifacts_with, MockBackend, RPMExtraOptions};
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
        let backend = MockBackend::new(None, "sources".into(), output.path().into(), vec![]);
        let rpms = backend.collect_results(input.path()).unwrap();
        assert_eq!(rpms.len(), 2);
        assert!(output.path().join("rpm/srpm/test.src.rpm").is_file());
        assert!(output.path().join("rpm/rpms/test.noarch.rpm").is_file());
        assert!(output.path().join("rpm/rpms/test-debuginfo.x86_64.rpm").is_file());
        assert!(!output.path().join("rpm/rpms/state.log").exists());
    }

    #[test]
    fn artifact_move_replaces_old_output_without_copying() {
        use std::os::unix::fs::MetadataExt;
        let directory = tempfile::tempdir().unwrap();
        let source = directory.path().join("source.rpm");
        let destination = directory.path().join("output.rpm");
        std::fs::write(&source, "new artifact").unwrap();
        std::fs::write(&destination, "old artifact").unwrap();
        let inode = std::fs::metadata(&source).unwrap().ino();
        move_artifact(&source, &destination).unwrap();
        assert!(!source.exists());
        assert_eq!(std::fs::metadata(&destination).unwrap().ino(), inode);
        assert_eq!(std::fs::read_to_string(destination).unwrap(), "new artifact");
    }

    #[test]
    fn missing_binary_artifacts_are_an_error() {
        let input = tempfile::tempdir().unwrap();
        let output = tempfile::tempdir().unwrap();
        std::fs::write(input.path().join("test.src.rpm"), "source").unwrap();
        let old = output.path().join("rpm/srpm/test.src.rpm");
        std::fs::create_dir_all(old.parent().unwrap()).unwrap();
        std::fs::write(&old, "old source").unwrap();
        let backend = MockBackend::new(None, "sources".into(), output.path().into(), vec![]);
        let error = backend.collect_results(input.path()).unwrap_err();
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
    fn later_publication_failure_rolls_back_the_whole_set() {
        let input = tempfile::tempdir().unwrap();
        let output = tempfile::tempdir().unwrap();
        let artifacts: Vec<_> = ["test.src.rpm", "test.noarch.rpm", "extra.rpm"]
            .into_iter()
            .map(|name| {
                let source = input.path().join(name);
                let destination = output.path().join(name);
                std::fs::write(&source, "new").unwrap();
                std::fs::write(&destination, "old").unwrap();
                (source, destination)
            })
            .collect();
        let mut calls = 0;
        let result = publish_artifacts_with(&artifacts, |source, destination| {
            calls += 1;
            if calls == 3 {
                Err(std::io::Error::from_raw_os_error(nix::libc::EIO))
            } else {
                std::fs::rename(source, destination)
            }
        });
        assert!(result.is_err());
        for (source, destination) in artifacts {
            assert_eq!(std::fs::read_to_string(destination).unwrap(), "old");
            assert_eq!(std::fs::read_to_string(source).unwrap(), "new");
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
        let backend = MockBackend::new(None, "sources".into(), output.path().into(), vec![]);
        let error = backend.collect_results(input.path()).unwrap_err();
        assert!(error.to_string().contains("not a regular file"));
        assert_eq!(std::fs::read_to_string(old).unwrap(), "old");
        assert!(input.path().join("test.src.rpm").exists());
    }

    #[test]
    fn copy_fallback_preserves_destination_owner_and_group() {
        use std::os::{
            fd::AsRawFd,
            unix::fs::{MetadataExt, PermissionsExt},
        };
        let directory = tempfile::tempdir().unwrap();
        let source = directory.path().join("source.rpm");
        let destination = directory.path().join("output.rpm");
        let file = std::fs::File::create(&source).unwrap();
        // Pick another supplementary group when available to exercise the copy
        // path as an ordinary user; the metadata-compatible path is tested above.
        // SAFETY: first call asks for count, second writes into an allocated array.
        let count = unsafe { nix::libc::getgroups(0, std::ptr::null_mut()) };
        let mut groups = vec![0; usize::try_from(count).unwrap()];
        // SAFETY: buffer is allocated for count gids.
        assert_eq!(unsafe { nix::libc::getgroups(count, groups.as_mut_ptr()) }, count);
        let original_gid = file.metadata().unwrap().gid();
        let group = groups.into_iter().find(|group| *group != original_gid);
        let Some(group) = group else {
            return;
        };
        // SAFETY: valid fd, -1 keeps uid, group is a supplementary group.
        assert_eq!(unsafe { nix::libc::fchown(file.as_raw_fd(), u32::MAX, group) }, 0);
        std::fs::write(&source, "new").unwrap();
        std::fs::set_permissions(&source, std::fs::Permissions::from_mode(0o644)).unwrap();
        std::fs::write(&destination, "old").unwrap();
        let old = std::fs::metadata(&destination).unwrap();
        let source_inode = file.metadata().unwrap().ino();
        move_artifact(&source, &destination).unwrap();
        let new = std::fs::metadata(destination).unwrap();
        assert_eq!((new.uid(), new.gid()), (old.uid(), old.gid()));
        assert_ne!(new.ino(), source_inode);
    }

    #[test]
    fn copy_fallback_inherits_setgid_output_group() {
        use std::os::{
            fd::AsRawFd,
            unix::fs::{MetadataExt, PermissionsExt},
        };
        let directory = tempfile::tempdir().unwrap();
        let input = tempfile::tempdir().unwrap();
        let parent = std::fs::File::open(directory.path()).unwrap();
        // SAFETY: query count, then fill a buffer allocated for that many gids.
        let count = unsafe { nix::libc::getgroups(0, std::ptr::null_mut()) };
        let mut groups = vec![0; usize::try_from(count).unwrap()];
        // SAFETY: writable buffer holds count gids.
        assert_eq!(unsafe { nix::libc::getgroups(count, groups.as_mut_ptr()) }, count);
        let gid = parent.metadata().unwrap().gid();
        let Some(group) = groups.into_iter().find(|group| *group != gid) else {
            return;
        };
        // SAFETY: valid directory fd, -1 preserves uid, group is supplementary.
        assert_eq!(unsafe { nix::libc::fchown(parent.as_raw_fd(), u32::MAX, group) }, 0);
        std::fs::set_permissions(directory.path(), std::fs::Permissions::from_mode(0o2775))
            .unwrap();
        let source = input.path().join("test.rpm");
        let destination = directory.path().join("test.rpm");
        std::fs::write(&source, "new").unwrap();
        let metadata = std::fs::metadata(&source).unwrap();
        assert_ne!(metadata.gid(), group);
        move_artifact(&source, &destination).unwrap();
        let copied = std::fs::metadata(destination).unwrap();
        assert_eq!(copied.uid(), metadata.uid());
        assert_eq!(copied.gid(), group);
        assert_ne!(copied.ino(), metadata.ino());
    }

    #[test]
    fn cross_filesystem_publication_copies_and_removes_source() {
        use std::os::unix::fs::MetadataExt;
        let output = tempfile::tempdir().unwrap();
        let Ok(input) = tempfile::tempdir_in("/dev/shm") else {
            return;
        };
        if std::fs::metadata(input.path()).unwrap().dev()
            == std::fs::metadata(output.path()).unwrap().dev()
        {
            return;
        }
        let source = input.path().join("test.rpm");
        let destination = output.path().join("test.rpm");
        std::fs::write(&source, "new").unwrap();
        move_artifact(&source, &destination).unwrap();
        assert_eq!(std::fs::read_to_string(destination).unwrap(), "new");
        assert!(!source.exists());
    }

    #[test]
    fn source_unlink_failure_after_publication_is_not_artifact_failure() {
        use std::os::unix::fs::PermissionsExt;
        // SAFETY: geteuid has no arguments or pointer requirements.
        if unsafe { nix::libc::geteuid() } == 0 {
            return;
        }
        let input = tempfile::tempdir().unwrap();
        let output = tempfile::tempdir().unwrap();
        let source = input.path().join("test.rpm");
        let destination = output.path().join("test.rpm");
        std::fs::write(&source, "new").unwrap();
        std::fs::set_permissions(input.path(), std::fs::Permissions::from_mode(0o555)).unwrap();
        let result = move_artifact(&source, &destination);
        std::fs::set_permissions(input.path(), std::fs::Permissions::from_mode(0o755)).unwrap();
        result.unwrap();
        assert_eq!(std::fs::read_to_string(destination).unwrap(), "new");
        assert!(source.exists());
    }

    fn python(script: &str, paths: &[&Path]) -> tokio::process::Command {
        let mut command = tokio::process::Command::new("python3");
        command.arg("-c").arg(script).args(paths);
        command
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
        let backend = MockBackend::new(None, "src".into(), output.path().into(), vec![]);
        let caller =
            tokio::spawn(
                async move { backend.run_single_process(build, Some(cleanup), tmp).await },
            );
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
            MockBackend::default().run_single_process(build, Some(cleanup), tmp),
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
        let backend = MockBackend::new(None, "src".into(), output.path().into(), vec![]);
        let caller =
            tokio::spawn(
                async move { backend.run_single_process(build, Some(cleanup), tmp).await },
            );
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
                move_artifact(entry.path(), &dest)?;
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
                move_artifact(entry.path(), &dest)?;
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
            move_artifact(entry.path(), &dest)?;

            if is_rpm {
                rpms.push(dest);
            }
        }

        //println!("rpms: {:?}", rpms);
        Ok(rpms)
    }
}
