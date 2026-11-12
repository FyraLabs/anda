use std::{
    fs::{self, Permissions},
    io,
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
};

use color_eyre::{eyre::eyre, Result};
use tempfile::NamedTempFile;
use xattr::FileExt;

const SELINUX_LABEL: &str = "security.selinux";

#[derive(PartialEq, Eq)]
struct FileIdentity {
    uid: u32,
    gid: u32,
    selinux_label: Option<Vec<u8>>,
}

impl FileIdentity {
    fn of(path: &Path) -> io::Result<Self> {
        let metadata = fs::metadata(path)?;
        Ok(Self { uid: metadata.uid(), gid: metadata.gid(), selinux_label: selinux_label(path)? })
    }

    fn apply_to(&self, file: &NamedTempFile, permissions: Permissions) -> io::Result<()> {
        let current = Self::of(file.path())?;
        if (current.uid, current.gid) != (self.uid, self.gid) {
            std::os::unix::fs::fchown(file.as_file(), Some(self.uid), Some(self.gid))?;
            // chown clears set-id bits, so restore the copied mode
            file.as_file().set_permissions(permissions)?;
        }
        if current.selinux_label != self.selinux_label {
            match &self.selinux_label {
                Some(label) => file.as_file().set_xattr(SELINUX_LABEL, label)?,
                None => file.as_file().remove_xattr(SELINUX_LABEL)?,
            }
        }
        Ok(())
    }
}

fn selinux_label(path: &Path) -> io::Result<Option<Vec<u8>>> {
    match xattr::get(path, SELINUX_LABEL) {
        Err(error) if error.raw_os_error() == Some(nix::libc::ENOTSUP) => Ok(None),
        label => label,
    }
}

fn hard_link_into(directory: &Path, target: &Path) -> io::Result<NamedTempFile> {
    let link = NamedTempFile::new_in(directory)?;
    fs::remove_file(link.path())?;
    fs::hard_link(target, link.path())?;
    Ok(link)
}

fn is_unsupported_link(error: &io::Error) -> bool {
    matches!(
        error.raw_os_error(),
        Some(nix::libc::EXDEV | nix::libc::EPERM | nix::libc::EOPNOTSUPP)
    )
}

pub struct Artifact {
    source: PathBuf,
    destination: PathBuf,
}

impl Artifact {
    pub const fn new(source: PathBuf, destination: PathBuf) -> Self {
        Self { source, destination }
    }

    pub fn publish(self) -> Result<()> {
        ArtifactSet(vec![self]).publish()
    }

    fn directory(&self) -> Result<&Path> {
        self.destination.parent().ok_or_else(|| eyre!("Invalid artifact destination"))
    }

    fn existing_destination(&self) -> Result<Option<&Path>> {
        match fs::symlink_metadata(&self.destination) {
            Ok(metadata) if metadata.file_type().is_file() => Ok(Some(&self.destination)),
            Ok(_) => Err(eyre!(
                "Artifact destination is not a regular file: {}",
                self.destination.display()
            )),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error.into()),
        }
    }

    fn stage(&self) -> Result<NamedTempFile> {
        let directory = self.directory()?;
        let staged = NamedTempFile::new_in(directory)?;
        let expected =
            FileIdentity::of(self.existing_destination()?.unwrap_or_else(|| staged.path()))?;
        let source_metadata = fs::metadata(&self.source)?;
        if !source_metadata.is_file() {
            return Err(eyre!("Artifact source is not a regular file"));
        }
        if FileIdentity::of(&self.source)? == expected {
            match hard_link_into(directory, &self.source) {
                Ok(link) => return Ok(link),
                Err(error) if is_unsupported_link(&error) => {}
                Err(error) => return Err(error.into()),
            }
        }
        fs::copy(&self.source, staged.path())?;
        expected.apply_to(&staged, source_metadata.permissions())?;
        Ok(staged)
    }

    fn back_up_destination(&self) -> Result<Option<NamedTempFile>> {
        if !self.destination.exists() {
            return Ok(None);
        }
        Ok(Some(hard_link_into(self.directory()?, &self.destination)?))
    }

    fn remove_source(&self) {
        if let Err(error) = fs::remove_file(&self.source) {
            tracing::warn!(%error, path = %self.source.display(), "Cannot remove published artifact source");
        }
    }
}

struct PreparedArtifact<'a> {
    artifact: &'a Artifact,
    staged: NamedTempFile,
    backup: Option<NamedTempFile>,
}

impl<'a> PreparedArtifact<'a> {
    fn prepare(artifact: &'a Artifact) -> Result<Self> {
        Ok(Self { artifact, staged: artifact.stage()?, backup: artifact.back_up_destination()? })
    }

    fn roll_back(&self) -> Result<(), String> {
        let destination = &self.artifact.destination;
        self.backup
            .as_ref()
            .map_or_else(
                || fs::remove_file(destination),
                |backup| fs::rename(backup.path(), destination),
            )
            .map_err(|error| format!("{}: {error}", destination.display()))
    }

    fn retain_backup(self) {
        let Some(backup) = self.backup else { return };
        match backup.keep() {
            Ok((_, path)) => {
                tracing::error!(path = %path.display(), "Artifact backup retained for recovery");
            }
            Err(error) => tracing::error!(%error, "Cannot retain artifact backup"),
        }
    }
}

pub struct ArtifactSet(Vec<Artifact>);

impl ArtifactSet {
    pub const fn new(artifacts: Vec<Artifact>) -> Self {
        Self(artifacts)
    }

    pub fn publish(&self) -> Result<()> {
        self.publish_with(|staged, destination| fs::rename(staged, destination))
    }

    fn publish_with<F>(&self, mut publish: F) -> Result<()>
    where
        F: FnMut(&Path, &Path) -> io::Result<()>,
    {
        let prepared = self.0.iter().map(PreparedArtifact::prepare).collect::<Result<Vec<_>>>()?;
        for (index, artifact) in prepared.iter().enumerate() {
            if let Err(error) = publish(artifact.staged.path(), &artifact.artifact.destination) {
                return Err(Self::roll_back(prepared, index, error));
            }
        }
        self.0.iter().for_each(Artifact::remove_source);
        Ok(())
    }

    fn roll_back(
        prepared: Vec<PreparedArtifact<'_>>,
        published: usize,
        error: io::Error,
    ) -> color_eyre::Report {
        let failures: Vec<_> =
            prepared.iter().take(published).rev().filter_map(|a| a.roll_back().err()).collect();
        if failures.is_empty() {
            return error.into();
        }
        prepared.into_iter().for_each(PreparedArtifact::retain_backup);
        eyre!("{error}; artifact rollback failed: {}", failures.join(", "))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    fn publish(source: &Path, destination: &Path) -> Result<()> {
        Artifact::new(source.to_path_buf(), destination.to_path_buf()).publish()
    }

    fn other_supplementary_group(gid: u32) -> Option<nix::unistd::Gid> {
        nix::unistd::getgroups().unwrap().into_iter().find(|group| group.as_raw() != gid)
    }

    #[test]
    fn replaces_old_output_without_copying() {
        let directory = tempfile::tempdir().unwrap();
        let source = directory.path().join("source.rpm");
        let destination = directory.path().join("output.rpm");
        fs::write(&source, "new artifact").unwrap();
        fs::write(&destination, "old artifact").unwrap();
        let inode = fs::metadata(&source).unwrap().ino();
        publish(&source, &destination).unwrap();
        assert!(!source.exists());
        assert_eq!(fs::metadata(&destination).unwrap().ino(), inode);
        assert_eq!(fs::read_to_string(destination).unwrap(), "new artifact");
    }

    #[test]
    fn later_publication_failure_rolls_back_the_whole_set() {
        let input = tempfile::tempdir().unwrap();
        let output = tempfile::tempdir().unwrap();
        let paths: Vec<_> = ["test.src.rpm", "test.noarch.rpm", "extra.rpm"]
            .into_iter()
            .map(|name| {
                let source = input.path().join(name);
                let destination = output.path().join(name);
                fs::write(&source, "new").unwrap();
                fs::write(&destination, "old").unwrap();
                (source, destination)
            })
            .collect();
        let set = ArtifactSet::new(
            paths
                .iter()
                .cloned()
                .map(|(source, destination)| Artifact::new(source, destination))
                .collect(),
        );
        let mut calls = 0;
        let result = set.publish_with(|source, destination| {
            calls += 1;
            if calls == 3 {
                Err(io::Error::from_raw_os_error(nix::libc::EIO))
            } else {
                fs::rename(source, destination)
            }
        });
        assert!(result.is_err());
        for (source, destination) in paths {
            assert_eq!(fs::read_to_string(destination).unwrap(), "old");
            assert_eq!(fs::read_to_string(source).unwrap(), "new");
        }
    }

    #[test]
    fn copy_fallback_preserves_destination_owner_and_group() {
        let directory = tempfile::tempdir().unwrap();
        let source = directory.path().join("source.rpm");
        let destination = directory.path().join("output.rpm");
        let file = fs::File::create(&source).unwrap();
        let Some(group) = other_supplementary_group(file.metadata().unwrap().gid()) else {
            return;
        };
        std::os::unix::fs::fchown(&file, None, Some(group.as_raw())).unwrap();
        fs::write(&source, "new").unwrap();
        fs::set_permissions(&source, Permissions::from_mode(0o644)).unwrap();
        fs::write(&destination, "old").unwrap();
        let old = fs::metadata(&destination).unwrap();
        let source_inode = file.metadata().unwrap().ino();
        publish(&source, &destination).unwrap();
        let new = fs::metadata(destination).unwrap();
        assert_eq!((new.uid(), new.gid()), (old.uid(), old.gid()));
        assert_ne!(new.ino(), source_inode);
    }

    #[test]
    fn copy_fallback_inherits_setgid_output_group() {
        let directory = tempfile::tempdir().unwrap();
        let input = tempfile::tempdir().unwrap();
        let parent = fs::File::open(directory.path()).unwrap();
        let Some(group) = other_supplementary_group(parent.metadata().unwrap().gid()) else {
            return;
        };
        std::os::unix::fs::fchown(&parent, None, Some(group.as_raw())).unwrap();
        fs::set_permissions(directory.path(), Permissions::from_mode(0o2775)).unwrap();
        let source = input.path().join("test.rpm");
        let destination = directory.path().join("test.rpm");
        fs::write(&source, "new").unwrap();
        let metadata = fs::metadata(&source).unwrap();
        assert_ne!(metadata.gid(), group.as_raw());
        publish(&source, &destination).unwrap();
        let copied = fs::metadata(destination).unwrap();
        assert_eq!(copied.uid(), metadata.uid());
        assert_eq!(copied.gid(), group.as_raw());
        assert_ne!(copied.ino(), metadata.ino());
    }

    #[test]
    fn cross_filesystem_publication_copies_and_removes_source() {
        let output = tempfile::tempdir().unwrap();
        let Ok(input) = tempfile::tempdir_in("/dev/shm") else {
            return;
        };
        if fs::metadata(input.path()).unwrap().dev() == fs::metadata(output.path()).unwrap().dev() {
            return;
        }
        let source = input.path().join("test.rpm");
        let destination = output.path().join("test.rpm");
        fs::write(&source, "new").unwrap();
        publish(&source, &destination).unwrap();
        assert_eq!(fs::read_to_string(destination).unwrap(), "new");
        assert!(!source.exists());
    }

    #[test]
    fn source_unlink_failure_after_publication_is_not_artifact_failure() {
        if nix::unistd::geteuid().is_root() {
            return;
        }
        let input = tempfile::tempdir().unwrap();
        let output = tempfile::tempdir().unwrap();
        let source = input.path().join("test.rpm");
        let destination = output.path().join("test.rpm");
        fs::write(&source, "new").unwrap();
        fs::set_permissions(input.path(), Permissions::from_mode(0o555)).unwrap();
        let result = publish(&source, &destination);
        fs::set_permissions(input.path(), Permissions::from_mode(0o755)).unwrap();
        result.unwrap();
        assert_eq!(fs::read_to_string(destination).unwrap(), "new");
        assert!(source.exists());
    }
}
