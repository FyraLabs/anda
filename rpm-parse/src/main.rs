use libflate::gzip::Decoder;
use quick_xml::{
    events::{attributes::Attribute, BytesStart, Event},
    Reader,
};
use reqwest::blocking::Response;
use std::{
    borrow::Cow,
    error::Error,
    io::BufReader,
    str,
    string::FromUtf8Error,
    time::SystemTime,
};

const URL: &str = "https://ftp.yz.yamagata-u.ac.jp/pub/linux/fedora-projects/fedora/linux/releases/36/Everything/x86_64/os/repodata/f0f7fc0fca36fdec9d2b684d7fd8a07512e49d74849acfbb2a74a96989b6f180-filelists.xml.gz";
type Rd = Reader<BufReader<Decoder<Response>>>;

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

fn goto_next_pkg(reader: &mut Rd) {
    let mut buf = vec![];
    loop {
        match reader.read_event(&mut buf) {
            Ok(Event::End(_)) => break,
            Err(e) => panic!("Error at position {}: {:?}", reader.buffer_position(), e),
            _ => (),
        }
    }
}

fn parse_pkg(reader: &mut Rd, e: &BytesStart) -> Result<Pkg, Box<dyn Error>> {
    let mut buf = vec![];
    let mut pkg = Pkg {
        id: get_attr(&e, "pkgid")?,
        name: get_attr(&e, "name")?,
        arch: get_attr(&e, "arch")?,
        ver: String::new(),
        rel: String::new(),
        files: vec![],
    };
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


fn parse(mut reader: Rd) -> Result<(), Box<dyn Error>> {
    println!("Parsing");
    let mut buf: Vec<u8> = Vec::new();
    let mut pkgs: Vec<Pkg> = Vec::new();
    let t = SystemTime::now();
    loop {
        match reader.read_event(&mut buf) {
            Ok(Event::Start(ref e)) => {
                let tag = String::from_utf8_lossy(e.name());
                if tag != "package" {
                    goto_next_pkg(&mut reader);
                    continue;
                }
                if get_attr(&e, "name")? != "0ad-data" {
                    println!("{}", get_attr(&e, "name")?);
                    goto_next_pkg(&mut reader);
                    continue;
                }
                pkgs.push(parse_pkg(&mut reader, e)?);
            }
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

fn main() -> Result<(), Box<dyn Error>> {
    let resp = reqwest::blocking::get(URL)?;
    let decoder = Decoder::new(resp)?;
    let bufreader = BufReader::new(decoder);
    let reader = Reader::from_reader(bufreader);
    parse(reader)?;
    Ok(())
}
