//! Utility functions for Andaman.

use crate::{build, error::PackerError};
use anyhow::Result;
use async_zip::read::seek::ZipFileReader;
use async_zip::write::{EntryOptions, ZipFileWriter};
use async_zip::Compression;
use futures::stream::TryStreamExt;
use git2::Repository;
use log::{debug, info};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::{fs, io};
use tokio::fs::File;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio_util::compat::FuturesAsyncReadCompatExt;
use uuid::Uuid;
use indicatif::{ProgressBar, ProgressStyle};

/// Returns the current commit hash of the given repository located at the given path.
pub fn current_commit(path: &Path) -> Option<String> {
    let repo = Repository::open(path).ok()?;
    let head = repo.head().ok()?;
    let commit = head.peel_to_commit().ok()?;
    Some(commit.id().to_string())
}

pub fn branch_name(path: &Path) -> Option<String> {
    let repo = Repository::open(path).ok()?;
    let head = repo.head().ok()?;
    let name = head.shorthand()?;
    Some(name.to_string())
}
/// Project packer object.
/// Deals with `andasrc.zip` files.
pub struct ProjectPacker;

impl ProjectPacker {
    pub async fn pack_git(url: &str) -> Result<PathBuf, PackerError> {
        // parse url and get the repo slug
        let repo_slug = url.split('/').last().unwrap();

        //let tempdir = tempfile::tempdir().unwrap();
        let tempdir = PathBuf::from("/tmp/anda-packer");
        fs::remove_dir_all(&tempdir).unwrap_or(());

        let git_url = tempdir.join(repo_slug);

        fs::create_dir_all(&git_url)?;

        let repo = Repository::clone_recurse(url, git_url).map_err(PackerError::Git)?;

        let repo_path = repo.path().to_path_buf();
        let repo_path = repo_path.parent().unwrap();
        let pack = Self::pack(repo_path, None).await?;

        Ok(pack)
    }

    pub async fn pack(path: &Path, output: Option<String>) -> Result<PathBuf, PackerError> {
        // get folder name of path
        // check if path is folder
        if !path.is_dir() {
            return Err(PackerError::Path(format!(
                "{} is not a folder",
                path.display()
            )));
        }

        let path = if path.file_name().is_none() {
            path.canonicalize().unwrap()
        } else {
            path.to_path_buf()
        };

        debug!("packing {}", path.display());

        let folder_name = path.clone();
        let folder_name = folder_name.file_name().unwrap().to_str().unwrap();

        let packfile_path = if let Some(output) = output {
            PathBuf::from(output)
        } else {
            let packfile_name = format!("/tmp/{}.andasrc.zip", folder_name);
            PathBuf::from(packfile_name)
        };

        let mut packfile = File::create(&packfile_path).await?;

        let mut writer = ZipFileWriter::new(&mut packfile);
        let mut file_list: HashSet<PathBuf> = HashSet::new();

        //debug!("walking {}", path.display());
        let walker = ignore::Walk::new(&path);

        for result in walker {
            //debug!("{:?}", result);
            file_list.insert(
                result
                    .unwrap()
                    .path()
                    .to_path_buf()
                    .strip_prefix(&path)
                    .unwrap()
                    .to_path_buf(),
            );
        }

        let old_dir = std::env::current_dir().unwrap();

        std::env::set_current_dir(path).unwrap();

        let pb = ProgressBar::new(file_list.len() as u64);
        pb.set_style(ProgressStyle::with_template(
            "{pos}/{len} Packing [{bar:30}] {msg}"
        ).unwrap()
        .progress_chars("=> ")
        );

        //tar.append_dir_all(".", path)?;
        for file in file_list {
            //pb.println(format!("adding {}", file.display()));
            pb.set_message(format!("{}", file.display()));

            // set current directory to path

            if file.is_file() {
                // spawn a thread to add file to tarball
                let opts = EntryOptions::new(file.to_str().unwrap().to_string(), Compression::Zstd);

                // read data from file to buf
                let mut file = File::open(file).await?;
                //let metadata = file.metadata().await?;
                let mut buf = vec![];
                file.read_to_end(&mut buf).await?;
                // add file to zip pack
                writer.write_entry_whole(opts, &buf).await.unwrap();
                //tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                pb.inc(1);
            }
        }
        pb.finish_with_message("done");

        debug!("Finishing pack");
        //tar.finish().await.unwrap();
        writer.close().await.unwrap();
        std::env::set_current_dir(old_dir).unwrap();

        println!("Packed {}", packfile_path.display());
        Ok(packfile_path)
    }

    pub async fn download_and_call_unpack_build(
        url: &str,
        workdir: Option<PathBuf>,
        opts: &build::BuilderOptions,
        projects: Vec<String>
    ) -> Result<(), PackerError> {
        let tmp_dir = tempfile::tempdir().unwrap();
        // download file using reqwest
        let resp = reqwest::get(url).await.unwrap();
        //let mut buf = vec![];
        let filename = resp
            .url()
            .path_segments()
            .and_then(|segments| segments.last())
            .and_then(|name| {
                if name.is_empty() {
                    None
                } else {
                    Some(name)
                }
            })
            .unwrap_or("build.andasrc.zip");
        let dest = tmp_dir.path().join(filename);

        let data = resp.bytes_stream();
        let data = data
            .map_err(|e| futures::io::Error::new(futures::io::ErrorKind::Other, e))
            .into_async_read();

        let mut data = data.compat();

        let mut file = File::create(&dest).await?;
        tokio::io::copy(&mut data, &mut file).await?;

        Self::unpack_and_build(&dest, workdir, opts, projects).await
    }

    pub async fn unpack_and_build(
        path: &PathBuf,
        workdir: Option<PathBuf>,
        opts: &build::BuilderOptions,
        projects: Vec<String>
    ) -> Result<(), PackerError> {
        //let tar = GzipDecoder::new(buf.as_slice());

        let workdir = if let Some(workdir) = workdir {
            workdir
        } else {
            PathBuf::from("/tmp/anda")
        };

        if workdir.exists() {
            // check if it's the default temp dir
            if !workdir.to_str().unwrap().contains("/tmp/") {
                info!("workdir already exists, do you want to delete it? (y/N)");
                let mut input = String::new();
                io::stdin().read_line(&mut input)?;
                if input.trim() == "y" {
                    fs::remove_dir_all(&workdir).unwrap();
                } else {
                    return Err(PackerError::Path(
                        "workdir already exists, please delete it manually".to_string(),
                    ));
                }
            }
        }

        let mut packfile = File::open(path).await?;
        let mut reader = ZipFileReader::new(&mut packfile).await.unwrap();

        // turn zip file reader into zipentryreaders

        let entry_count = reader.entries().len();

        for index in 0..entry_count {
            let i = reader.entry_reader(index).await.unwrap();
            let entry = i.entry();

            if entry.dir() {
                continue;
            }

            //debug!("{}", entry.name());

            // create parent directories if needed
            let mut path = workdir.clone();
            path.push(entry.name());
            let parent = path.parent().unwrap();
            if !parent.exists() {
                fs::create_dir_all(parent).unwrap();
            }
            let buf = i.read_to_end_crc().await.unwrap();

            // write files to disk
            let mut file = File::create(&path).await?;
            file.write_all(&buf).await?;
        }

        // extract zip file to workdir

        //let old_pwd = std::env::current_dir().unwrap();

        std::env::set_current_dir(&workdir).unwrap();

        // print current dir
        debug!("{}", std::env::current_dir().unwrap().display());
        // execute anda build internally
        crate::build::ProjectBuilder::new(workdir)
            .build(projects, opts)
            .await?;

        Ok(())
    }
}

/// Packs the project and pushes it to the server
pub async fn push_build(root: &PathBuf) -> Result<crate::api::Build, PackerError> {
    let packfile = ProjectPacker::pack(root, None).await?;

    let _build_push = crate::api::AndaBackend::new(None)
        .upload_build(Uuid::nil(), &packfile, None)
        .await?;

    todo!()
}
