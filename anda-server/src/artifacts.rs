use anyhow::Result;
use aws_sdk_s3::types::ByteStream;
use aws_sdk_s3::{Client, Config, Credentials, Endpoint, Region};
use lazy_static::lazy_static;
use std::env;
use std::path::PathBuf;
use tokio::fs::File;
use tokio::io::AsyncReadExt;
use walkdir::WalkDir;

lazy_static! {
    pub static ref CLIENT: Client = Client::from_conf(
        Config::builder()
            .credentials_provider(Credentials::new(
                env::var("S3_ACCESS_KEY").unwrap().as_str(),
                env::var("S3_SECRET_KEY").unwrap().as_str(),
                None,
                None,
                "Environment"
            ))
            .region(Region::new("us-east-1"))
            .endpoint_resolver(Endpoint::immutable(
                env::var("S3_ENDPOINT").unwrap().parse().unwrap(),
            ))
            .build()
    );
    pub static ref BUCKET: String = env::var("S3_BUCKET").unwrap();
}

pub struct S3Artifact {
    pub connection: Client,
}

impl S3Artifact {
    pub fn new() -> Result<S3Artifact> {
        dotenv::dotenv().ok();
        Ok(S3Artifact {
            connection: CLIENT.clone(),
        })
    }

    pub async fn upload_file(&self, dest: &str, src: PathBuf) -> Result<()> {
        // convert path to absoluate path
        let file_path = src.canonicalize()?;
        println!("Uploading {} to {}", file_path.display(), dest);
        // Read file from `file` path
        let mut file = File::open(file_path).await?;

        let metadata = file.metadata().await?;

        // convert to &[u8]
        let mut bytes = vec![0; metadata.len() as usize];
        // Read entire file into `bytes`
        file.read(&mut bytes).await?;
        // upload to S3
        self.connection
            .put_object()
            .key(dest)
            .body(ByteStream::from(bytes))
            .bucket(BUCKET.as_str())
            .send()
            .await?;
        // self.connection.put_object(path, &bytes).await?;
        Ok(())
    }

    pub async fn upload_folder(&self, dest: &str, src: PathBuf) -> Result<()> {
        // convert to relative path
        Ok(for entry in WalkDir::new(&src) {
            let entry = entry?;
            if entry.file_type().is_file() {
                let file_path = entry.into_path();
                // get file name
                // let file_name = file_path.file_name().unwrap().to_str().unwrap();
                let real_path = format!("{}/{}", dest, file_path.strip_prefix(&src).unwrap().display());
                self.upload_file(&real_path, file_path).await?;
            }
        })
    }
}

#[cfg(test)]
mod test_s3 {
    use std::env::current_dir;

    use super::*;

    #[tokio::test]
    async fn test_s3() {
        let artifact = S3Artifact::new().unwrap();
        let ls = artifact
            .connection
            .list_objects()
            .bucket(BUCKET.as_str())
            .send()
            .await
            .unwrap();
        println!("{:#?}", ls);
    }

    #[tokio::test]
    async fn test_s3_upload() {
        let artifact = S3Artifact::new().unwrap();
        artifact
            .upload_file("/test/cargo.toml", PathBuf::from("./Cargo.toml"))
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn test_s3_upload_folder() {
        let artifact = S3Artifact::new().unwrap();
        artifact
            .upload_folder("/test", current_dir().unwrap())
            .await
            .unwrap();
    }
}
