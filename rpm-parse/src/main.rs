use quick_xml::{
    events::{attributes::Attribute, BytesStart, Event},
    Reader,
};
use reqwest;
use std::{
    borrow::BorrowMut,
    borrow::Cow,
    error::Error,
    fs::File,
    io::{self, BufReader, Write},
    str,
    string::FromUtf8Error,
    time::SystemTime,
};
use tokio;

const URL: &str = "https://ftp.yz.yamagata-u.ac.jp/pub/linux/fedora-projects/fedora/linux/releases/36/Everything/x86_64/os/repodata/f0f7fc0fca36fdec9d2b684d7fd8a07512e49d74849acfbb2a74a96989b6f180-filelists.xml.gz";
const FILE: &str = "filelists.xml.gz";
const SIZE: f32 = 53282995.;
const FILE2: &str = "filelists.xml";

struct Pkg {
    id: String,
    name: String,
    arch: String,       // x86_64
    ver: String,        // 0.1.23
    rel: String,        // 2.fc36
    files: Vec<String>, // /usr/share/pkgname/...
}

fn extract_cow(c: Cow<[u8]>) -> Result<String, FromUtf8Error> {
    Ok(format!("{}", String::from_utf8(c.to_vec())?))
}
fn ex_attr(a: Attribute) -> Result<String, FromUtf8Error> {
    extract_cow(a.value)
}
fn get_attr(e: &BytesStart, attrname: &str) -> Result<String, Box<dyn Error>> {
    match e.try_get_attribute(attrname)? {
        Some(attr) => Ok(ex_attr(attr)?),
        None => panic!("Expect value for {}", attrname),
    }
}

fn goto_next_pkg(reader: &mut Reader<BufReader<File>>) {
    let mut buf = vec![];
    loop {
        match reader.read_event(&mut buf) {
            Ok(Event::End(e)) => {
                if String::from_utf8_lossy(e.name()) == "package" {
                    break;
                }
            }
            Err(e) => {
                panic!("Error at position {}: {:?}", reader.buffer_position(), e)
            }
            _ => (),
        }
    }
}

fn parse_pkg(reader: &mut Reader<BufReader<File>>, e: &BytesStart) -> Result<Pkg, Box<dyn Error>> {
    let mut pkg = Pkg {
        id: String::new(),
        name: String::new(),
        arch: String::new(),
        ver: String::new(),
        rel: String::new(),
        files: vec![],
    };
    for attr in e.attributes() {
        let attr = attr?;
        match attr.key {
            b"pkgid" => pkg.id = ex_attr(attr)?,
            b"name" => pkg.name = ex_attr(attr)?,
            b"arch" => pkg.arch = ex_attr(attr)?,
            _ => (),
        }
    }
    let mut buf = vec![];
    loop {
        match reader.read_event(&mut buf) {
            Ok(Event::Empty(e)) => {
                if e.name() == b"version" {
                    pkg.ver = get_attr(&e, "ver")?;
                    pkg.rel = get_attr(&e, "rel")?;
                } else {
                    panic!("parse_pkg: Not sure what the tag <{:?}> is", e.name());
                }
            }
            Ok(Event::Start(e)) => {
                if e.name() == b"file" {
                    if let Ok(Event::Text(e)) = reader.read_event(&mut buf) {
                        pkg.files.push(String::from_utf8(e.to_vec())?);
                    } else {
                        panic!("parse_pkg: Expected text on {}", reader.buffer_position());
                    }
                    if let Ok(Event::End(_)) = reader.read_event(&mut buf) {
                        continue;
                    } else {
                        panic!("parse_pkg: Expected end on {}", reader.buffer_position());
                    }
                }
            }
            Ok(Event::End(e)) => {
                assert_eq!(e.name(), b"package");
                break;
            }
            Err(e) => panic!("Error at position {}: {:?}", reader.buffer_position(), e),
            _ => (),
        }
    }
    Ok(pkg)
}

async fn download() -> Result<Vec<u8>, Box<dyn Error>> {
    println!("Downloading...");
    let mut response = reqwest::get(URL).await?;
    let mut file = File::create(FILE)?;
    let mut size: usize = 0;
    let time = SystemTime::now();
    let mut gz: Vec<u8> = Vec::new();
    while let Ok(Some(mut item)) = response.chunk().await {
        size += item.len();
        print!(
            "{}\r {:.2}% | {:.0} KiB/s | {}/{}",
            format!("{}[2K", 27 as char),
            size as f32 / SIZE * 100.,
            size as f32
                / 1024.
                / SystemTime::now()
                    .duration_since(time)
                    ?
                    .as_secs_f32(),
            size,
            SIZE as i32
        );
        io::stdout().flush()?;
        file.write_all(item.borrow_mut())?;
        gz.append(&mut item.to_vec());
    }
    file.sync_all()?;
    Ok(gz)
}

fn decompress(gz: Vec<u8>) -> Result<(), Box<dyn Error>> {
    println!("\nDecompressing...");
    let time = SystemTime::now();
    let mut decompressed = File::create(FILE2)?;
    let mut inflate = libflate::gzip::Decoder::new(&*gz)?;
    io::copy(&mut inflate, &mut decompressed)?;
    println!(
        "Decompression took {} seconds",
        SystemTime::now().duration_since(time)?.as_secs()
    );
    decompressed.sync_all()?;
    Ok(())
}

fn parse() -> Result<(), Box<dyn Error>> {
    println!("Parsing XML...");
    let mut reader = quick_xml::Reader::from_file(FILE2)?;
    let mut buf: Vec<u8> = Vec::new();
    let t = SystemTime::now();
    let mut pkgs: Vec<Pkg> = Vec::new();
    loop {
        match reader.read_event(&mut buf) {
            Ok(Event::Start(ref e)) => {
                let tag = String::from_utf8_lossy(e.name());
                if tag != "package" {
                    goto_next_pkg(&mut reader);
                    continue;
                }
                pkgs.push(parse_pkg(&mut reader, e)?);
            }
            Ok(Event::Text(_)) => (), // panic!("Didn't expect text at {}", reader.buffer_position()),
            Ok(Event::Eof) => break,
            Err(e) => panic!("Error at position {}: {:?}", reader.buffer_position(), e),
            _ => (),
        }
    }
    println!(
        "Parsing took {} seconds",
        SystemTime::now().duration_since(t)?.as_secs()
    );
    println!("Found {} packages", pkgs.len());
    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    // let gz = download().await?;
    // decompress(gz)?;
    parse()?;

    Ok(())
}
