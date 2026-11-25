use super::*;
use graph_evaluator::testing::FixedEvaluator;
use graph_evaluator::{Clusters, Decision, GraphEvaluation};
use project_graph::testing::FixedGrapher;
use project_graph::{Build, GraphId, Module, ModuleId, ProjectGraph, ProjectGrapher};
use std::collections::BTreeMap;
use std::net::SocketAddr;
use std::path::Path;

fn graph() -> ProjectGraph {
    ProjectGraph {
        id: GraphId("sample@1".into()),
        build: Build {
            project: "sample-project".into(),
            source_revision: "1".into(),
            target: "lib".into(),
            configuration: "default".into(),
        },
        modules: BTreeMap::from([(
            ModuleId("core".into()),
            Module {
                name: "core".into(),
                root: ".".into(),
            },
        )]),
        units: BTreeMap::new(),
        items: BTreeMap::new(),
        facts: BTreeMap::new(),
        routes: BTreeMap::new(),
        gaps: vec![],
    }
}

fn evaluation() -> GraphEvaluation {
    GraphEvaluation {
        graph: GraphId("sample@1".into()),
        clusters: Clusters::empty(),
        modules: BTreeMap::from([(
            ModuleId("missing".into()),
            Decision::Deferred {
                reason: "not sure".into(),
            },
        )]),
        relationships: vec![],
    }
}

/// Stand-ins for the graphers and evaluators still being built.
struct StandIns {
    graphers: bool,
}

impl Implementations for StandIns {
    fn grapher(&self, language: Language) -> Result<Box<dyn ProjectGrapher>, Unavailable> {
        match (self.graphers, language) {
            (true, Language::Rust) => Ok(Box::new(FixedGrapher(graph()))),
            _ => Err(Unavailable(format!("no {language:?} grapher"))),
        }
    }

    fn evaluator(
        &self,
        _choice: EvaluatorChoice,
    ) -> Result<Box<dyn graph_evaluator::GraphEvaluator>, Unavailable> {
        Ok(Box::new(FixedEvaluator(evaluation())))
    }
}

#[derive(Default)]
struct Recorded {
    notes: Vec<String>,
    pages: Vec<(PathBuf, String)>,
    served: Vec<(SocketAddr, String)>,
}

impl MapOutputs for Recorded {
    fn note(&mut self, message: &str) {
        self.notes.push(message.into());
    }

    fn write_page(&mut self, path: &Path, page: &str) -> std::io::Result<()> {
        self.pages.push((path.into(), page.into()));
        Ok(())
    }

    fn serve_page(&mut self, address: SocketAddr, page: &str) -> std::io::Error {
        self.served.push((address, page.into()));
        std::io::Error::other("stopped")
    }
}

fn command() -> MapCommand {
    MapCommand {
        selection: BuildSelection {
            root: "/src/sample".into(),
            target: "lib".into(),
            configuration: "default".into(),
        },
        language: Language::Rust,
        evaluator: EvaluatorChoice::Manual,
        page: Some("map.html".into()),
        serve: None,
    }
}

#[test]
fn map_writes_the_page_and_notes_what_the_builder_could_not_match() {
    let mut outputs = Recorded::default();
    map(&command(), &StandIns { graphers: true }, &mut outputs).unwrap();
    let [(path, page)] = outputs.pages.as_slice() else {
        panic!("one page is written")
    };
    assert_eq!(path, Path::new("map.html"));
    assert!(page.contains("sample-project"));
    assert_eq!(
        outputs.notes,
        [
            "the evaluation judges module missing, which the graph does not have",
            "wrote the map page to map.html",
        ]
    );
    assert!(outputs.served.is_empty());
}

#[test]
fn serving_runs_after_writing_and_reports_why_it_stopped() {
    let mut command = command();
    let address: SocketAddr = "127.0.0.1:8080".parse().unwrap();
    command.serve = Some(address);
    let mut outputs = Recorded::default();
    let error = map(&command, &StandIns { graphers: true }, &mut outputs).unwrap_err();
    assert!(matches!(error, MapError::Serve { address: a, .. } if a == address));
    assert_eq!(outputs.pages.len(), 1);
    assert_eq!(outputs.served.len(), 1);
    assert_eq!(outputs.served[0].1, outputs.pages[0].1);
}

#[test]
fn a_command_with_nowhere_to_show_the_map_does_nothing() {
    let mut command = command();
    command.page = None;
    let mut outputs = Recorded::default();
    let error = map(&command, &StandIns { graphers: true }, &mut outputs).unwrap_err();
    assert!(matches!(error, MapError::NothingToShow));
    assert!(outputs.notes.is_empty());
}

#[test]
fn an_unavailable_grapher_stops_the_command() {
    let mut outputs = Recorded::default();
    let error = map(&command(), &StandIns { graphers: false }, &mut outputs).unwrap_err();
    assert_eq!(error.to_string(), "no Rust grapher");
    assert!(outputs.pages.is_empty());
}

#[test]
fn a_grapher_failure_stops_the_command() {
    let mut command = command();
    command.selection.target = "bin".into();
    let error = build(&command, &StandIns { graphers: true }).unwrap_err();
    assert!(matches!(error, MapError::Graphing(_)));
}
