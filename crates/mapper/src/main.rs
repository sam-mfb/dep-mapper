//! The `srs` binary: parses the command line and runs the command with the
//! installed implementations, the file system, the terminal and the network.

use map_preview::{PollingWatcher, Preview, Settings, Watcher};
use mapper::cli::{self, Command};
use mapper::draft::{self, DraftCommand, DraftError, DraftFiles, PreviewSink, WatchFiles};
use mapper::implementations::Installed;
use mapper::run::{self, MapOutputs};
use mapper::serve::PageServer;
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

/// Watches the files of the latest build, starting again when they change.
struct Polling(Option<(Vec<PathBuf>, PollingWatcher)>);

impl WatchFiles for Polling {
    fn wait(&mut self, paths: &[PathBuf]) -> Result<(), String> {
        let current = matches!(&self.0, Some((watched, _)) if watched == paths);
        if !current {
            let watcher = PollingWatcher::new(paths.to_vec(), WATCH_INTERVAL)
                .map_err(|error| error.to_string())?;
            self.0 = Some((paths.to_vec(), watcher));
        }
        let (_, watcher) = self.0.as_mut().expect("set above");
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
        Command::Map(command) => {
            run::map(&command, &Installed, &mut Terminal).map_err(|e| e.to_string())
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
