use super::*;
use crate::commands::{self, Outcome};
use crate::fixture::{self, Scratch};
use crate::store::DirStore;
use crate::workbook::ByteOffsets;
use graph_evaluator::{Decision, ModuleKind, RelationshipKind};
use project_graph::{GraphId, ModuleId};
use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

/// Plays the agent: runs each command line against the workbook in turn.
struct ScriptedAgent {
    root: PathBuf,
    lines: Vec<Vec<&'static str>>,
    tasks: Rc<RefCell<Vec<AgentTask>>>,
}

impl AgentRunner for ScriptedAgent {
    fn run(&self, task: &AgentTask) -> Result<(), RunError> {
        self.tasks.borrow_mut().push(task.clone());
        let store = DirStore::new(&self.root);
        for line in &self.lines {
            let args: Vec<String> = line.iter().map(|s| s.to_string()).collect();
            let outcome =
                commands::run("srs eval", &args, &store, &mut Vec::new()).map_err(|e| {
                    RunError {
                        message: e.to_string(),
                    }
                })?;
            if outcome == Outcome::Refused {
                return Err(RunError {
                    message: format!("refused: {line:?}"),
                });
            }
        }
        Ok(())
    }
}

struct Failing;

impl AgentRunner for Failing {
    fn run(&self, _task: &AgentTask) -> Result<(), RunError> {
        Err(RunError {
            message: "the agent could not start".into(),
        })
    }
}

fn evaluator(root: &PathBuf, runner: Box<dyn AgentRunner>) -> ManualEvaluator {
    ManualEvaluator::new(
        Box::new(DirStore::new(root)),
        runner,
        Box::new(ByteOffsets),
        Settings {
            page_size: 20,
            command: "srs eval".into(),
        },
    )
}

fn full_session() -> Vec<Vec<&'static str>> {
    vec![
        vec![
            "judge",
            "app",
            "--concern",
            "Composes the program.",
            "--kind",
            "composition",
            "--cluster",
            "Program",
            "--reason",
            "It builds the store and hands it on.",
            "--cite",
            "route:r-wire",
        ],
        vec![
            "judge",
            "disk",
            "--concern",
            "Keeps records in files.",
            "--kind",
            "effect",
            "--cluster",
            "Storage",
            "--reason",
            "It implements the store contract.",
            "--cite",
            "route:r-impl",
        ],
        vec!["defer", "store", "--reason", "Only one item is visible."],
        vec!["clusters", "Program", "Storage"],
        vec![
            "relate",
            "routes/disk+store",
            "--kind",
            "implements",
            "--reason",
            "DiskStore implements Store.",
            "--cite",
            "route:r-impl",
            "--cite",
            "fact:f-impl",
        ],
        vec![
            "relate",
            "routes/app+disk",
            "--kind",
            "full-injection",
            "--reason",
            "app constructs the store.",
            "--cite",
            "route:r-wire",
        ],
        vec![
            "skip-route",
            "route:r-write",
            "--reason",
            "A direct file write; no architecture.",
        ],
        vec!["submit"],
    ]
}

#[test]
fn an_agent_that_works_through_the_workbook_produces_the_evaluation() {
    let scratch = Scratch::new();
    let tasks = Rc::new(RefCell::new(Vec::new()));
    let agent = ScriptedAgent {
        root: scratch.0.clone(),
        lines: full_session(),
        tasks: tasks.clone(),
    };
    let evaluation = evaluator(&scratch.0, Box::new(agent))
        .evaluate(&fixture::graph())
        .unwrap();

    assert_eq!(evaluation.graph, GraphId("fixture@1".into()));
    assert!(matches!(
        &evaluation.modules[&ModuleId("disk".into())],
        Decision::Judged { judgment, .. } if judgment.kind == ModuleKind::Effect
    ));
    assert!(matches!(
        &evaluation.modules[&ModuleId("store".into())],
        Decision::Deferred { .. }
    ));
    let kinds: Vec<&RelationshipKind> = evaluation.relationships.iter().map(|r| &r.kind).collect();
    assert_eq!(
        kinds,
        [
            &RelationshipKind::FullInjection,
            &RelationshipKind::Implements
        ]
    );
    let graph = fixture::graph();
    assert!(
        evaluation
            .relationships
            .iter()
            .flat_map(|r| &r.evidence)
            .all(|r| graph.resolve(r).is_some())
    );

    let tasks = tasks.borrow();
    assert_eq!(tasks.len(), 1);
    assert!(
        tasks[0].prompt.contains("INSTRUCTIONS.md") && tasks[0].prompt.contains("srs eval submit")
    );
    assert!(scratch.0.join("INSTRUCTIONS.md").exists());
}

#[test]
fn unsubmitted_answers_are_kept_and_a_later_run_resumes_them_without_the_agent() {
    let scratch = Scratch::new();
    let error = evaluator(&scratch.0, Box::new(ReturnAtOnce))
        .evaluate(&fixture::graph())
        .unwrap_err();
    assert!(error.message.contains("were not submitted"));
    assert!(
        error
            .message
            .contains("finish with `srs eval submit`, then evaluate again")
    );

    // The work happens between runs, as with `srs eval` in a terminal.
    let store = DirStore::new(&scratch.0);
    for line in full_session() {
        let args: Vec<String> = line.iter().map(|s| s.to_string()).collect();
        assert_eq!(
            commands::run("srs eval", &args, &store, &mut Vec::new()).unwrap(),
            Outcome::Done
        );
    }
    let evaluation = evaluator(&scratch.0, Box::new(Failing))
        .evaluate(&fixture::graph())
        .unwrap();
    assert_eq!(evaluation.modules.len(), 3);
}

#[test]
fn a_workbook_for_another_graph_is_never_overwritten() {
    let scratch = Scratch::new();
    let _ = evaluator(&scratch.0, Box::new(ReturnAtOnce)).evaluate(&fixture::graph());
    let mut other = fixture::graph();
    other.id = GraphId("fixture@2".into());
    let error = evaluator(&scratch.0, Box::new(ReturnAtOnce))
        .evaluate(&other)
        .unwrap_err();
    assert!(error.message.contains("belongs to graph fixture@1"));
    assert_eq!(
        DirStore::new(&scratch.0)
            .load_workbook()
            .unwrap()
            .unwrap()
            .graph,
        "fixture@1"
    );
}

#[test]
fn a_runner_failure_is_an_evaluation_error() {
    let scratch = Scratch::new();
    let error = evaluator(&scratch.0, Box::new(Failing))
        .evaluate(&fixture::graph())
        .unwrap_err();
    assert_eq!(
        error.message,
        "the evaluation run failed: the agent could not start"
    );
}

#[test]
fn submitted_answers_that_were_changed_by_hand_are_checked_again() {
    let scratch = Scratch::new();
    let agent = ScriptedAgent {
        root: scratch.0.clone(),
        lines: full_session(),
        tasks: Rc::default(),
    };
    evaluator(&scratch.0, Box::new(agent))
        .evaluate(&fixture::graph())
        .unwrap();
    let path = scratch.0.join("answers/clusters.json");
    let text = std::fs::read_to_string(&path)
        .unwrap()
        .replace("\"Storage\"", "\"Disks\"");
    std::fs::write(&path, text).unwrap();
    let error = evaluator(&scratch.0, Box::new(Failing))
        .evaluate(&fixture::graph())
        .unwrap_err();
    assert!(
        error.message.contains("have problems") && error.message.contains("cluster \"Storage\"")
    );
}
