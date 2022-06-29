use async_compression::futures::bufread::GzipDecoder;
use futures::{
    io::{self, BufReader, ErrorKind},
    prelude::*,
};
use reqwest;
use serde::Deserialize;
use serde_xml_rs::{from_reader, from_str};
use std::str;

#[derive(Debug, Deserialize)]
pub struct Metalink {
    pub files: Files,
}

#[derive(Debug, Deserialize)]
pub struct Files {
    #[serde(rename = "file")]
    pub files: Vec<File>,
}

#[derive(Debug, Deserialize)]
pub struct File {
    pub name: String,
    pub size: u16,
    pub verification: Verification,
    pub resources: Resources,
}

#[derive(Debug, Deserialize)]
pub struct Verification {
    #[serde(rename = "hash")]
    pub hashes: Vec<Hash>,
}

#[derive(Debug, Deserialize)]
pub struct Hash {
    pub r#type: HashType,
}

#[derive(Debug, Deserialize)]
pub struct Resources {
    #[serde(rename = "url")]
    pub urls: Vec<Url>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HashType {
    Md5,
    Sha1,
    Sha256,
    Sha512,
}

#[derive(Debug, Deserialize)]
pub struct Url {
    pub protocol: Protocol,
    pub r#type: UrlType,
    pub location: String,
    pub preference: i8,
}

#[derive(Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Protocol {
    Http,
    Https,
    Rsync,
}

#[derive(Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum UrlType {
    Https,
    Http,
    Rsync,
}

async fn _parse_metalink() {
    let resp = r#"<metalink><files><file name="repomd.xml"><size>6285</size><verification><hash type="md5">hash</hash></verification><resources maxconnections="1"><url protocol="http" type="http" location="JP" preference="100">link</url></resources></file></files></metalink>"#;
    let metalink: Metalink = from_str(&resp).unwrap();
    assert_eq!(&metalink.files.files[0].name, "repomd.xml");
}

#[derive(Debug, Deserialize)]
pub struct Repomd {
    pub revision: u32,
    #[serde(rename = "data")]
    pub data: Vec<Data>,
}

#[derive(Debug, Deserialize)]
pub struct Data {
    pub r#type: String,
    pub checksum: Checksum,
    #[serde(rename = "open-checksum")]
    pub ocm: Option<OpenChecksum>,
    pub location: Location,
    pub timestamp: u32,
    pub size: u32,
    #[serde(rename = "open-size")]
    pub osize: Option<u32>,
    #[serde(rename = "header-size")]
    pub hsize: Option<u32>,
}

#[derive(Debug, Deserialize)]
pub struct Checksum {
    pub r#type: HashType,
    #[serde(rename = "$value")]
    pub value: String,
}

#[derive(Debug, Deserialize)]
pub struct OpenChecksum {
    pub r#type: HashType,
    #[serde(rename = "$value")]
    pub value: String,
}

#[derive(Debug, Deserialize)]
pub struct Location {
    pub href: String,
}

async fn _parse_repomd() {
    let resp = r#"
<repomd xmlns="http://linux.duke.edu/metadata/repo" xmlns:rpm="http://linux.duke.edu/metadata/rpm">
  <revision>1651698971</revision>
  <data type="group_xz">
    <checksum type="sha256">3f69beebaa5fb330617ca37b79c7e6381a415957f2999bc141386a1271ec86bc</checksum>
    <location href="repodata/3f69beebaa5fb330617ca37b79c7e6381a415957f2999bc141386a1271ec86bc-comps-Everything.x86_64.xml.xz"/>
    <timestamp>1651698877</timestamp>
    <size>257776</size>
  </data>
  <data type="group_zck">
    <checksum type="sha256">cd4101d88f0f384899265e0677fb70a7d9e5bdf9f3da89b36bf39b6ade52c93f</checksum>
    <open-checksum type="sha256">3f69beebaa5fb330617ca37b79c7e6381a415957f2999bc141386a1271ec86bc</open-checksum>
    <header-checksum type="sha256">b77cbb8a2379c3099b5332c45075d59f92161ca2f01b3de71a3c1dffc08322f6</header-checksum>
    <location href="repodata/cd4101d88f0f384899265e0677fb70a7d9e5bdf9f3da89b36bf39b6ade52c93f-comps-Everything.x86_64.xml.zck"/>
    <timestamp>1651698971</timestamp>
    <size>479764</size>
    <open-size>257776</open-size>
    <header-size>1240</header-size>
  </data>
</repomd>
"#;
    let repomd: Repomd = from_str(&resp).unwrap();
    assert_eq!(repomd.data[0].size, 257776);
}

#[cfg(test)]
mod test_super {
    use super::*;

    #[test]
    fn parse_metalink() {
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(_parse_metalink())
    }

    #[test]
    fn parse_repomd() {
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(_parse_repomd())
    }
}
