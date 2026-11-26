use super::*;
use crate::model::{AssistantTurn, ModelError};
use crate::source::{Excerpt, SourceError, locate};
use graph_evaluator::{ClusterName, Decision, ModuleKind, RelationshipKind};
use graph_evaluator_pieces::testing;
use project_graph::{ModuleId, RouteId, SourceSpan};
use serde_json::json;
use std::cell::RefCell;

struct SameText;

impl SourceText for SameText {
    fn excerpt(&self, span: &SourceSpan) -> Result<Excerpt, SourceError> {
        locate(
            "line one.\nline two.\nline 333.\nline 444.\n",
            &span.bytes,
            2,
        )
    }
}

/// Answers each request with a closure and keeps every request it saw.
struct FakeModel<F> {
    answer: F,
    requests: RefCell<Vec<ModelRequest>>,
}

type Reply = Result<ModelReply, ModelError>;

impl<F: Fn(&ModelRequest) -> Reply> FakeModel<F> {
    fn new(answer: F) -> Self {
        FakeModel {
            answer,
            requests: RefCell::new(vec![]),
        }
    }

    fn first_turns(&self) -> Vec<String> {
        self.requests
            .borrow()
            .iter()
            .map(|r| first_turn(r).to_string())
            .collect()
    }
}

impl<F: Fn(&ModelRequest) -> Reply> ModelClient for &FakeModel<F> {
    fn reply(&self, request: &ModelRequest) -> Reply {
        self.requests.borrow_mut().push(request.clone());
        (self.answer)(request)
    }
}

#[derive(Default)]
struct RecordedLog(RefCell<Vec<LogEvent>>);

impl EvaluationLog for &RecordedLog {
    fn record(&self, event: &LogEvent) {
        self.0.borrow_mut().push(event.clone());
    }
}

fn answer(value: serde_json::Value) -> Reply {
    Ok(ModelReply::Answer(AssistantTurn {
        text: value.to_string(),
        provider_record: None,
    }))
}

fn first_turn(request: &ModelRequest) -> &str {
    match &request.turns[0] {
        Turn::User(text) => text,
        Turn::Assistant(_) => panic!("conversations start with the user"),
    }
}

/// The module a module piece is about, from its heading `Module x (module:x)`.
fn piece_module(request: &ModelRequest) -> Option<&str> {
    let text = first_turn(request);
    if !text.starts_with("Module piece") {
        return None;
    }
    text.split("(module:").nth(1)?.split(')').next()
}

/// The routes a routes page lists, from its `- route:<id>` lines.
fn routes_in(text: &str) -> Vec<String> {
    text.lines()
        .filter_map(|line| line.strip_prefix("- "))
        .filter(|rest| rest.starts_with("route:") && !rest.contains(' '))
        .map(String::from)
        .collect()
}

/// Judges every module, moves `store` into `disk`'s cluster, and finds one
/// relationship per routes page.
fn sensible(request: &ModelRequest) -> Reply {
    let text = first_turn(request);
    if let Some(module) = piece_module(request) {
        let (kind, cluster) = match module {
            "app" => ("composition", "Program"),
            "disk" => ("effect", "Storage"),
            _ => ("contract", "Contracts"),
        };
        return answer(json!({"answer": {
            "decision": "judge", "concern": format!("The {module} module."), "kind": kind,
            "cluster": cluster, "reason": "Its items say so.", "evidence": [format!("item:{module}::{}", match module { "app" => "main", "disk" => "DiskStore", _ => "Store" })],
        }}));
    }
    if text.starts_with("Clusters piece") {
        return answer(json!({"clusters": [
            {"name": "Program", "modules": ["module:app"]},
            {"name": "Storage", "modules": ["module:disk", "store"]},
        ]}));
    }
    let routes = routes_in(text);
    let kind = if routes == ["route:r-impl"] {
        "implements"
    } else {
        "direct-use"
    };
    answer(json!({
        "relationships": [{"kind": kind, "other_kind": "", "reason": "The route shows it.", "evidence": routes}],
        "set_aside": [],
    }))
}

fn evaluate_with<F: Fn(&ModelRequest) -> Reply>(
    model: &FakeModel<F>,
    log: &RecordedLog,
    settings: EvaluatorSettings,
    graph: &ProjectGraph,
) -> Result<GraphEvaluation, EvaluationError> {
    LlmEvaluator::new(model, SameText, log, settings).evaluate(graph)
}

fn evaluate<F: Fn(&ModelRequest) -> Reply>(
    model: &FakeModel<F>,
    log: &RecordedLog,
) -> Result<GraphEvaluation, EvaluationError> {
    evaluate_with(
        model,
        log,
        EvaluatorSettings::recommended(),
        &testing::graph(),
    )
}

#[test]
fn judges_modules_settles_clusters_and_relates_each_routes_piece() {
    let model = FakeModel::new(sensible);
    let log = RecordedLog::default();
    let evaluation = evaluate(&model, &log).unwrap();

    assert_eq!(evaluation.graph, testing::graph().id);
    let names: Vec<_> = evaluation.clusters.iter().map(|c| c.0.as_str()).collect();
    assert_eq!(names, ["Program", "Storage"]);
    let Decision::Judged { judgment, .. } = &evaluation.modules[&ModuleId("store".into())] else {
        panic!("store should be judged");
    };
    assert_eq!(judgment.kind, ModuleKind::Contract);
    assert_eq!(
        judgment.cluster,
        ClusterName("Storage".into()),
        "moved by the clusters piece"
    );
    let mut kinds: Vec<_> = evaluation
        .relationships
        .iter()
        .map(|r| r.kind.clone())
        .collect();
    kinds.sort_by_key(|kind| format!("{kind:?}"));
    assert_eq!(
        kinds,
        [
            RelationshipKind::DirectUse,
            RelationshipKind::DirectUse,
            RelationshipKind::Implements
        ]
    );
    // Three modules, the clusters piece, three routes pieces, in that order.
    let turns = model.first_turns();
    assert_eq!(turns.len(), 7);
    assert!(turns[..3].iter().all(|t| t.starts_with("Module piece")));
    assert!(turns[3].starts_with("Clusters piece"));
    assert!(turns[4..].iter().all(|t| t.starts_with("Routes piece")));
}

#[test]
fn every_request_carries_the_instructions_and_the_overview() {
    let model = FakeModel::new(sensible);
    let log = RecordedLog::default();
    evaluate(&model, &log).unwrap();
    let requests = model.requests.borrow();
    assert!(requests.iter().all(|r| r.system == requests[0].system));
    assert!(
        requests[0]
            .system
            .starts_with("You are reviewing the architecture")
    );
    assert!(
        requests[0]
            .system
            .contains("- module:store \"store\" at crates/store")
    );
}

#[test]
fn pieces_carry_source_excerpts_and_the_decisions_so_far() {
    let model = FakeModel::new(sensible);
    let log = RecordedLog::default();
    evaluate(&model, &log).unwrap();
    let turns = model.first_turns();
    assert!(
        turns[0]
            .contains("Source excerpts:\nitem:app::main at crates/app/src/lib.rs:2-2\n| line two."),
        "{}",
        turns[0]
    );
    assert!(turns[0].contains("Route details:"), "{}", turns[0]);
    assert!(
        turns[3].contains(
            "- module:store (contract, proposed cluster \"Contracts\"): The store module."
        ),
        "{}",
        turns[3]
    );
    let wire = turns
        .iter()
        .find(|t| t.starts_with("Routes piece") && routes_in(t) == ["route:r-wire"])
        .unwrap();
    assert!(
        wire.contains("- module:disk (effect, cluster \"Storage\")"),
        "after the clusters piece the cluster is settled: {wire}"
    );
    let disk_store = turns
        .iter()
        .find(|t| t.starts_with("Routes piece") && routes_in(t) == ["route:r-impl"])
        .unwrap();
    assert!(
        disk_store.contains("- module:store (contract, cluster \"Storage\")"),
        "moved by the clusters piece: {disk_store}"
    );
}

#[test]
fn a_refused_answer_goes_back_with_its_problems_and_the_repair_is_kept() {
    let model = FakeModel::new(|request: &ModelRequest| {
        if piece_module(request) == Some("disk") && request.turns.len() == 1 {
            return answer(
                json!({"answer": {"decision": "judge", "concern": "x", "kind": "effect", "cluster": "Storage", "reason": "y", "evidence": ["route:nope"]}}),
            );
        }
        sensible(request)
    });
    let log = RecordedLog::default();
    let evaluation = evaluate(&model, &log).unwrap();
    assert!(matches!(
        evaluation.modules[&ModuleId("disk".into())],
        Decision::Judged { ref reason, .. } if reason == "Its items say so."
    ));
    let requests = model.requests.borrow();
    let repair = requests
        .iter()
        .find(|r| r.turns.len() == 3)
        .expect("a repair request");
    assert!(matches!(&repair.turns[1], Turn::Assistant(turn) if turn.text.contains("route:nope")));
    assert!(
        matches!(&repair.turns[2], Turn::User(text) if text.contains("route:nope")),
        "{:?}",
        repair.turns[2]
    );
    let repairing: Vec<_> = log
        .0
        .borrow()
        .iter()
        .filter_map(|e| match e {
            LogEvent::Repairing { piece, .. } => Some(piece.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(repairing, ["module:disk"]);
}

#[test]
fn an_answer_still_refused_after_the_repairs_fails_with_its_problems() {
    let model = FakeModel::new(|request: &ModelRequest| {
        if first_turn(request).starts_with("Clusters piece") {
            return answer(
                json!({"clusters": [{"name": "All", "modules": ["module:app", "module:disk"]}]}),
            );
        }
        sensible(request)
    });
    let log = RecordedLog::default();
    let error = evaluate(&model, &log).unwrap_err();
    assert_eq!(
        error.message,
        "clusters: the answer was still refused after 2 repairs: module:store is in no cluster"
    );
    let asked = model
        .first_turns()
        .iter()
        .filter(|t| t.starts_with("Clusters piece"))
        .count();
    assert_eq!(asked, 3);
}

#[test]
fn text_that_does_not_follow_the_schema_is_repaired() {
    let model = FakeModel::new(|request: &ModelRequest| {
        if piece_module(request) == Some("app") && request.turns.len() == 1 {
            return Ok(ModelReply::Answer(AssistantTurn {
                text: "I think it is composition.".into(),
                provider_record: None,
            }));
        }
        sensible(request)
    });
    let log = RecordedLog::default();
    evaluate(&model, &log).unwrap();
    assert!(log.0.borrow().iter().any(|e| matches!(
        e,
        LogEvent::Repairing { piece, problems } if piece == "module:app" && problems[0].starts_with("the answer does not follow its schema")
    )));
}

#[test]
fn other_relationships_carry_their_name_and_need_one() {
    let model = FakeModel::new(|request: &ModelRequest| {
        let text = first_turn(request);
        if text.starts_with("Routes piece") && routes_in(text) == ["route:r-write"] {
            let other = if request.turns.len() == 1 {
                ""
            } else {
                "persists"
            };
            return answer(json!({
                "relationships": [{"kind": "other", "other_kind": other, "reason": "It writes a file.", "evidence": ["route:r-write"]}],
                "set_aside": [],
            }));
        }
        sensible(request)
    });
    let log = RecordedLog::default();
    let evaluation = evaluate(&model, &log).unwrap();
    assert!(
        evaluation
            .relationships
            .iter()
            .any(|r| r.kind == RelationshipKind::Other("persists".into()))
    );
    assert!(log.0.borrow().contains(&LogEvent::Repairing {
        piece: "routes/app".into(),
        problems: vec!["relationship 1: kind other needs other_kind".into()],
    }));
}

#[test]
fn a_route_can_be_set_aside_instead_of_related() {
    let model = FakeModel::new(|request: &ModelRequest| {
        let text = first_turn(request);
        if text.starts_with("Routes piece") && routes_in(text) == ["route:r-write"] {
            return answer(json!({
                "relationships": [],
                "set_aside": [{"route": "route:r-write", "reason": "A plain file write."}],
            }));
        }
        sensible(request)
    });
    let log = RecordedLog::default();
    let evaluation = evaluate(&model, &log).unwrap();
    assert_eq!(evaluation.relationships.len(), 2);
}

#[test]
fn warnings_are_logged_and_the_answer_kept() {
    let model = FakeModel::new(|request: &ModelRequest| {
        let text = first_turn(request);
        if text.starts_with("Routes piece") && routes_in(text) == ["route:r-write"] {
            return answer(json!({
                "relationships": [{"kind": "incidental", "other_kind": "", "reason": "It writes a file.", "evidence": ["fact:f-write"]}],
                "set_aside": [],
            }));
        }
        sensible(request)
    });
    let log = RecordedLog::default();
    let evaluation = evaluate(&model, &log).unwrap();
    assert_eq!(evaluation.relationships.len(), 3);
    assert!(
        log.0
            .borrow()
            .iter()
            .any(|e| matches!(e, LogEvent::Warning { piece, .. } if piece == "routes/app"))
    );
}

#[test]
fn a_refusal_truncation_or_model_error_fails_with_the_piece_named() {
    let cases: [(Reply, &str); 3] = [
        (
            Ok(ModelReply::Refused {
                explanation: "cyber: no".into(),
            }),
            "module:app: the model declined: cyber: no",
        ),
        (
            Ok(ModelReply::Truncated),
            "module:app: the answer exceeded the output limit",
        ),
        (
            Err(ModelError::Permanent {
                message: "bad key".into(),
            }),
            "module:app: model request failed: bad key",
        ),
    ];
    for (reply, expected) in cases {
        let model = FakeModel::new(move |_: &ModelRequest| reply.clone());
        let log = RecordedLog::default();
        assert_eq!(evaluate(&model, &log).unwrap_err().message, expected);
    }
}

#[test]
fn with_no_judged_modules_the_clusters_piece_is_skipped() {
    let model = FakeModel::new(|request: &ModelRequest| {
        if piece_module(request).is_some() {
            return answer(
                json!({"answer": {"decision": "defer", "reason": "Not enough evidence."}}),
            );
        }
        sensible(request)
    });
    let log = RecordedLog::default();
    let evaluation = evaluate(&model, &log).unwrap();
    assert!(evaluation.clusters.is_empty());
    assert!(
        model
            .first_turns()
            .iter()
            .all(|t| !t.starts_with("Clusters piece"))
    );
}

#[test]
fn long_routes_pieces_are_asked_page_by_page() {
    let mut graph = testing::graph();
    let wire = graph.routes[&RouteId("r-wire".into())].clone();
    graph.routes.insert(RouteId("r-wire-2".into()), wire);
    let model = FakeModel::new(sensible);
    let log = RecordedLog::default();
    let settings = EvaluatorSettings {
        routes_per_page: 1,
        ..EvaluatorSettings::recommended()
    };
    evaluate_with(&model, &log, settings, &graph).unwrap();
    let asked: Vec<String> = log
        .0
        .borrow()
        .iter()
        .filter_map(|event| match event {
            LogEvent::Asking { piece } if piece.starts_with("routes/app+disk") => {
                Some(piece.clone())
            }
            _ => None,
        })
        .collect();
    assert_eq!(asked, ["routes/app+disk page 1", "routes/app+disk page 2"]);
}

#[test]
fn a_module_piece_with_several_pages_is_asked_whole() {
    let mut graph = testing::graph();
    let main = graph.items[&project_graph::ItemId("app::main".into())].clone();
    for n in 0..3 {
        let mut item = main.clone();
        item.name = format!("helper{n}");
        graph
            .items
            .insert(project_graph::ItemId(format!("app::helper{n}")), item);
    }
    let model = FakeModel::new(sensible);
    let log = RecordedLog::default();
    let settings = EvaluatorSettings {
        routes_per_page: 1,
        ..EvaluatorSettings::recommended()
    };
    evaluate_with(&model, &log, settings, &graph).unwrap();
    let turns = model.first_turns();
    let app = turns
        .iter()
        .find(|t| t.starts_with("Module piece") && t.contains("(module:app)"))
        .unwrap();
    for handle in [
        "item:app::main",
        "item:app::helper0",
        "item:app::helper1",
        "item:app::helper2",
        "route:r-wire",
        "route:r-write",
    ] {
        assert!(app.contains(handle), "{handle} missing from\n{app}");
    }
    let asked = log
        .0
        .borrow()
        .iter()
        .filter(|e| matches!(e, LogEvent::Asking { piece } if piece == "module:app"))
        .count();
    assert_eq!(asked, 1, "one request for the whole module");
}

#[test]
fn the_model_is_told_which_routes_were_left_out_of_the_graph() {
    let mut graph = testing::graph();
    route_exclusion::exclude(&mut graph, &["std".to_string()], |name| {
        name.starts_with("std::")
    });
    let model = FakeModel::new(sensible);
    let log = RecordedLog::default();
    evaluate_with(&model, &log, EvaluatorSettings::recommended(), &graph).unwrap();
    let modules: Vec<String> = model
        .first_turns()
        .into_iter()
        .filter(|t| t.starts_with("Module piece"))
        .collect();
    assert_eq!(modules.len(), 3);
    for turn in modules {
        assert!(
            turn.contains("Routes that start in the excluded packages std are left out: 0 routes and 0 facts removed."),
            "{turn}"
        );
    }
}
