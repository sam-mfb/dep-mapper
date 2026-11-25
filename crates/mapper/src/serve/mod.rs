//! Serves one page over HTTP: every request gets the page. Enough for a
//! browser on the same machine to show a map.

use std::io::{BufRead, BufReader, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};

pub struct PageServer {
    listener: TcpListener,
}

impl PageServer {
    pub fn bind(address: SocketAddr) -> std::io::Result<Self> {
        Ok(PageServer {
            listener: TcpListener::bind(address)?,
        })
    }

    /// The address the server listens on, with the port chosen when bound
    /// to port 0.
    pub fn address(&self) -> std::io::Result<SocketAddr> {
        self.listener.local_addr()
    }

    /// Answers each connection with the page. A connection that fails is
    /// passed to `failed` and serving goes on. Returns only when accepting
    /// connections fails.
    pub fn serve(&self, page: &str, failed: &mut dyn FnMut(std::io::Error)) -> std::io::Error {
        loop {
            match self.listener.accept() {
                Ok((stream, _)) => {
                    if let Err(error) = answer(stream, page) {
                        failed(error);
                    }
                }
                Err(error) => return error,
            }
        }
    }
}

/// Reads the request head and sends the page.
fn answer(stream: TcpStream, page: &str) -> std::io::Result<()> {
    let mut reader = BufReader::new(&stream);
    let mut line = String::new();
    loop {
        line.clear();
        if reader.read_line(&mut line)? == 0 || line == "\r\n" || line == "\n" {
            break;
        }
    }
    let mut stream = &stream;
    write!(
        stream,
        "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        page.len()
    )?;
    stream.write_all(page.as_bytes())?;
    stream.flush()
}

#[cfg(test)]
mod tests;
