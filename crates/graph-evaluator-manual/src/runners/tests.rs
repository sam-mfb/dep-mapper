use super::*;
use crate::fixture::Scratch;
use crate::store::DirStore;
use std::cell::Cell;
use std::rc::Rc;

fn task() -> AgentTask {
    AgentTask {
        location: "/tmp/workbook".into(),
        prompt: "Evaluate the graph.".into(),
    }
}

/// Counts pauses, and after a set number marks the workbook submitted.
struct Pauses {
    count: Rc<Cell<u32>>,
    submit_after: Option<u32>,
    root: std::path::PathBuf,
}

impl Pause for Pauses {
    fn pause(&self, _duration: Duration) {
        self.count.set(self.count.get() + 1);
        if Some(self.count.get()) == self.submit_after {
            DirStore::new(&self.root).set_submitted(true).unwrap();
        }
    }
}

#[derive(Clone, Default)]
struct Shared(Rc<std::cell::RefCell<Vec<u8>>>);

impl Write for Shared {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.borrow_mut().extend_from_slice(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

fn waiting(
    scratch: &Scratch,
    submit_after: Option<u32>,
    out: Shared,
) -> (WaitForSubmission, Rc<Cell<u32>>) {
    let count = Rc::new(Cell::new(0));
    let pauses = Pauses {
        count: count.clone(),
        submit_after,
        root: scratch.0.clone(),
    };
    let runner = WaitForSubmission::new(
        Box::new(DirStore::new(&scratch.0)),
        Box::new(out),
        Box::new(pauses),
        Duration::from_secs(5),
        Duration::from_secs(60),
    );
    (runner, count)
}

#[test]
fn waiting_shows_the_request_and_stops_once_the_answers_are_submitted() {
    let scratch = Scratch::new();
    let out = Shared::default();
    let (runner, pauses) = waiting(&scratch, Some(3), out.clone());
    runner.run(&task()).unwrap();
    assert_eq!(pauses.get(), 3);
    let shown = String::from_utf8(out.0.borrow().clone()).unwrap();
    assert!(shown.starts_with("Evaluate the graph."));
}

#[test]
fn waiting_gives_up_quietly_after_the_limit_so_the_evaluator_reports_it() {
    let scratch = Scratch::new();
    let (runner, pauses) = waiting(&scratch, None, Shared::default());
    runner.run(&task()).unwrap();
    assert_eq!(pauses.get(), 12);
}

/// Each call's program, arguments and directory.
type Calls = Rc<std::cell::RefCell<Vec<(String, Vec<String>, PathBuf)>>>;

struct Recorded {
    calls: Calls,
    result: Result<bool, String>,
}

impl Processes for Recorded {
    fn run(&self, program: &str, args: &[String], directory: &Path) -> Result<bool, String> {
        self.calls
            .borrow_mut()
            .push((program.into(), args.to_vec(), directory.to_path_buf()));
        self.result.clone()
    }
}

fn claude(result: Result<bool, String>) -> (ClaudeCode, Calls) {
    let calls = Rc::default();
    let processes = Recorded {
        calls: Rc::clone(&calls),
        result,
    };
    let runner = ClaudeCode::new(
        Box::new(processes),
        "claude".into(),
        suggested_claude_args(),
        PathBuf::from("/src/project"),
    );
    (runner, calls)
}

#[test]
fn claude_code_runs_with_the_prompt_in_the_project_directory() {
    let (runner, calls) = claude(Ok(true));
    runner.run(&task()).unwrap();
    let calls = calls.borrow();
    let (program, args, directory) = &calls[0];
    assert_eq!(program, "claude");
    assert_eq!(
        args[..2],
        ["--allowedTools".to_string(), "Bash(srs eval:*)".to_string()]
    );
    assert_eq!(
        args[args.len() - 2..],
        ["-p".to_string(), "Evaluate the graph.".to_string()]
    );
    assert_eq!(directory, &PathBuf::from("/src/project"));
}

#[test]
fn claude_code_failures_are_run_errors() {
    let (failed, _) = claude(Ok(false));
    assert_eq!(
        failed.run(&task()).unwrap_err().message,
        "claude exited with a failure"
    );
    let (missing, _) = claude(Err("not found".into()));
    assert_eq!(
        missing.run(&task()).unwrap_err().message,
        "could not start claude: not found"
    );
}
