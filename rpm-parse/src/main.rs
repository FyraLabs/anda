use libflate::gzip::Decoder;
use quick_xml::{
    events::{BytesStart, Event},
    Reader,
};
use std::{error::Error, io::BufReader, str, time::{SystemTime, Duration}};

const URL: &str = "https://ftp.yz.yamagata-u.ac.jp/pub/linux/fedora-projects/fedora/linux/releases/36/Everything/x86_64/os/repodata/f0f7fc0fca36fdec9d2b684d7fd8a07512e49d74849acfbb2a74a96989b6f180-filelists.xml.gz";
type Rd<'a> = Reader<BufReader<Decoder<&'a [u8]>>>;
type Res<T> = Result<T, Box<dyn Error>>;

struct Pkg {
    id: String,
    name: String,
    arch: String,       // x86_64
    ver: String,        // 0.1.23
    rel: String,        // 2.fc36
    files: Vec<String>, // /usr/share/pkgname/...
}

fn get_attr(e: &BytesStart, attrname: &str) -> Res<String> {
    match e.try_get_attribute(attrname)? {
        Some(attr) => Ok(format!("{}", String::from_utf8(attr.value.to_vec())?)),
        None => panic!("Expect value for {} on tag {:?}", attrname, e),
    }
}

fn goto_next_pkg(mut reader: Rd) -> Rd {
    let mut buf = vec![];
    loop {
        match reader.read_event(&mut buf) {
            Ok(Event::End(_)) => return reader,
            Err(e) => panic!("Error at position {}: {:?}", reader.buffer_position(), e),
            _ => (),
        }
    }
}

fn parse_pkg(reader: &mut Rd, e: BytesStart) -> Res<Pkg> {
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

fn find_pkg(mut reader: Rd, key: &str, value: &str) -> Res<Option<Pkg>> {
    println!("Parsing");
    // let mut waitforend = vec![];
    loop {
        match reader.read_event(&mut vec![]) {
            Ok(Event::Start(e)) => {
                let tag = String::from_utf8(e.name().to_vec())?;
                if tag == "package" && get_attr(&e, key)? == value {
                    return Ok(Some(parse_pkg(&mut reader, e)?));
                }
                // waitforend.push(tag);
            }
            // Ok(Event::End(ref e)) => {
            //     assert_eq!(String::from_utf8(e.name().to_vec())?, waitforend.pop().unwrap());
            // }
            Err(e) => return Err(Box::new(e)),
            Ok(Event::Eof) => return Ok(None),
            _ => (),
        }
    }
}

//TODO drop goto_next_pkg
fn parse(mut reader: Rd) -> Res<()> {
    println!("Parsing");
    let mut pkgs: Vec<Pkg> = Vec::new();
    let t = SystemTime::now();
    loop {
        match reader.read_event(&mut vec![]) {
            Ok(Event::Start(e)) => {
                let tag = String::from_utf8_lossy(e.name());
                if tag != "package" {
                    reader = goto_next_pkg(reader);
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

fn main() -> Res<()> {
    let client = reqwest::blocking::ClientBuilder::new().timeout(Duration::from_secs(60*5)).build()?;
    println!("Sending request");
    let resp = client.get(URL).send()?;
    println!("Downloading");
    let bytes = resp.bytes()?.to_vec();
    let decoder = Decoder::new(&*bytes)?;
    let bufreader = BufReader::new(decoder);
    let reader = Reader::from_reader(bufreader);
    let t = SystemTime::now();
    println!("Finding pkg");
    find_pkg(reader, "name" ,"zzuf")?.unwrap();
    println!("Took {} seconds", SystemTime::now().duration_since(t)?.as_secs());
    Ok(())
}
