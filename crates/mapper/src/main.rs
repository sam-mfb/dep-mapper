//! The `srs` binary: parses the command line and runs the command with the
//! installed implementations, the file system, the terminal and the network.

use mapper::cli::{self, Command};
use mapper::implementations::Installed;
use mapper::run::{self, MapOutputs};
use mapper::serve::PageServer;
use std::net::SocketAddr;
use std::path::Path;
use std::process::ExitCode;

/// Notes go to standard error, pages to files and the network.
struct Terminal;

impl MapOutputs for Terminal {
    fn note(&mut self, message: &str) {
        eprintln!("srs: {message}");
    }

    fn write_page(&mut self, path: &Path, page: &str) -> std::io::Result<()> {
        std::fs::write(path, page)
    }

    fn serve_page(&mut self, address: SocketAddr, page: &str) -> std::io::Error {
        match PageServer::bind(address) {
            Ok(server) => server.serve(page, &mut |error| {
                eprintln!("srs: a connection failed: {error}");
            }),
            Err(error) => error,
        }
    }
}

fn main() -> ExitCode {
    let command = match cli::parse(std::env::args_os()) {
        Ok(command) => command,
        Err(outcome) if outcome.requested => {
            print!("{}", outcome.text);
            return ExitCode::SUCCESS;
        }
        Err(outcome) => {
            eprint!("{}", outcome.text);
            return ExitCode::from(2);
        }
    };
    let result = match command {
        Command::Map(command) => run::map(&command, &Installed, &mut Terminal),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("srs: {error}");
            ExitCode::FAILURE
        }
    }
}
