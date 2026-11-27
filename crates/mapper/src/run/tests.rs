use super::*;
use graph_evaluator::testing::FixedEvaluator;
use graph_evaluator::{Clusters, Decision, GraphEvaluation};
use project_graph::testing::FixedGrapher;
use project_graph::{Build, GraphId, Module, ModuleId, ProjectGraph, ProjectGrapher};
use std::cell::RefCell;
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

/// Stand-ins for the graphers and evaluators still being built. Keeps the
/// cluster plan each evaluator was built with.
#[derive(Default)]
struct StandIns {
    graphers: bool,
    plans: RefCell<Vec<LayerPlan>>,
}

fn stand_ins(graphers: bool) -> StandIns {
    StandIns {
        graphers,
        plans: RefCell::default(),
    }
}

impl Implementations for StandIns {
    fn grapher(&self, language: &Language) -> Result<Box<dyn ProjectGrapher>, Unavailable> {
        match (self.graphers, language) {
            (true, Language::Rust { .. }) => Ok(Box::new(FixedGrapher(graph()))),
            _ => Err(Unavailable("no grapher".into())),
        }
    }

    fn evaluator(
        &self,
        _choice: &EvaluatorChoice,
        _selection: &BuildSelection,
        layers: &LayerPlan,
    ) -> Result<Box<dyn graph_evaluator::GraphEvaluator>, Unavailable> {
        self.plans.borrow_mut().push(layers.clone());
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

/// Files in memory, for a draft that gives the layers.
#[derive(Default)]
struct Files(BTreeMap<PathBuf, String>);

impl DraftFiles for Files {
    fn read(&self, path: &Path) -> std::io::Result<String> {
        self.0
            .get(path)
            .cloned()
            .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "no such file"))
    }

    fn write(&mut self, _path: &Path, _text: &str) -> std::io::Result<()> {
        Err(std::io::Error::other("srs map writes no draft files"))
    }

    fn note(&mut self, _message: &str) {}
}

fn no_files() -> Files {
    Files::default()
}

fn command() -> MapCommand {
    MapCommand {
        selection: BuildSelection {
            root: "/src/sample".into(),
            target: "lib".into(),
            configuration: "default".into(),
        },
        language: Language::Rust {
            cache: "/cache".into(),
            excluded: vec![],
        },
        evaluator: EvaluatorChoice::Llm,
        layers: LayerSource::Proposed,
        out: "map.toml".into(),
        page: Some("map.html".into()),
        serve: None,
    }
}

#[test]
fn map_saves_the_map_writes_the_page_and_notes_what_the_builder_could_not_match() {
    let mut outputs = Recorded::default();
    map(&command(), &stand_ins(true), &no_files(), &mut outputs).unwrap();
    let [(saved_path, saved), (page_path, page)] = outputs.files.as_slice() else {
        panic!("the map and its page are written")
    };
    assert_eq!(saved_path, Path::new("map.toml"));
    let built = build(&command(), &stand_ins(true), &no_files()).unwrap();
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
    let error = map(&command, &stand_ins(true), &no_files(), &mut outputs).unwrap_err();
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
    map(&command, &stand_ins(true), &no_files(), &mut outputs).unwrap();
    assert_eq!(outputs.files.len(), 1);
    assert!(outputs.served.is_empty());
}

#[test]
fn an_unavailable_grapher_stops_the_command() {
    let mut outputs = Recorded::default();
    let error = map(&command(), &stand_ins(false), &no_files(), &mut outputs).unwrap_err();
    assert_eq!(error.to_string(), "no grapher");
    assert!(outputs.files.is_empty());
}

#[test]
fn a_grapher_failure_stops_the_command() {
    let mut command = command();
    command.selection.target = "bin".into();
    let error = build(&command, &stand_ins(true), &no_files()).unwrap_err();
    assert!(matches!(error, MapError::Graphing(_)));
}

fn plan_for(layers: LayerSource, files: &Files) -> Result<LayerPlan, MapError> {
    let mut command = command();
    command.layers = layers;
    let implementations = stand_ins(true);
    build(&command, &implementations, files)?;
    Ok(implementations.plans.borrow()[0].clone())
}

#[test]
fn without_a_layer_source_the_evaluator_proposes_the_layers() {
    assert_eq!(
        plan_for(LayerSource::Proposed, &no_files()).unwrap(),
        LayerPlan::Proposed
    );
}

#[test]
fn listed_layers_are_fixed_without_hints() {
    let plan = plan_for(
        LayerSource::Listed(vec!["Program".into(), "Base".into()]),
        &no_files(),
    )
    .unwrap();
    assert_eq!(
        plan,
        LayerPlan::Fixed {
            layers: vec!["Program".into(), "Base".into()],
            previous: BTreeMap::new(),
        }
    );
}

/// A saved base map with two layers and two judged modules, and a draft over
/// it that adds a layer and moves one module into it.
fn draft_files() -> Files {
    let base = r#"format = 2
kind = "map"
project = "sample"
layers = ["program", "base"]

[modules.app]
name = "app"
status = "observed"
review = { state = "judged", concern = "Runs the program.", kind = "composition", layer = "program", reason = "r", primary = "note:a" }

[modules.core]
name = "core"
status = "observed"
review = { state = "judged", concern = "Computes results.", kind = "logic", layer = "program", reason = "r", primary = "note:a" }

[modules.loose]
name = "loose"
status = "observed"
review = { state = "unreviewed" }

[evidence."note:a"]
kind = "source"
summary = "Found in the source."
"#;
    let draft = r#"format = 2
kind = "draft"
base = "base.toml"
layers = ["foundation"]

[update.modules.core]
layer = "foundation"
reason = "It uses nothing else."
primary = "note:b"

[evidence."note:b"]
kind = "proposal"
summary = "A proposal."
"#;
    Files(BTreeMap::from([
        (PathBuf::from("plans/base.toml"), base.to_string()),
        (PathBuf::from("plans/draft.toml"), draft.to_string()),
    ]))
}

#[test]
fn a_draft_fixes_the_combined_layers_and_hints_each_modules_planned_layer() {
    let plan = plan_for(
        LayerSource::Draft("plans/draft.toml".into()),
        &draft_files(),
    )
    .unwrap();
    assert_eq!(
        plan,
        LayerPlan::Fixed {
            layers: vec!["program".into(), "base".into(), "foundation".into()],
            previous: BTreeMap::from([
                ("app".to_string(), "program".to_string()),
                ("core".to_string(), "foundation".to_string()),
            ]),
        }
    );
}

#[test]
fn a_draft_with_problems_stops_the_command_before_analysis() {
    let mut files = draft_files();
    files.0.insert(
        "plans/draft.toml".into(),
        "format = 2\nkind = \"draft\"\nbase = \"missing.toml\"\n".into(),
    );
    let mut command = command();
    command.layers = LayerSource::Draft("plans/draft.toml".into());
    let implementations = stand_ins(true);
    let error = build(&command, &implementations, &files).unwrap_err();
    assert!(matches!(&error, MapError::Layers(_)), "{error}");
    assert!(error.to_string().contains("missing.toml"), "{error}");
    assert!(implementations.plans.borrow().is_empty());
}
