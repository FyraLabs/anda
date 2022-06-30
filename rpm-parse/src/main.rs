use quick_xml::events::Event;
use reqwest;
use std::time::SystemTime;
use std::{
    borrow::BorrowMut,
    fs::File,
    io::{self, Write},
};
use tokio;

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let cli: String = format!("{}[2J", 27 as char);
    const URL: &str = "https://ftp.yz.yamagata-u.ac.jp/pub/linux/fedora-projects/fedora/linux/releases/36/Everything/x86_64/os/repodata/f0f7fc0fca36fdec9d2b684d7fd8a07512e49d74849acfbb2a74a96989b6f180-filelists.xml.gz";
    const FILE: &str = "filelists.xml.gz";
    const SIZE: f32 = 53282995.0;
    const FILE2: &str = "filelists.xml";

    println!("Downloading...");
    let mut response = reqwest::get(URL).await.unwrap();
    let mut file = File::create(FILE)?;
    let mut size: usize = 0;
    let time = SystemTime::now();
    while let Some(mut item) = response.chunk().await.unwrap() {
        size += item.len();
        print!(
            "{}\r {:.2}% | {:.0} KiB/s | {}/{}",
            cli,
            size as f32 / SIZE * 100.0,
            size as f32
                / 1024.0
                / SystemTime::now()
                    .duration_since(time)
                    .unwrap()
                    .as_secs_f32(),
            size,
            SIZE as i32
        );
        io::stdout().flush().unwrap();
        file.write_all(item.borrow_mut())?;
        // time = SystemTime::now();
    }
    file.sync_all()?;

    println!("\nDecompressing...");
    let mut decompressed = File::create(FILE2)?;
    let file = File::open(FILE)?;
    let mut inflate = libflate::gzip::Decoder::new(file)?;
    io::copy(&mut inflate, &mut decompressed).unwrap();

    println!("Parsing XML...");
    let mut reader = quick_xml::Reader::from_file(FILE2).unwrap();
    let mut buf: Vec<u8> = Vec::new();
    // let mut txt = Vec::new();

    loop {
        match reader.read_event(&mut buf) {
            Ok(Event::Start(ref e)) => {
                let tag = String::from_utf8_lossy(e.name());
                println!("tag: {:?}", tag);
            }
            Ok(Event::Text(e)) => (), //txt.push(e.unescape_and_decode(&reader).unwrap()),
            Ok(Event::Eof) => break, // exits the loop when reaching end of file
            Err(e) => panic!("Error at position {}: {:?}", reader.buffer_position(), e),
            _ => (), // There are several other `Event`s we do not consider here
        }
    }
    println!("Finished!");
    Ok(())
}
