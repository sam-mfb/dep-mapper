use super::*;
use crate::fixture;
use crate::workbook::{ByteOffsets, Presentation, present};

fn workbook() -> Workbook {
    present(
        &fixture::graph(),
        &Presentation {
            page_size: 10,
            source: &ByteOffsets,
            command: "srs eval",
        },
    )
}

fn judge(module: &str, cluster: &str, cite: &[&str]) -> Edit {
    Edit::Judge {
        module: module.into(),
        concern: "Stores records.".into(),
        kind: "effect".into(),
        cluster: cluster.into(),
        reason: "It writes files.".into(),
        cite: cite.iter().map(|c| c.to_string()).collect(),
    }
}

fn relate(piece: &str, cite: &[&str]) -> Edit {
    Edit::Relate {
        piece: PieceId(piece.into()),
        kind: "full-injection".into(),
        reason: "app builds the store and hands it on.".into(),
        cite: cite.iter().map(|c| c.to_string()).collect(),
    }
}

#[test]
fn a_judgment_goes_to_its_module_piece_and_recording_again_replaces_it() {
    let workbook = workbook();
    let first = record(
        &workbook,
        &Answers::new(),
        judge("disk", "Storage", &["item:disk::DiskStore"]),
    )
    .unwrap();
    assert_eq!(first.piece, PieceId::module("disk"));
    assert!(first.warnings.is_empty());
    let second = record(
        &workbook,
        &first.answers,
        judge("module:disk", "Files", &["route:r-impl"]),
    )
    .unwrap();
    let modules = &second.answers[&PieceId::module("disk")].modules;
    assert_eq!(modules.len(), 1);
    assert!(matches!(&modules["disk"], ModuleAnswer::Judged { cluster, .. } if cluster == "Files"));
}

#[test]
fn a_refused_edit_reports_its_problems_and_keeps_nothing() {
    let workbook = workbook();
    let refused = record(
        &workbook,
        &Answers::new(),
        judge("disk", "Storage", &["route:nope", "module:app"]),
    )
    .unwrap_err();
    let rules: Vec<Rule> = refused.problems.iter().map(|p| p.rule).collect();
    assert_eq!(rules, [Rule::UnknownHandle, Rule::ModuleCited]);
    let unknown = record(
        &workbook,
        &Answers::new(),
        judge("ghost", "Storage", &["route:r-impl"]),
    )
    .unwrap_err();
    assert_eq!(unknown.problems[0].rule, Rule::UnknownSubject);
}

#[test]
fn a_judgment_is_kept_with_a_warning_when_its_citations_never_touch_the_module() {
    let recorded = record(
        &workbook(),
        &Answers::new(),
        judge("store", "Storage", &["route:r-write"]),
    )
    .unwrap();
    assert_eq!(recorded.warnings.len(), 1);
    assert_eq!(recorded.warnings[0].rule, Rule::CitationOffSubject);
}

#[test]
fn problems_elsewhere_in_the_answers_do_not_block_an_unrelated_edit() {
    let workbook = workbook();
    let mut answers = Answers::new();
    answers
        .entry(PieceId::module("app"))
        .or_default()
        .modules
        .insert(
            "app".into(),
            ModuleAnswer::Deferred {
                reason: String::new(),
            },
        );
    assert!(
        record(
            &workbook,
            &answers,
            judge("disk", "Storage", &["route:r-impl"])
        )
        .is_ok()
    );
}

#[test]
fn relationships_get_numbered_identifiers_in_their_piece_and_can_be_withdrawn() {
    let workbook = workbook();
    let one = record(
        &workbook,
        &Answers::new(),
        relate("routes/app+disk", &["route:r-wire"]),
    )
    .unwrap();
    assert_eq!(
        one.subject,
        Subject::Relationship("routes/app+disk#1".into())
    );
    let two = record(
        &workbook,
        &one.answers,
        relate("routes/app+disk", &["route:r-wire", "fact:f-construct"]),
    )
    .unwrap();
    assert_eq!(
        two.subject,
        Subject::Relationship("routes/app+disk#2".into())
    );
    let withdrawn = record(
        &workbook,
        &two.answers,
        Edit::Withdraw {
            relationship: "routes/app+disk#1".into(),
        },
    )
    .unwrap();
    let left: Vec<&String> = withdrawn.answers[&PieceId("routes/app+disk".into())]
        .relationships
        .keys()
        .collect();
    assert_eq!(left, ["routes/app+disk#2"]);
    let again = record(
        &workbook,
        &withdrawn.answers,
        relate("routes/app+disk", &["route:r-wire"]),
    )
    .unwrap();
    assert_eq!(
        again.subject,
        Subject::Relationship("routes/app+disk#3".into())
    );
    assert!(
        record(
            &workbook,
            &again.answers,
            Edit::Withdraw {
                relationship: "routes/app+disk#9".into()
            }
        )
        .is_err()
    );
}

#[test]
fn relationships_are_recorded_only_in_routes_pieces() {
    let refused = record(
        &workbook(),
        &Answers::new(),
        relate("module/app", &["route:r-wire"]),
    )
    .unwrap_err();
    assert_eq!(
        refused.problems[0].subject,
        Subject::Piece(PieceId::module("app"))
    );
}

#[test]
fn a_route_set_aside_goes_to_the_piece_that_holds_it() {
    let workbook = workbook();
    let edit = Edit::SetAside {
        route: "route:r-write".into(),
        reason: "Logging only.".into(),
    };
    let recorded = record(&workbook, &Answers::new(), edit).unwrap();
    assert_eq!(recorded.piece, PieceId("routes/app".into()));
    let missing = Edit::SetAside {
        route: "route:nope".into(),
        reason: "x".into(),
    };
    assert!(record(&workbook, &Answers::new(), missing).is_err());
}

#[test]
fn a_cluster_list_must_keep_every_cluster_a_judgment_names_and_a_judgment_must_use_a_listed_one() {
    let workbook = workbook();
    let judged = record(
        &workbook,
        &Answers::new(),
        judge("disk", "Storage", &["route:r-impl"]),
    )
    .unwrap();
    let dropped = record(
        &workbook,
        &judged.answers,
        Edit::Clusters(vec!["Program".into()]),
    )
    .unwrap_err();
    assert_eq!(dropped.problems[0].rule, Rule::UnlistedCluster);
    let listed = record(
        &workbook,
        &judged.answers,
        Edit::Clusters(vec!["Program".into(), "Storage".into()]),
    )
    .unwrap();
    let unlisted = record(
        &workbook,
        &listed.answers,
        judge("app", "Elsewhere", &["route:r-wire"]),
    )
    .unwrap_err();
    assert_eq!(unlisted.problems[0].rule, Rule::UnlistedCluster);
}

#[test]
fn stored_answers_read_back_with_missing_fields_as_empty() {
    let text = r#"{"modules":{"disk":{"decision":"deferred","reason":"later"}}}"#;
    let answers: PieceAnswers = serde_json::from_str(text).unwrap();
    assert!(
        answers.relationships.is_empty()
            && answers.set_aside.is_empty()
            && answers.clusters.is_none()
    );
    assert_eq!(
        answers.modules["disk"],
        ModuleAnswer::Deferred {
            reason: "later".into()
        }
    );
}
