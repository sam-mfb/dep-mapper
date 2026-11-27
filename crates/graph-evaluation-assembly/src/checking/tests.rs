use super::*;
use crate::answers::PieceAnswers;
use crate::testing;
use crate::workbook::{ByteOffsets, ClusterPlan, Presentation, present};
use std::collections::BTreeMap;

fn workbook() -> Workbook {
    present(
        &testing::graph(),
        &Presentation {
            page_size: 10,
            source: &ByteOffsets,
            command: Some("srs eval"),
            clusters: &ClusterPlan::proposed(),
        },
    )
}

fn strings(items: &[&str]) -> Vec<String> {
    items.iter().map(|s| s.to_string()).collect()
}

fn judged(kind: &str, cluster: &str, cite: &[&str]) -> ModuleAnswer {
    ModuleAnswer::Judged {
        concern: "A concern.".into(),
        kind: kind.into(),
        cluster: cluster.into(),
        reason: "A reason.".into(),
        cite: strings(cite),
    }
}

fn relationship(kind: &str, cite: &[&str]) -> RelationshipAnswer {
    RelationshipAnswer {
        kind: kind.into(),
        reason: "A reason.".into(),
        cite: strings(cite),
    }
}

fn piece<'a>(answers: &'a mut Answers, id: &str) -> &'a mut PieceAnswers {
    answers.entry(PieceId(id.into())).or_default()
}

fn rules(findings: &[Finding]) -> Vec<Rule> {
    findings.iter().map(|f| f.rule).collect()
}

/// Answers covering every module, the clusters and every route.
fn complete() -> Answers {
    let mut answers = Answers::new();
    piece(&mut answers, "module/app").modules.insert(
        "app".into(),
        judged("composition", "Program", &["route:r-wire"]),
    );
    piece(&mut answers, "module/disk").modules.insert(
        "disk".into(),
        judged("effect", "Storage", &["route:r-impl"]),
    );
    piece(&mut answers, "module/store").modules.insert(
        "store".into(),
        ModuleAnswer::Deferred {
            reason: "Unclear.".into(),
        },
    );
    piece(&mut answers, "clusters").clusters = Some(strings(&["Program", "Storage"]));
    piece(&mut answers, "routes/disk+store")
        .relationships
        .insert(
            "routes/disk+store#1".into(),
            relationship("implements", &["route:r-impl", "fact:f-impl"]),
        );
    piece(&mut answers, "routes/app+disk").relationships.insert(
        "routes/app+disk#1".into(),
        relationship("full-injection", &["route:r-wire"]),
    );
    piece(&mut answers, "routes/app")
        .set_aside
        .insert("route:r-write".into(), "Incidental file write.".into());
    answers
}

#[test]
fn complete_answers_assemble_into_the_evaluation_with_graph_references() {
    let assembled = assemble(&workbook(), &complete()).unwrap();
    assert!(assembled.warnings.is_empty());
    let evaluation = assembled.evaluation;
    assert_eq!(evaluation.graph, GraphId("fixture@1".into()));
    assert_eq!(
        evaluation
            .clusters
            .iter()
            .map(|c| c.0.as_str())
            .collect::<Vec<_>>(),
        ["Program", "Storage"]
    );
    assert_eq!(
        evaluation.modules[&ModuleId("disk".into())],
        Decision::Judged {
            judgment: ModuleJudgment {
                concern: "A concern.".into(),
                kind: ModuleKind::Effect,
                cluster: ClusterName("Storage".into()),
            },
            reason: "A reason.".into(),
            evidence: vec![GraphRef::from(project_graph::RouteId("r-impl".into()))],
        }
    );
    assert_eq!(
        evaluation.modules[&ModuleId("store".into())],
        Decision::Deferred {
            reason: "Unclear.".into()
        }
    );
    assert_eq!(evaluation.relationships.len(), 2);
    let graph = testing::graph();
    let cited = evaluation.relationships.iter().flat_map(|r| &r.evidence);
    assert!(cited.into_iter().all(|r| graph.resolve(r).is_some()));
}

#[test]
fn any_problem_stops_the_evaluation_and_every_problem_is_reported() {
    let mut answers = complete();
    piece(&mut answers, "module/app")
        .modules
        .insert("app".into(), judged("glue", "Program", &[]));
    piece(&mut answers, "routes/app").relationships.insert(
        "routes/app#1".into(),
        relationship("other:", &["route:r-write", "fact:"]),
    );
    let problems = assemble(&workbook(), &answers).unwrap_err();
    assert_eq!(
        rules(&problems),
        [
            Rule::UnknownModuleKind,
            Rule::NoCitations,
            Rule::UnknownRelationshipKind,
            Rule::MalformedHandle
        ]
    );
}

#[test]
fn empty_texts_are_problems() {
    let mut answers = complete();
    piece(&mut answers, "module/app").modules.insert(
        "app".into(),
        ModuleAnswer::Judged {
            concern: " ".into(),
            kind: "logic".into(),
            cluster: "Program".into(),
            reason: String::new(),
            cite: strings(&["route:r-wire"]),
        },
    );
    piece(&mut answers, "module/store").modules.insert(
        "store".into(),
        ModuleAnswer::Deferred {
            reason: String::new(),
        },
    );
    piece(&mut answers, "routes/app")
        .set_aside
        .insert("route:r-write".into(), String::new());
    let review = review(&workbook(), &answers);
    assert_eq!(rules(&review.problems), [Rule::EmptyText; 4]);
}

#[test]
fn answers_in_the_wrong_place_are_problems() {
    let mut answers = complete();
    piece(&mut answers, "module/app").modules.insert(
        "disk".into(),
        judged("effect", "Storage", &["route:r-impl"]),
    );
    piece(&mut answers, "module/app").clusters = Some(strings(&["Program"]));
    piece(&mut answers, "routes/app")
        .set_aside
        .insert("route:r-impl".into(), "Wrong piece.".into());
    piece(&mut answers, "elsewhere")
        .modules
        .insert("app".into(), judged("logic", "Program", &["route:r-wire"]));
    let review = review(&workbook(), &answers);
    assert_eq!(
        rules(&review.problems),
        [
            Rule::UnknownSubject,
            Rule::WrongPiece,
            Rule::WrongPiece,
            Rule::WrongPiece
        ]
    );
}

#[test]
fn the_cluster_list_must_exist_once_a_module_is_judged_and_must_not_repeat() {
    let mut answers = complete();
    answers.remove(&PieceId::clusters());
    assert_eq!(
        rules(&review(&workbook(), &answers).problems),
        [Rule::MissingClusterList]
    );
    piece(&mut answers, "clusters").clusters =
        Some(strings(&["Program", "Storage", "Program", ""]));
    assert_eq!(
        rules(&review(&workbook(), &answers).problems),
        [Rule::DuplicateCluster, Rule::EmptyText]
    );
}

#[test]
fn deferring_every_module_needs_no_cluster_list() {
    let mut answers = Answers::new();
    piece(&mut answers, "module/app").modules.insert(
        "app".into(),
        ModuleAnswer::Deferred {
            reason: "Later.".into(),
        },
    );
    let assembled = assemble(&workbook(), &answers).unwrap();
    assert!(assembled.evaluation.clusters.is_empty());
}

#[test]
fn relationships_without_routes_or_with_unshown_kinds_are_kept_with_warnings() {
    let mut answers = complete();
    piece(&mut answers, "routes/disk+store")
        .relationships
        .insert(
            "routes/disk+store#2".into(),
            relationship("implements", &["item:disk::DiskStore"]),
        );
    let assembled = assemble(&workbook(), &answers).unwrap();
    assert_eq!(
        rules(&assembled.warnings),
        [Rule::NoRouteCited, Rule::KindNotShown]
    );
    assert_eq!(assembled.evaluation.relationships.len(), 3);
}

#[test]
fn progress_counts_routes_cited_anywhere_or_set_aside() {
    let workbook = workbook();
    let mut answers = Answers::new();
    piece(&mut answers, "routes/disk+store")
        .relationships
        .insert(
            "routes/disk+store#1".into(),
            relationship("implements", &["route:r-impl", "route:r-wire"]),
        );
    let progress: BTreeMap<String, Progress> = progress(&workbook, &answers)
        .into_iter()
        .map(|(id, p)| (id.0, p))
        .collect();
    assert_eq!(progress["module/app"], Progress::NotStarted);
    assert_eq!(progress["clusters"], Progress::NotStarted);
    assert_eq!(progress["routes/disk+store"], Progress::Done);
    assert_eq!(progress["routes/app+disk"], Progress::Done);
    assert_eq!(progress["routes/app"], Progress::NotStarted);
    let complete: Vec<Progress> = super::progress(&workbook, &complete())
        .into_iter()
        .map(|(_, p)| p)
        .collect();
    assert!(complete.iter().all(|p| *p == Progress::Done));
}

#[test]
fn kinds_read_from_their_written_names() {
    assert_eq!(module_kind("contract"), Some(ModuleKind::Contract));
    assert_eq!(module_kind("Contract"), None);
    assert_eq!(
        relationship_kind("adapted-injection"),
        Some(RelationshipKind::AdaptedInjection)
    );
    assert_eq!(
        relationship_kind("other:logging"),
        Some(RelationshipKind::Other("logging".into()))
    );
    assert_eq!(relationship_kind("other: "), None);
}

fn fixed_workbook(clusters: &[&str]) -> Workbook {
    present(
        &testing::graph(),
        &Presentation {
            page_size: 10,
            source: &ByteOffsets,
            command: Some("srs eval"),
            clusters: &ClusterPlan::fixed(strings(clusters), BTreeMap::new()).unwrap(),
        },
    )
}

/// `complete`, with `store` judged in a cluster instead of deferred.
fn store_judged(cluster: &str) -> Answers {
    let mut answers = complete();
    piece(&mut answers, "module/store").modules.insert(
        "store".into(),
        judged("contract", cluster, &["item:store::Store"]),
    );
    answers
}

#[test]
fn a_module_that_uses_a_module_in_a_higher_cluster_is_warned_about() {
    // disk implements store's contract, so disk uses store.
    let mut answers = store_judged("Contracts");
    piece(&mut answers, "clusters").clusters = Some(strings(&["Program", "Contracts", "Storage"]));
    let assembled = assemble(&workbook(), &answers).unwrap();
    assert_eq!(rules(&assembled.warnings), [Rule::UpwardDependency]);
    let warning = &assembled.warnings[0];
    assert_eq!(warning.subject, Subject::Module("disk".into()));
    assert!(warning.message.contains("module:store"));
    assert!(warning.message.contains("fact:f-impl"));
}

#[test]
fn a_module_that_uses_its_own_or_a_lower_cluster_is_not_warned_about() {
    for list in [
        &["Program", "Storage", "Contracts"][..],
        &["Program", "Storage"][..],
    ] {
        let cluster = list.last().unwrap();
        let mut answers = store_judged(cluster);
        piece(&mut answers, "clusters").clusters = Some(strings(list));
        let assembled = assemble(&workbook(), &answers).unwrap();
        assert!(assembled.warnings.is_empty(), "{list:?}");
    }
}

#[test]
fn the_workbook_records_which_module_uses_which_from_use_facts_only() {
    let workbook = workbook();
    // f-impl makes disk use store. f-construct, where app builds a
    // DiskStore, is not a use fact, and f-write leaves the project.
    assert_eq!(
        workbook.uses,
        BTreeMap::from([(
            "disk".to_string(),
            BTreeMap::from([("store".to_string(), "fact:f-impl".to_string())])
        )])
    );
}

#[test]
fn a_fixed_list_is_the_cluster_list_and_needs_no_answer() {
    let workbook = fixed_workbook(&["Program", "Storage", "Unused"]);
    let mut answers = complete();
    answers.remove(&PieceId::clusters());
    let assembled = assemble(&workbook, &answers).unwrap();
    assert!(assembled.warnings.is_empty());
    assert_eq!(
        assembled
            .evaluation
            .clusters
            .iter()
            .map(|c| c.0.as_str())
            .collect::<Vec<_>>(),
        ["Program", "Storage", "Unused"]
    );
    let clusters = progress(&workbook, &answers)
        .into_iter()
        .find(|(id, _)| *id == PieceId::clusters());
    assert_eq!(clusters, Some((PieceId::clusters(), Progress::Done)));
}

#[test]
fn with_a_fixed_list_a_judgment_naming_another_cluster_is_a_problem() {
    let workbook = fixed_workbook(&["Program", "Files"]);
    let mut answers = complete();
    answers.remove(&PieceId::clusters());
    assert_eq!(
        rules(&review(&workbook, &answers).problems),
        [Rule::UnlistedCluster]
    );
}

#[test]
fn with_a_fixed_list_a_recorded_list_is_ignored_with_a_warning() {
    let workbook = fixed_workbook(&["Program", "Storage"]);
    let mut answers = complete();
    piece(&mut answers, "clusters").clusters = Some(strings(&["Storage", "Program", "Other"]));
    let assembled = assemble(&workbook, &answers).unwrap();
    assert_eq!(rules(&assembled.warnings), [Rule::RecordedListIgnored]);
    assert_eq!(
        assembled
            .evaluation
            .clusters
            .iter()
            .map(|c| c.0.as_str())
            .collect::<Vec<_>>(),
        ["Program", "Storage"]
    );
}
