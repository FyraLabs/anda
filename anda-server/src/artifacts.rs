use s3::bucket::Bucket;
use s3::creds::Credentials;
use s3::region::Region;
use anyhow::Result;
use lazy_static::lazy_static;
use std::env;

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
    ).unwrap()
  ).unwrap();
}

struct S3Artifact {
    connection: s3::Bucket,
}


impl S3Artifact {
    pub fn new() -> Result<S3Artifact> {
        Ok(S3Artifact {
            connection: BUCKET.clone(),
        })
    }
}