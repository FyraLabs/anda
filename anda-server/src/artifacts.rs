use anyhow::Result;
use aws_sdk_s3::types::ByteStream;
use aws_sdk_s3::{Client, Config, Credentials, Region, Endpoint};
use lazy_static::lazy_static;
use tokio::fs::File;
use tokio::io::AsyncReadExt;
use std::env;
use std::path::PathBuf;

lazy_static! {
    pub static ref CLIENT: Client = Client::from_conf(Config::builder()
    .credentials_provider(Credentials::new(
        env::var("S3_ACCESS_KEY").unwrap().as_str(),
        env::var("S3_SECRET_KEY").unwrap().as_str(),    
        None,
        None,
        "Environment"
    )).region(Region::new("us-east-1")).endpoint_resolver(Endpoint::immutable(
        env::var("S3_ENDPOINT").unwrap().parse().unwrap(),
    )).build());

    pub static ref BUCKET: String = env::var("S3_BUCKET").unwrap();
}

struct S3Artifact {
    pub connection: Client,
}

impl S3Artifact {
    pub fn new() -> Result<S3Artifact> {
        dotenv::dotenv().ok();
        Ok(S3Artifact {
            connection: CLIENT.clone(),
        })
    }

    pub async fn upload_file(&self, path: &str, file: PathBuf) -> Result<()> {
        // convert path to absoluate path
        let file_path = file.canonicalize()?;
        println!("Uploading {} to {}", file_path.display(), path);
        // Read file from `file` path
        let mut file = File::open(file_path).await?;

        let metadata = file.metadata().await?;

        // convert to &[u8]
        let mut bytes = vec![0; metadata.len() as usize];
        // Read entire file into `bytes`
        file.read(&mut bytes).await?;
        // upload to S3
        self.connection.put_object().key(path).body(ByteStream::from(bytes)).bucket(BUCKET.as_str()).send().await?;
        // self.connection.put_object(path, &bytes).await?;
        Ok(())
    }

}

#[cfg(test)]
mod test_s3 {
    use rocket::http::hyper::body::Buf;

    use super::*;

    #[tokio::test]
    async fn test_s3() {
        let artifact = S3Artifact::new().unwrap();
        let ls = artifact.connection.list_objects().bucket(BUCKET.as_str()).send().await.unwrap();
        println!("{:#?}", ls);
        let a = artifact.connection.get_object().key("/Dockerfile").bucket(BUCKET.as_str()).send().await.unwrap();
        println!("{:?}", std::str::from_utf8(a.body.collect().await.unwrap().chunk()));
    }

    #[tokio::test]
    async fn test_s3_upload() {
        let artifact = S3Artifact::new().unwrap();
        artifact.upload_file("/Dockerfile", PathBuf::from("./Dockerfile")).await.unwrap();
    }
}
