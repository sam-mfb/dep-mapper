//! Serves an HTML page with a live preview, and reloads open browsers when
//! the file changes. A file that cannot be read is shown as a problem.
//!
//!     cargo run -p map-preview --example serve -- 127.0.0.1:8000 /path/to/page.html

use map_preview::{PollingWatcher, Preview, Settings, Watcher};
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;
use std::time::Duration;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [address, page] = args.as_slice() else {
        eprintln!("usage: serve <address:port> <page.html>");
        return ExitCode::FAILURE;
    };
    let settings = Settings {
        heartbeat: Duration::from_secs(15),
        on_connection_error: Arc::new(|error| eprintln!("connection: {error}")),
    };
    let page = PathBuf::from(page);
    let result = Preview::start(address, settings)
        .map_err(|error| error.to_string())
        .and_then(|preview| {
            println!(
                "Previewing {} at http://{}/",
                page.display(),
                preview.address()
            );
            let mut watcher = PollingWatcher::new(vec![page.clone()], Duration::from_millis(200))
                .map_err(|error| error.to_string())?;
            loop {
                match std::fs::read_to_string(&page) {
                    Ok(text) => preview.show_page(text),
                    Err(error) => {
                        preview.show_problems(vec![format!("{}: {error}", page.display())])
                    }
                }
                watcher
                    .wait_for_change()
                    .map_err(|error| error.to_string())?;
            }
        });
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("serve failed: {error}");
            ExitCode::FAILURE
        }
    }
}
