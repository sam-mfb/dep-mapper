use super::*;
use crate::fixture::{self, Scratch};
use crate::store::DirStore;
use crate::workbook::{ByteOffsets, Presentation, present};

struct Session {
    _scratch: Scratch,
    store: DirStore,
}

impl Session {
    fn new() -> Session {
        let scratch = Scratch::new();
        let store = DirStore::new(&scratch.0);
        let workbook = present(
            &fixture::graph(),
            &Presentation {
                page_size: 1,
                source: &ByteOffsets,
                command: "srs eval",
            },
        );
        store
            .save_workbook(&workbook, &instructions("srs eval", &workbook))
            .unwrap();
        Session {
            _scratch: scratch,
            store,
        }
    }

    fn run(&self, line: &[&str]) -> (Outcome, String) {
        let args: Vec<String> = line.iter().map(|s| s.to_string()).collect();
        let mut out = Vec::new();
        let outcome = run("srs eval", &args, &self.store, &mut out).unwrap();
        (outcome, String::from_utf8(out).unwrap())
    }

    fn fail(&self, line: &[&str]) -> CommandError {
        let args: Vec<String> = line.iter().map(|s| s.to_string()).collect();
        run("srs eval", &args, &self.store, &mut Vec::new()).unwrap_err()
    }
}

const JUDGE_DISK: &[&str] = &[
    "judge",
    "module:disk",
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
    "--cite",
    "fact:f-impl",
];

#[test]
fn a_recorded_judgment_is_stored_and_shown_with_its_piece() {
    let session = Session::new();
    let (outcome, text) = session.run(JUDGE_DISK);
    assert_eq!(outcome, Outcome::Done);
    assert_eq!(text, "Recorded module:disk.\n");
    let (_, shown) = session.run(&["show", "module/disk"]);
    assert!(
        shown.contains("Module disk")
            && shown.contains("effect in Storage: Keeps records in files.")
    );
    let (_, index) = session.run(&["index"]);
    assert!(
        index
            .lines()
            .any(|l| l.contains("done") && l.contains("module/disk"))
    );
}

#[test]
fn a_refused_answer_explains_why_and_stores_nothing() {
    let session = Session::new();
    let (outcome, text) = session.run(&[
        "judge",
        "disk",
        "--concern",
        "x",
        "--kind",
        "storage",
        "--cluster",
        "S",
        "--reason",
        "r",
        "--cite",
        "route:nope",
    ]);
    assert_eq!(outcome, Outcome::Refused);
    assert!(text.starts_with("Not recorded:"));
    assert!(
        text.contains("\"storage\" is not a module kind")
            && text.contains("route:nope is not in this graph")
    );
    assert!(session.store.load_answers().unwrap().is_empty());
}

#[test]
fn relationships_print_their_identifier_and_can_be_withdrawn() {
    let session = Session::new();
    let (_, text) = session.run(&[
        "relate",
        "routes/app+disk",
        "--kind",
        "full-injection",
        "--reason",
        "app builds it.",
        "--cite",
        "route:r-wire",
    ]);
    assert_eq!(text, "Recorded relationship routes/app+disk#1.\n");
    let (_, shown) = session.run(&["show", "routes/app+disk"]);
    assert!(shown.contains("Relationship routes/app+disk#1: full-injection, citing route:r-wire."));
    let (_, withdrawn) = session.run(&["withdraw", "routes/app+disk#1"]);
    assert_eq!(withdrawn, "Recorded the withdrawal of routes/app+disk#1.\n");
    assert!(
        session.store.load_answers().unwrap()[&PieceId("routes/app+disk".into())]
            .relationships
            .is_empty()
    );
}

#[test]
fn show_gives_a_handle_in_full_and_pages_of_long_pieces() {
    let session = Session::new();
    let (_, route) = session.run(&["show", "route:r-wire"]);
    assert!(route.starts_with("route:r-wire") && route.contains("via fact:f-construct"));
    let (_, second) = session.run(&["show", "module/app", "--page", "2"]);
    assert!(second.contains("(page 2 of 2)"));
    assert!(matches!(
        session.fail(&["show", "module/app", "--page", "3"]),
        CommandError::Usage(_)
    ));
    assert!(matches!(
        session.fail(&["show", "nowhere"]),
        CommandError::Usage(_)
    ));
}

#[test]
fn submit_refuses_while_problems_remain_and_any_later_answer_clears_the_submission() {
    let session = Session::new();
    session.run(JUDGE_DISK);
    let (outcome, text) = session.run(&["submit"]);
    assert_eq!(outcome, Outcome::Refused);
    assert!(text.contains("no cluster list is recorded") && text.contains("Not submitted."));
    assert!(!session.store.submitted().unwrap());
    session.run(&["clusters", "Storage"]);
    let (outcome, text) = session.run(&["submit"]);
    assert_eq!(outcome, Outcome::Done, "{text}");
    assert!(session.store.submitted().unwrap());
    session.run(&["defer", "app", "--reason", "Needs a look at main."]);
    assert!(!session.store.submitted().unwrap());
}

#[test]
fn set_aside_routes_count_toward_their_piece() {
    let session = Session::new();
    let (_, text) = session.run(&[
        "skip-route",
        "route:r-write",
        "--reason",
        "A log file write.",
    ]);
    assert_eq!(text, "Recorded route:r-write as set aside.\n");
    let (_, index) = session.run(&["index"]);
    assert!(
        index
            .lines()
            .any(|l| l.contains("done") && l.contains("routes/app "))
    );
}

#[test]
fn malformed_commands_are_usage_errors() {
    let session = Session::new();
    for line in [
        &["dance"][..],
        &["judge", "disk", "--kind", "effect"],
        &["defer", "disk", "--reason"],
        &["defer", "disk", "--reason", "r", "--colour", "red"],
        &["index", "extra"],
        &["defer", "disk", "--reason", "a", "--reason", "b"],
    ] {
        assert!(
            matches!(session.fail(line), CommandError::Usage(_)),
            "{line:?}"
        );
    }
}

#[test]
fn commands_need_a_workbook_except_help() {
    let scratch = Scratch::new();
    let store = DirStore::new(&scratch.0);
    let mut out = Vec::new();
    assert_eq!(
        run("srs eval", &["help".to_string()], &store, &mut out).unwrap(),
        Outcome::Done
    );
    assert!(String::from_utf8(out).unwrap().contains("skip-route"));
    let error = run("srs eval", &["index".to_string()], &store, &mut Vec::new()).unwrap_err();
    assert!(matches!(error, CommandError::NoWorkbook(_)));
}

#[test]
fn the_instructions_name_the_project_and_the_command() {
    let workbook = present(
        &fixture::graph(),
        &Presentation {
            page_size: 10,
            source: &ByteOffsets,
            command: "srs eval",
        },
    );
    let text = instructions("srs eval", &workbook);
    assert!(text.starts_with("# Evaluating fixture"));
    assert!(text.contains("`srs eval submit`"));
}
