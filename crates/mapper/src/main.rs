//! The `srs` binary: parses the command line and runs the command with the
//! installed implementations, the file system, the terminal and the network.

use graph_evaluator_manual::commands::Outcome;
use mapper::cli::{self, Command};
use mapper::eval;
use mapper::implementations::Installed;
use mapper::run::{self, MapOutputs};
use mapper::serve::PageServer;
use std::net::SocketAddr;
use std::path::Path;
use std::process::ExitCode;

/// Notes go to standard error, maps and pages to files, pages to the network.
struct Terminal;

impl MapOutputs for Terminal {
    fn note(&mut self, message: &str) {
        eprintln!("srs: {message}");
    }

    fn write_file(&mut self, path: &Path, text: &str) -> std::io::Result<()> {
        std::fs::write(path, text)
    }

    fn serve_page(&mut self, address: SocketAddr, page: &str) -> std::io::Error {
        let server = match PageServer::bind(address) {
            Ok(server) => server,
            Err(error) => return error,
        };
        match server.address() {
            Ok(bound) => self.note(&format!("serving the map at http://{bound}/")),
            Err(error) => return error,
        }
        server.serve(page, &mut |error| {
            eprintln!("srs: a connection failed: {error}");
        })
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
    match command {
        Command::Map(command) => match run::map(&command, &Installed, &mut Terminal) {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => {
                eprintln!("srs: {error}");
                ExitCode::FAILURE
            }
        },
        // A refused answer is reported in the output; a usage or store
        // failure is not an answer at all.
        Command::Eval(command) => match eval::eval(&command, &mut std::io::stdout()) {
            Ok(Outcome::Done) => ExitCode::SUCCESS,
            Ok(Outcome::Refused) => ExitCode::FAILURE,
            Err(error) => {
                eprintln!("srs eval: {error}");
                ExitCode::from(2)
            }
        },
    }
}
