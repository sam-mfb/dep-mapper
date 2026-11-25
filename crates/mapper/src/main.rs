//! The `srs` binary: parses the command line and runs the command with the
//! installed implementations, the file system, the terminal and the network.

use graph_evaluator_manual::commands::Outcome;
use map_preview::{PollingWatcher, Preview, Settings, Watcher};
use mapper::cli::{self, Command};
use mapper::draft::{self, DraftCommand, DraftError, DraftFiles, PreviewSink, WatchFiles};
use mapper::eval;
use mapper::implementations::Installed;
use mapper::run::{self, MapOutputs};
use mapper::serve::PageServer;
use mapper::show;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;
use std::time::Duration;

/// How often an idle preview connection is checked, and how often the
/// draft's files are checked for changes.
const PREVIEW_HEARTBEAT: Duration = Duration::from_secs(15);
const WATCH_INTERVAL: Duration = Duration::from_millis(250);

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
            Ok(bound) => MapOutputs::note(self, &format!("serving the map at http://{bound}/")),
            Err(error) => return error,
        }
        server.serve(page, &mut |error| {
            eprintln!("srs: a connection failed: {error}");
        })
    }
}

impl DraftFiles for Terminal {
    fn read(&self, path: &Path) -> std::io::Result<String> {
        std::fs::read_to_string(path)
    }

    fn write(&mut self, path: &Path, text: &str) -> std::io::Result<()> {
        std::fs::write(path, text)
    }

    fn note(&mut self, message: &str) {
        eprintln!("srs: {message}");
    }
}

/// The live preview, seen as where each build of the draft is shown.
struct Shown(Preview);

impl PreviewSink for Shown {
    fn show_page(&self, page: String) {
        self.0.show_page(page);
    }

    fn show_problems(&self, problems: Vec<String>) {
        for problem in &problems {
            eprintln!("srs: {problem}");
        }
        self.0.show_problems(problems);
    }
}

/// Watches the files of the latest build by polling.
struct Polling(Option<PollingWatcher>);

impl WatchFiles for Polling {
    fn watch(&mut self, paths: &[PathBuf]) -> Result<(), String> {
        let watcher = PollingWatcher::new(paths.to_vec(), WATCH_INTERVAL)
            .map_err(|error| error.to_string())?;
        self.0 = Some(watcher);
        Ok(())
    }

    fn wait(&mut self) -> Result<(), String> {
        let watcher = self.0.as_mut().ok_or("nothing is watched yet")?;
        watcher
            .wait_for_change()
            .map(|_| ())
            .map_err(|error| error.to_string())
    }
}

/// Runs `srs draft preview` until watching fails.
fn preview(draft: &Path, listen: SocketAddr) -> Result<(), String> {
    let settings = Settings {
        heartbeat: PREVIEW_HEARTBEAT,
        on_connection_error: Arc::new(|error| eprintln!("srs: a connection failed: {error}")),
    };
    let preview =
        Preview::start(&listen.to_string(), settings).map_err(|error| error.to_string())?;
    eprintln!(
        "srs: previewing {} at http://{}/",
        draft.display(),
        preview.address()
    );
    let error = draft::preview(draft, &Terminal, &Shown(preview), &mut Polling(None));
    Err(error.to_string())
}

/// The installed implementations, with their settings from the environment.
fn installed() -> Installed {
    Installed {
        anthropic_api_key: std::env::var("ANTHROPIC_API_KEY")
            .ok()
            .filter(|key| !key.is_empty()),
    }
}

fn main() -> ExitCode {
    let current_dir = match std::env::current_dir() {
        Ok(dir) => dir,
        Err(error) => {
            eprintln!("srs: cannot read the current directory: {error}");
            return ExitCode::FAILURE;
        }
    };
    let command = match cli::parse(std::env::args_os(), &current_dir) {
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
        // A refused answer is reported in the output; a usage or store
        // failure is not an answer at all.
        Command::Eval(command) => {
            return match eval::eval(&command, &mut std::io::stdout()) {
                Ok(Outcome::Done) => ExitCode::SUCCESS,
                Ok(Outcome::Refused) => ExitCode::FAILURE,
                Err(error) => {
                    eprintln!("srs eval: {error}");
                    ExitCode::from(2)
                }
            };
        }
        Command::Show(command) => match std::fs::read_to_string(&command.map) {
            Ok(text) => show::show(&command, &text, &mut Terminal).map_err(|e| e.to_string()),
            Err(error) => Err(format!("reading {}: {error}", command.map.display())),
        },
        Command::Map(command) => {
            run::map(&command, &installed(), &mut Terminal).map_err(|e| e.to_string())
        }
        Command::Draft(DraftCommand::Check { draft }) => {
            draft::check(&draft, &mut Terminal).map_err(|e: DraftError| e.to_string())
        }
        Command::Draft(DraftCommand::Render { draft, out }) => {
            draft::render(&draft, &out, &mut Terminal).map_err(|e| e.to_string())
        }
        Command::Draft(DraftCommand::Preview { draft, listen }) => preview(&draft, listen),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("srs: {error}");
            ExitCode::FAILURE
        }
    }
}
