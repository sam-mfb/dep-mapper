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
        _choice: &EvaluatorChoice,
        _selection: &BuildSelection,
    ) -> Result<Box<dyn graph_evaluator::GraphEvaluator>, Unavailable> {
        Ok(Box::new(FixedEvaluator(evaluation())))
    }
}

#[derive(Default)]
struct Recorded {
    notes: Vec<String>,
    files: Vec<(PathBuf, String)>,
    served: Vec<(SocketAddr, String)>,
}

impl MapOutputs for Recorded {
    fn note(&mut self, message: &str) {
        self.notes.push(message.into());
    }

    fn write_file(&mut self, path: &Path, text: &str) -> std::io::Result<()> {
        self.files.push((path.into(), text.into()));
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
        evaluator: EvaluatorChoice::Llm,
        out: "map.toml".into(),
        page: Some("map.html".into()),
        serve: None,
    }
}

#[test]
fn map_saves_the_map_writes_the_page_and_notes_what_the_builder_could_not_match() {
    let mut outputs = Recorded::default();
    map(&command(), &StandIns { graphers: true }, &mut outputs).unwrap();
    let [(saved_path, saved), (page_path, page)] = outputs.files.as_slice() else {
        panic!("the map and its page are written")
    };
    assert_eq!(saved_path, Path::new("map.toml"));
    let built = build(&command(), &StandIns { graphers: true }).unwrap();
    assert_eq!(map_persist::load(saved), Ok(MapDocument::Map(built.map)));
    assert_eq!(page_path, Path::new("map.html"));
    assert!(page.contains("sample-project"));
    assert_eq!(
        outputs.notes,
        [
            "the evaluation judges module missing, which the graph does not have",
            "saved the map to map.toml",
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
    assert_eq!(outputs.files.len(), 2);
    assert_eq!(outputs.served.len(), 1);
    assert_eq!(outputs.served[0].1, outputs.files[1].1);
}

#[test]
fn without_a_page_or_address_the_map_is_only_saved() {
    let mut command = command();
    command.page = None;
    let mut outputs = Recorded::default();
    map(&command, &StandIns { graphers: true }, &mut outputs).unwrap();
    assert_eq!(outputs.files.len(), 1);
    assert!(outputs.served.is_empty());
}

#[test]
fn an_unavailable_grapher_stops_the_command() {
    let mut outputs = Recorded::default();
    let error = map(&command(), &StandIns { graphers: false }, &mut outputs).unwrap_err();
    assert_eq!(error.to_string(), "no Rust grapher");
    assert!(outputs.files.is_empty());
}

#[test]
fn a_grapher_failure_stops_the_command() {
    let mut command = command();
    command.selection.target = "bin".into();
    let error = build(&command, &StandIns { graphers: true }).unwrap_err();
    assert!(matches!(error, MapError::Graphing(_)));
}
