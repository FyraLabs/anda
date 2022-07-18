use anyhow::Result;
use lazy_static::lazy_static;
use s3::bucket::Bucket;
use s3::creds::Credentials;
use s3::region::Region;
use tokio::fs::File;
use tokio::io::AsyncReadExt;
use std::env;
use std::io::Read;
use std::path::PathBuf;

lazy_static! {
    pub static ref BUCKET: Bucket = Bucket::new(
        env::var("S3_BUCKET").unwrap().as_str(),
        Region::Custom {
            region: "".into(),
            endpoint: env::var("S3_ENDPOINT").unwrap().into(),
        },
        Credentials::new(
            Some(env::var("S3_ACCESS_KEY").unwrap().as_str()),
            Some(env::var("S3_SECRET_KEY").unwrap().as_str()),
            None,
            None,
            None
        )
        .unwrap()
    )
    .unwrap();
}

struct S3Artifact {
    pub connection: s3::Bucket,
}

impl S3Artifact {
    pub fn new() -> Result<S3Artifact> {
        dotenv::dotenv().ok();
        Ok(S3Artifact {
            connection: BUCKET.clone(),
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
        self.connection.put_object(path, &bytes).await?;
        Ok(())
    }

}

#[cfg(test)]
mod test_s3 {
    use super::*;

    #[tokio::test]
    async fn test_s3() {
        let artifact = S3Artifact::new().unwrap();
        let ls = artifact.connection.list("".to_string(), Some("/".to_string())).await.unwrap();
        println!("{:#?}", ls);
        let a = artifact.connection.get_object("/Dockerfile".to_string()).await.unwrap();
        println!("{:?}", std::str::from_utf8(&a.0).unwrap());
    }

    #[tokio::test]
    async fn test_s3_upload() {
        let artifact = S3Artifact::new().unwrap();
        artifact.upload_file("/Dockerfile", PathBuf::from("./Dockerfile")).await.unwrap();
    }
}
