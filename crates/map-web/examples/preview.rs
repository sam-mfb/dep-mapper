//! Previews map-web with dep-mapper's own architecture, taken entirely from
//! `project_map::example::dep_mapper`.
//!
//!     cargo run -p map-web --example preview -- --serve 127.0.0.1:8000
//!     cargo run -p map-web --example preview -- --out /path/to/map.html
//!
//! `--serve` answers every request with the page until stopped. `--out`
//! writes the page to a file and exits.

use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::process::ExitCode;

const USAGE: &str = "usage: preview (--serve <address:port> | --out <file>)";

enum Output {
    Serve(String),
    File(String),
}

fn parse(args: &[String]) -> Result<Output, String> {
    match args {
        [flag, value] if flag == "--serve" => Ok(Output::Serve(value.clone())),
        [flag, value] if flag == "--out" => Ok(Output::File(value.clone())),
        _ => Err(USAGE.into()),
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let output = match parse(&args) {
        Ok(output) => output,
        Err(usage) => {
            eprintln!("{usage}");
            return ExitCode::FAILURE;
        }
    };
    let page = map_web::page(&project_map::example::dep_mapper());
    let result = match output {
        Output::File(path) => std::fs::write(&path, &page).map(|()| println!("Wrote {path}")),
        Output::Serve(address) => serve(&address, &page),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("preview failed: {error}");
            ExitCode::FAILURE
        }
    }
}

fn serve(address: &str, page: &str) -> std::io::Result<()> {
    let listener = TcpListener::bind(address)?;
    println!(
        "Serving the dep-mapper example map at http://{}/",
        listener.local_addr()?
    );
    for stream in listener.incoming() {
        // One failed connection does not stop the preview.
        if let Err(error) = stream.and_then(|stream| respond(stream, page)) {
            eprintln!("connection failed: {error}");
        }
    }
    Ok(())
}

/// Reads the request head and answers with the page, whatever was asked.
fn respond(mut stream: TcpStream, page: &str) -> std::io::Result<()> {
    let mut reader = BufReader::new(&stream);
    let mut line = String::new();
    while reader.read_line(&mut line)? > 2 {
        line.clear();
    }
    write!(
        stream,
        "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{page}",
        page.len()
    )
}
