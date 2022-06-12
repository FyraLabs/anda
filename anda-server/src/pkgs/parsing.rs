use std::str;
use serde::{Deserialize, Serialize};
// use serde_yaml::Value;
// use tokio::fs::{read, read_dir, ReadDir};
use serde_xml_rs::from_str;
use tokio;
// use reqwest;


#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct Metalink {
    // pub files: Files,
    #[serde(rename = "files")]
    pub files: Vec<FileKind>,
}

// #[derive(Debug, Serialize, Deserialize, PartialEq)]
// pub struct Files {
// }

#[derive(Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub enum FileKind {
    File(File)
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct File {
    pub name: String,
    pub size: i16,
    #[serde(rename = "verification")]
    pub verification: Vec<HashKind>,
    pub resources: Vec<UrlKind>,
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub enum HashKind {
    Hash(Hash)
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct Hash {
    pub r#type: HashType,
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub enum HashType {
    md5,
    sha1,
    sha256,
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub enum UrlKind {
    Url(Url)
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub struct Url {
    pub protocol: Protocol,
    pub r#type: UrlType,
    pub location: String,
    pub preference: i8,
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub enum Protocol {
    http,
    https,
    rsync,
}

#[derive(Debug, Serialize, Deserialize, PartialEq)]
pub enum UrlType {
    https,
    http,
    rsync,
}


async fn test_parse() {
    // let resp = reqwest::get("https://mirrors.fedoraproject.org/metalink?repo=fedora-36&arch=x86_64")
    //     .await
    //     .unwrap()
    //     .text()
    //     .await
    //     .unwrap();
    // println!("{}", resp);
    let resp = r#"
<metalink>
<files>
<file name="repomd.xml">
<size>6285</size>
<verification>
    <hash type="md5">471e9eec10af547e2ac5883ef8085680</hash>
    <hash type="sha1">3a4214f0efe3ac4d193a24b96648861613a66292</hash>
    <hash type="sha256">4900a802ace6c0f4b13d10ec6b645cb47cdd8069c9d92bbc3231334183ec401c</hash>
    <hash type="sha512">ff0c749bbc7508106f9e44c261f74f5d0d03abe6793d75dfbdc780d340d168cddebfcdc4e336cf219f7ab8c2ce7753466f304dcffdad114c0b35759def023e11</hash>
</verification>
<resources maxconnections="1">
    <url protocol="http" type="http" location="JP" preference="100">http://ftp.riken.jp/Linux/fedora/releases/36/Everything/x86_64/os/repodata/repomd.xml</url>
    <url protocol="rsync" type="rsync" location="JP" preference="100">rsync://ftp.riken.jp/fedora/releases/36/Everything/x86_64/os/repodata/repomd.xml</url>
    <url protocol="https" type="https" location="JP" preference="100">https://ftp.riken.jp/Linux/fedora/releases/36/Everything/x86_64/os/repodata/repomd.xml</url>
    <url protocol="https" type="https" location="CN" preference="99">https://mirrors.tuna.tsinghua.edu.cn/fedora/releases/36/Everything/x86_64/os/repodata/repomd.xml</url>
    <url protocol="http" type="http" location="CN" preference="99">http://mirrors.tuna.tsinghua.edu.cn/fedora/releases/36/Everything/x86_64/os/repodata/repomd.xml</url>
    <url protocol="rsync" type="rsync" location="CN" preference="99">rsync://mirrors.tuna.tsinghua.edu.cn/fedora/releases/36/Everything/x86_64/os/repodata/repomd.xml</url>
    <url protocol="http" type="http" location="JP" preference="98">http://ftp.iij.ad.jp/pub/linux/Fedora/fedora/linux/releases/36/Everything/x86_64/os/repodata/repomd.xml</url>
    <url protocol="rsync" type="rsync" location="JP" preference="98">rsync://ftp.iij.ad.jp/pub/linux/Fedora/fedora/linux/releases/36/Everything/x86_64/os/repodata/repomd.xml</url>
    <url protocol="https" type="https" location="JP" preference="97">https://ftp.yz.yamagata-u.ac.jp/pub/linux/fedora-projects/fedora/linux/releases/36/Everything/x86_64/os/repodata/repomd.xml</url>
    <url protocol="http" type="http" location="JP" preference="97">http://ftp.yz.yamagata-u.ac.jp/pub/linux/fedora-projects/fedora/linux/releases/36/Everything/x86_64/os/repodata/repomd.xml</url>
    <url protocol="rsync" type="rsync" location="JP" preference="97">rsync://ftp.yz.yamagata-u.ac.jp/pub/linux/fedora-projects/fedora/linux/releases/36/Everything/x86_64/os/repodata/repomd.xml</url>
    <url protocol="http" type="http" location="TH" preference="96">http://mirror2.totbb.net/fedora/linux/releases/36/Everything/x86_64/os/repodata/repomd.xml</url>
    <url protocol="rsync" type="rsync" location="TH" preference="96">rsync://mirror2.totbb.net/fedora/linux/releases/36/Everything/x86_64/os/repodata/repomd.xml</url>
    <url protocol="rsync" type="rsync" location="ID" preference="95">rsync://fedora.mirror.angkasa.id/fedora-enchilada/linux/releases/36/Everything/x86_64/os/repodata/repomd.xml</url>
    <url protocol="http" type="http" location="ID" preference="95">http://fedora.mirror.angkasa.id/pub/fedora/linux/releases/36/Everything/x86_64/os/repodata/repomd.xml</url>
    <url protocol="http" type="http" location="ID" preference="94">http://mr.heru.id/fedora/linux/releases/36/Everything/x86_64/os/repodata/repomd.xml</url>
    <url protocol="https" type="https" location="ID" preference="94">https://mr.heru.id/fedora/linux/releases/36/Everything/x86_64/os/repodata/repomd.xml</url>
    <url protocol="https" type="https" location="SG" preference="93">https://download.nus.edu.sg/mirror/fedora/linux/releases/36/Everything/x86_64/os/repodata/repomd.xml</url>
    <url protocol="http" type="http" location="SG" preference="93">http://download.nus.edu.sg/mirror/fedora/linux/releases/36/Everything/x86_64/os/repodata/repomd.xml</url>
    <url protocol="rsync" type="rsync" location="SG" preference="93">rsync://download.nus.edu.sg/fedora/linux/releases/36/Everything/x86_64/os/repodata/repomd.xml</url>
    <url protocol="rsync" type="rsync" location="CN" preference="92">rsync://mirrors.bfsu.edu.cn/fedora/releases/36/Everything/x86_64/os/repodata/repomd.xml</url>
    <url protocol="http" type="http" location="CN" preference="92">http://mirrors.bfsu.edu.cn/fedora/releases/36/Everything/x86_64/os/repodata/repomd.xml</url>
    <url protocol="http" type="http" location="CN" preference="91">http://mirror.lzu.edu.cn/fedora/releases/36/Everything/x86_64/os/repodata/repomd.xml</url>
    <url protocol="https" type="https" location="CN" preference="91">https://mirror.lzu.edu.cn/fedora/releases/36/Everything/x86_64/os/repodata/repomd.xml</url>
    <url protocol="http" type="http" location="CN" preference="90">http://mirrors.163.com/fedora/releases/36/Everything/x86_64/os/repodata/repomd.xml</url>
    <url protocol="rsync" type="rsync" location="CN" preference="90">rsync://mirrors.163.com/fedora/releases/36/Everything/x86_64/os/repodata/repomd.xml</url>
</resources>
</file>
</files>
</metalink>"#;
    let metalink: Metalink = from_str(&resp).unwrap();
    match &metalink.files[0] {
        FileKind::File(f) => println!("{}", f.name)
    }


#[cfg(test)]
mod test_super {
    use super::*;

    #[test]
    fn parse() {
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(test_parse())
    }
    }
}
