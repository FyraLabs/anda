use anyhow::Result;
use aws_sdk_s3::{
    output::{PutObjectOutput, ListObjectsOutput, GetObjectOutput},
    types::ByteStream,
    {Client, Config, Credentials, Endpoint, Region},
};
use lazy_static::lazy_static;
use std::{env, path::PathBuf, collections::HashMap};
use tokio::{fs::File, io::AsyncReadExt};
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
    pub static ref S3_ENDPOINT: String = env::var("S3_ENDPOINT").unwrap();
    pub static ref BUCKET: String = env::var("S3_BUCKET").unwrap();
}

pub struct S3Artifact {
    pub connection: Client,
}

impl S3Artifact {
    pub fn new() -> Result<S3Artifact> {
        dotenv::dotenv()?;
        Ok(S3Artifact {
            connection: CLIENT.clone(),
        })
    }

    pub async fn upload_file(&self, dest: &str, src: PathBuf, metadata: HashMap<String, String>) -> Result<PutObjectOutput> {
        // convert path to absoluate path
        let file_path = src.canonicalize()?;
        println!("Uploading {} to {}", file_path.display(), dest);
        // Read file from `file` path
        let mut file = File::open(file_path).await?;

        //let metadata = file.metadata().await?;

        // convert to &[u8]
        let mut bytes = Vec::with_capacity(metadata.len() as usize);
        // Read entire file into `bytes`
        file.read_to_end(&mut bytes).await?;
        // upload to S3
        let mut ret = self
            .connection
            .put_object()
            .key(dest)
            .body(bytes.into())
            .bucket(BUCKET.as_str());

        for (key, value) in metadata.iter() {
            ret = ret.metadata(key, value);
        }
        let ret = ret
            .send()
            .await?;
        // self.connection.put_object(path, &bytes).await?;
        Ok(ret)
    }

    pub async fn upload_folder(&self, dest: &str, src: PathBuf) -> Result<()> {
        // convert to relative path
        for entry in WalkDir::new(&src) {
            let entry = entry?;
            if entry.file_type().is_file() {
                let file_path = entry.into_path();
                // get file name
                // let file_name = file_path.file_name().unwrap().to_str().unwrap();
                let real_path = format!(
                    "{}/{}",
                    dest,
                    file_path.strip_prefix(&src).unwrap().display()
                );
                self.upload_file(&real_path, file_path, HashMap::new()).await?;
            }
        };
        Ok(())
    }

    pub async fn get_file(&self, dest: &str) -> Result<GetObjectOutput> {
        let ret = self
            .connection
            .get_object()
            .key(dest)
            .bucket(BUCKET.as_str())
            .send()
            .await?;
        Ok(ret)
    }

    pub async fn get_by_e_tag(&self, e_tag: &str, dest: &str) -> Result<GetObjectOutput> {
        let ret = self
            .connection
            .get_object()
            .key(dest)
            .bucket(BUCKET.as_str())
            .if_match(e_tag)
            .send()
            .await?;
        Ok(ret)
    }

    pub async fn list_files(&self, dest: &str) -> Result<ListObjectsOutput> {
        Ok(self
            .connection
            .list_objects()
            .bucket(BUCKET.as_str())
            .prefix(format!("{}/", dest).as_str())
            .send()
            .await?)
    }

    pub async fn is_file(&self, dest: &str) -> Result<bool> {
        let ret = self
            .connection
            .head_object()
            .key(dest)
            .bucket(BUCKET.as_str())
            .send()
            .await?;

        let a = ret.content_length;

        if a == 0 {
            Ok(false)
        } else {
            Ok(true)
        }
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
            .prefix("artifacts/")
            .send()
            .await
            .unwrap();
        println!("{:#?}", ls);
    }

    #[tokio::test]
    async fn test_s3_upload() {
        let artifact = S3Artifact::new().unwrap();
        artifact
            .upload_file("/test/cargo.toml", PathBuf::from("./Cargo.toml"), HashMap::new())
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
