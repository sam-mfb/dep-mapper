//! Serving the preview over HTTP on a local address, with the standard
//! library's TCP listener. Paths:
//!
//! - `/`: the outer page.
//! - `/map`: the last good map page, or a note that none has been built.
//! - `/events`: Server-Sent Events. Each event's data is the status JSON
//!   (`Shown::status_json`), sent on connecting and after every change.
//!
//! Each connection is served on its own thread. Connection errors go to the
//! caller's `on_connection_error`; they never stop the preview.

use crate::outer::OUTER_PAGE;
use crate::shown::Shown;
use std::io::{BufRead, BufReader, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::thread;
use std::time::Duration;
use thiserror::Error;

/// How the preview runs.
#[derive(Clone)]
pub struct Settings {
    /// How often an idle event stream sends a comment, so a closed browser
    /// tab is noticed and its thread ends.
    pub heartbeat: Duration,
    /// Receives errors on single connections, such as a browser closing
    /// one mid-response.
    pub on_connection_error: Arc<dyn Fn(&std::io::Error) + Send + Sync>,
}

#[derive(Debug, Error)]
#[error("cannot serve the preview at {address}: {source}")]
pub struct PreviewError {
    pub address: String,
    #[source]
    pub source: std::io::Error,
}

/// A running preview. It serves until the program ends.
pub struct Preview {
    shared: Arc<Shared>,
    address: SocketAddr,
}

struct Shared {
    shown: Mutex<Shown>,
    changed: Condvar,
    settings: Settings,
}

impl Shared {
    /// The shown state. A thread that panicked while holding it left only
    /// whole values behind, so the state is still usable.
    fn shown(&self) -> MutexGuard<'_, Shown> {
        self.shown
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

impl Preview {
    /// Listens on `address`, such as `127.0.0.1:8000`, and serves from a
    /// background thread. Port 0 picks a free port; see `address`.
    pub fn start(address: &str, settings: Settings) -> Result<Preview, PreviewError> {
        let listener = TcpListener::bind(address)
            .and_then(|listener| listener.local_addr().map(|bound| (listener, bound)))
            .map_err(|source| PreviewError {
                address: address.into(),
                source,
            })?;
        let (listener, bound) = listener;
        let shared = Arc::new(Shared {
            shown: Mutex::new(Shown::new()),
            changed: Condvar::new(),
            settings,
        });
        let serving = Arc::clone(&shared);
        thread::spawn(move || {
            for stream in listener.incoming() {
                let shared = Arc::clone(&serving);
                match stream {
                    Ok(stream) => {
                        thread::spawn(move || {
                            if let Err(error) = respond(stream, &shared) {
                                (shared.settings.on_connection_error)(&error);
                            }
                        });
                    }
                    Err(error) => (serving.settings.on_connection_error)(&error),
                }
            }
        });
        Ok(Preview {
            shared,
            address: bound,
        })
    }

    /// The address the preview is served at.
    pub fn address(&self) -> SocketAddr {
        self.address
    }

    /// Shows a newly built page, and tells open browsers to reload.
    pub fn show_page(&self, page: String) {
        self.shared.shown().show_page(page);
        self.shared.changed.notify_all();
    }

    /// Shows the problems that kept a page from being built, beside the last
    /// good page.
    pub fn show_problems(&self, problems: Vec<String>) {
        self.shared.shown().show_problems(problems);
        self.shared.changed.notify_all();
    }
}

const NO_MAP: &str = "<!doctype html><meta charset=\"utf-8\"><title>No map</title>\
<p>No map has been built yet.</p>";

fn respond(mut stream: TcpStream, shared: &Shared) -> std::io::Result<()> {
    let path = request_path(&stream)?;
    let path = path.split('?').next().unwrap_or_default();
    match path {
        "/" => send(
            &mut stream,
            "200 OK",
            "text/html; charset=utf-8",
            OUTER_PAGE,
        ),
        "/map" => {
            let page = shared.shown().page().map(str::to_string);
            send(
                &mut stream,
                "200 OK",
                "text/html; charset=utf-8",
                page.as_deref().unwrap_or(NO_MAP),
            )
        }
        "/events" => events(stream, shared),
        _ => send(
            &mut stream,
            "404 Not Found",
            "text/plain; charset=utf-8",
            "Not found\n",
        ),
    }
}

/// Reads the request head and returns the path of its first line.
fn request_path(stream: &TcpStream) -> std::io::Result<String> {
    let mut reader = BufReader::new(stream);
    let mut first = String::new();
    reader.read_line(&mut first)?;
    let mut line = String::new();
    while reader.read_line(&mut line)? > 2 {
        line.clear();
    }
    Ok(first.split_whitespace().nth(1).unwrap_or("/").to_string())
}

fn send(stream: &mut TcpStream, status: &str, kind: &str, body: &str) -> std::io::Result<()> {
    write!(
        stream,
        "HTTP/1.1 {status}\r\nContent-Type: {kind}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
}

/// Streams the status now and after every change, until the browser goes.
fn events(mut stream: TcpStream, shared: &Shared) -> std::io::Result<()> {
    write!(
        stream,
        "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nCache-Control: no-store\r\nConnection: keep-alive\r\n\r\n"
    )?;
    let mut sent = None;
    loop {
        let message = {
            let mut shown = shared.shown();
            if sent == Some(shown.version()) {
                let (guard, _) = shared
                    .changed
                    .wait_timeout(shown, shared.settings.heartbeat)
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                shown = guard;
            }
            if sent == Some(shown.version()) {
                None
            } else {
                sent = Some(shown.version());
                Some(shown.status_json())
            }
        };
        match message {
            Some(status) => write!(stream, "data: {status}\n\n")?,
            // A comment line: browsers ignore it, and writing it fails once
            // the browser has gone.
            None => write!(stream, ":\n\n")?,
        }
        stream.flush()?;
    }
}

#[cfg(test)]
mod tests;
