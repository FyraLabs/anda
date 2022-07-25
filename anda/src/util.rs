use crate::error::PackerError;
use anyhow::{ Result};
use log::{debug, info};
use std::collections::HashSet;
use tokio::fs::File;
use std::path::{ PathBuf, Path};
use std::{fs, io};
use walkdir::WalkDir;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use async_zip::write::{EntryOptions, ZipFileWriter};
use async_zip::read::seek::ZipFileReader;
use async_zip::Compression;

pub struct ProjectPacker;

impl ProjectPacker {
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

        //let mut tar = TarBuilder::new(packfile);

        // parse gitignore file
        let gitignore_path = path.join(".gitignore");
        let andaignore_path = path.join(".andaignore");

        let mut file_list = HashSet::new();

        if gitignore_path.exists() {
            let gitignore = gitignore::File::new(&gitignore_path).unwrap();

            let files = gitignore.included_files();

            for file in files.unwrap() {
                let file_path = file.strip_prefix(&path).unwrap();
                debug!("adding {}", file_path.display());
                if file_path.exists() {
                    file_list.insert(file_path.to_path_buf());
                }
            }
        }

        if andaignore_path.exists() {
            let andaignore = gitignore::File::new(&andaignore_path).unwrap();

            let files = andaignore.included_files();

            for file in files.unwrap() {
                let file_path = file.strip_prefix(&path).unwrap();
                if file_path.exists() {
                    file_list.insert(file_path.to_path_buf());
                }
            }
        }

        //tar.follow_symlinks(true);
        // if gitignore and andaignore files don't exists, add all files in folder
        if !andaignore_path.exists() && !gitignore_path.exists() {
            WalkDir::new(&path)
                .into_iter()
                .filter_map(|e| e.ok())
                .filter(|e| e.file_type().is_file())
                .for_each(|e| {
                    let file_path = e.path().strip_prefix(&path).unwrap();
                    file_list.insert(file_path.to_path_buf());
                });
        }
        let old_dir = std::env::current_dir().unwrap();

        std::env::set_current_dir(path).unwrap();

        //let mut tasks = Vec::new();

        //tar.append_dir_all(".", path)?;
        for file in file_list {
            debug!("adding {}", file.display());

            // set current directory to path


            if file.is_file() {
                // spawn a thread to add file to tarball
                let opts = EntryOptions::new(
                    file.to_str().unwrap().to_string(),
                    Compression::Zstd,
                );

                // read data from file to buf
                let mut file = File::open(file).await?;
                //let metadata = file.metadata().await?;
                let mut buf = vec![];
                file.read_to_end(&mut buf).await?;
                // add file to zip pack
                writer.write_entry_whole(opts, &buf).await.unwrap();
            }
        }

        debug!("Finishing pack");
        //tar.finish().await.unwrap();
        writer.close().await.unwrap();
        std::env::set_current_dir(old_dir).unwrap();

        Ok(packfile_path)
    }

    pub async fn unpack_and_build(path: &PathBuf, workdir: Option<PathBuf>) -> Result<(), PackerError> {
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
                    return Err(PackerError::Path("workdir already exists, please delete it manually".to_string()));
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

            debug!("{}", entry.name());

            // create parent directories if needed
            let mut path = workdir.clone();
            path.push(entry.name());
            let parent = path.parent().unwrap();
            if !parent.exists() {
                fs::create_dir_all(parent).unwrap();
            }
            let buf = i.read_to_end_crc().await.unwrap();

            // write files to disk
            let mut file = File::create(path).await?;
            file.write_all(&buf).await?;
        }


        // extract zip file to workdir

        //let old_pwd = std::env::current_dir().unwrap();


        std::env::set_current_dir(&workdir).unwrap();

        // print current dir
        debug!("{}", std::env::current_dir().unwrap().display());
        // execute anda build internally
        crate::build::ProjectBuilder::new(workdir).build().await?;

        Ok(())
    }
}
