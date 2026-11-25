//! Previews map-web with sharsheret's own architecture, taken entirely from
//! `project_map::example::sharsheret`.
//!
//!     cargo run -p map-web --example preview -- --serve 127.0.0.1:8000
//!     cargo run -p map-web --example preview -- --out /path/to/map.html
//!     cargo run -p map-web --example preview -- --proposals --out /path/to/map.html
//!
//! `--serve` answers every request with the page until stopped. `--out`
//! writes the page to a file and exits. `--proposals` first lays a few
//! sample proposals over the map, to show how the page draws them.

use project_map::*;
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::process::ExitCode;

const USAGE: &str = "usage: preview [--proposals] (--serve <address:port> | --out <file>)";

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
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let proposals = args.first().is_some_and(|flag| flag == "--proposals");
    if proposals {
        args.remove(0);
    }
    let output = match parse(&args) {
        Ok(output) => output,
        Err(usage) => {
            eprintln!("{usage}");
            return ExitCode::FAILURE;
        }
    };
    let mut map = project_map::example::sharsheret();
    if proposals {
        propose(&mut map);
    }
    let page = map_web::page(&map);
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
        "Serving the sharsheret example map at http://{}/",
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

/// Sample proposals: a map cache is added, Map web's kind and Mapper's use
/// of it are updated, and Map persist is removed with its connections.
fn propose(map: &mut ProjectMap) {
    let note = CitationId("proposal:cache".into());
    map.evidence.insert(
        note.clone(),
        MapEvidence {
            kind: EvidenceKind::Proposal,
            build_unit: None,
            summary: "Sample: keep built maps in memory instead of saving them.".into(),
            location: None,
        },
    );
    map.modules.insert(
        ModuleId("map-cache".into()),
        MapModule {
            status: Status::ProposedAddition,
            name: "map-cache".into(),
            surface: vec![],
            description: judged(
                &note,
                ModuleDescription {
                    concern: "Keeps built maps in memory.".into(),
                    kind: ModuleKind::Effect,
                    layer: Some(LayerName("Map".into())),
                },
                "Sample addition.",
            ),
        },
    );
    for (from, to, role) in [
        ("map-cache", "project-map", UseRole::DirectUse),
        ("mapper", "map-cache", UseRole::SuppliesInput),
    ] {
        map.dependencies.insert(
            DependencyId {
                from: ModuleId(from.into()),
                to: ModuleId(to.into()),
            },
            MapDependency {
                status: Status::ProposedAddition,
                evidence: vec![note.clone()],
                description: judged(
                    &note,
                    RouteDescription {
                        role,
                        other_roles: vec![],
                    },
                    "Sample addition.",
                ),
            },
        );
    }
    let web = map.modules.get_mut(&ModuleId("map-web".into())).unwrap();
    if let Review::Judged { judgment, .. } = &web.description {
        let mut changed = judgment.clone();
        changed.kind = ModuleKind::Logic;
        web.status = Status::ProposedUpdate {
            proposed: judged(&note, changed, "Sample update: rendering becomes pure."),
        };
    }
    for (id, dependency) in map.dependencies.iter_mut() {
        if id.from.0 == "mapper" && id.to.0 == "map-web" {
            dependency.status = Status::ProposedUpdate {
                proposed: judged(
                    &note,
                    RouteDescription {
                        role: UseRole::Wires,
                        other_roles: vec![],
                    },
                    "Sample update: Mapper builds the page.",
                ),
            };
        }
        if id.from.0 == "map-persist" || id.to.0 == "map-persist" {
            dependency.status = Status::ProposedRemoval;
        }
    }
    map.modules
        .get_mut(&ModuleId("map-persist".into()))
        .unwrap()
        .status = Status::ProposedRemoval;
    map.routes
        .get_mut(&RouteId("save-the-project-map".into()))
        .unwrap()
        .status = Status::ProposedRemoval;
}

fn judged<T>(note: &CitationId, judgment: T, reason: &str) -> Review<T> {
    Review::Judged {
        judgment,
        reason: reason.into(),
        primary: note.clone(),
        supporting: vec![],
    }
}
