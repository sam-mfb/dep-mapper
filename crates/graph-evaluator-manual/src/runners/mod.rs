//! Runners that bring a worker to a workbook.
//!
//! - `WaitForSubmission` shows the request to a person, who can start any
//!   agent harness or answer themselves, and waits until the answers are
//!   submitted or the time allowed runs out.
//! - `ClaudeCode` runs Claude Code non-interactively with the request and
//!   returns when it exits.
//!
//! Sleeping and starting processes are effects, so both come in through
//! contracts: `Pause` and `Processes`. `SystemPause` and `SystemProcesses`
//! are the standard-library implementations.

use crate::evaluator::{AgentRunner, AgentTask, RunError};
use crate::store::WorkbookStore;
use std::cell::RefCell;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Waits for a while.
pub trait Pause {
    fn pause(&self, duration: Duration);
}

pub struct SystemPause;

impl Pause for SystemPause {
    fn pause(&self, duration: Duration) {
        std::thread::sleep(duration);
    }
}

/// Shows the request, then checks the store for a submission at each
/// interval until the limit has passed.
pub struct WaitForSubmission {
    store: Box<dyn WorkbookStore>,
    out: RefCell<Box<dyn Write>>,
    pause: Box<dyn Pause>,
    interval: Duration,
    limit: Duration,
}

impl WaitForSubmission {
    /// `store` must be the store the evaluator uses. The limit counts the
    /// intervals waited, so time spent checking is not included.
    pub fn new(
        store: Box<dyn WorkbookStore>,
        out: Box<dyn Write>,
        pause: Box<dyn Pause>,
        interval: Duration,
        limit: Duration,
    ) -> WaitForSubmission {
        WaitForSubmission {
            store,
            out: RefCell::new(out),
            pause,
            interval,
            limit,
        }
    }
}

impl AgentRunner for WaitForSubmission {
    fn run(&self, task: &AgentTask) -> Result<(), RunError> {
        let mut out = self.out.borrow_mut();
        let shown = writeln!(
            out,
            "{}\n\nWaiting for the answers to be submitted...",
            task.prompt
        )
        .and_then(|()| out.flush());
        shown.map_err(|e| RunError {
            message: format!("could not show the request: {e}"),
        })?;
        let mut waited = Duration::ZERO;
        loop {
            let submitted = self.store.submitted().map_err(|e| RunError {
                message: e.to_string(),
            })?;
            if submitted || waited >= self.limit {
                // An unsubmitted workbook is reported by the evaluator, with its answers kept.
                return Ok(());
            }
            self.pause.pause(self.interval);
            waited += self.interval;
        }
    }
}

/// Starts a program and waits for it to exit.
pub trait Processes {
    /// Runs `program` with `args` in `directory`, with the caller's terminal.
    /// Returns whether it exited successfully, or why it could not start.
    fn run(&self, program: &str, args: &[String], directory: &Path) -> Result<bool, String>;
}

pub struct SystemProcesses;

impl Processes for SystemProcesses {
    fn run(&self, program: &str, args: &[String], directory: &Path) -> Result<bool, String> {
        std::process::Command::new(program)
            .args(args)
            .current_dir(directory)
            .status()
            .map(|status| status.success())
            .map_err(|e| e.to_string())
    }
}

/// Runs Claude Code non-interactively: `<program> <args...> -p <prompt>`, in
/// `directory`, which should be the project checkout so the agent can read
/// the source.
pub struct ClaudeCode {
    processes: Box<dyn Processes>,
    program: String,
    args: Vec<String>,
    directory: PathBuf,
}

/// Arguments that let Claude Code read files and run the answer commands
/// without asking, for a workbook whose commands are `srs eval`.
pub fn suggested_claude_args() -> Vec<String> {
    ["--allowedTools", "Bash(srs eval:*)", "Read", "Grep", "Glob"]
        .into_iter()
        .map(String::from)
        .collect()
}

impl ClaudeCode {
    pub fn new(
        processes: Box<dyn Processes>,
        program: String,
        args: Vec<String>,
        directory: PathBuf,
    ) -> ClaudeCode {
        ClaudeCode {
            processes,
            program,
            args,
            directory,
        }
    }
}

impl AgentRunner for ClaudeCode {
    fn run(&self, task: &AgentTask) -> Result<(), RunError> {
        let mut args = self.args.clone();
        args.push("-p".into());
        args.push(task.prompt.clone());
        match self.processes.run(&self.program, &args, &self.directory) {
            Ok(true) => Ok(()),
            Ok(false) => Err(RunError {
                message: format!("{} exited with a failure", self.program),
            }),
            Err(cause) => Err(RunError {
                message: format!("could not start {}: {cause}", self.program),
            }),
        }
    }
}

#[cfg(test)]
mod tests;
