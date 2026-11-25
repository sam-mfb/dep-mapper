use super::*;
use crate::answers::ModuleAnswer;
use crate::fixture;
use crate::fixture::Scratch;
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

#[test]
fn an_empty_directory_has_no_workbook_answers_or_submission() {
    let scratch = Scratch::new();
    let store = DirStore::new(&scratch.0);
    assert!(store.load_workbook().unwrap().is_none());
    assert!(store.load_answers().unwrap().is_empty());
    assert!(!store.submitted().unwrap());
}

#[test]
fn a_saved_workbook_reads_back_and_its_pieces_and_instructions_are_readable_files() {
    let scratch = Scratch::new();
    let store = DirStore::new(&scratch.0);
    store.save_workbook(&workbook(), "Read me first.").unwrap();
    assert_eq!(store.load_workbook().unwrap(), Some(workbook()));
    assert_eq!(
        fs::read_to_string(scratch.0.join("INSTRUCTIONS.md")).unwrap(),
        "Read me first."
    );
    let mut pieces: Vec<String> = fs::read_dir(scratch.0.join("pieces"))
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    pieces.sort();
    assert_eq!(pieces[0], "001-module_2fapp.txt");
    assert_eq!(pieces.len(), workbook().pieces.len());
}

#[test]
fn answers_are_kept_per_piece_and_survive_saving_the_workbook_again() {
    let scratch = Scratch::new();
    let store = DirStore::new(&scratch.0);
    store.save_workbook(&workbook(), "x").unwrap();
    let mut disk = PieceAnswers::default();
    disk.modules.insert(
        "disk".into(),
        ModuleAnswer::Deferred {
            reason: "Later.".into(),
        },
    );
    let clusters = PieceAnswers {
        clusters: Some(vec!["Storage".into()]),
        ..PieceAnswers::default()
    };
    store.save_answers(&PieceId::module("disk"), &disk).unwrap();
    store.save_answers(&PieceId::clusters(), &clusters).unwrap();
    store.save_workbook(&workbook(), "x").unwrap();
    let answers = store.load_answers().unwrap();
    assert_eq!(answers.len(), 2);
    assert_eq!(answers[&PieceId::module("disk")], disk);
    assert_eq!(answers[&PieceId::clusters()], clusters);
    assert!(!scratch.0.join("answers").read_dir().unwrap().any(|e| {
        e.unwrap()
            .path()
            .extension()
            .is_some_and(|x| x == "partial")
    }));
}

#[test]
fn submission_can_be_marked_and_cleared() {
    let scratch = Scratch::new();
    let store = DirStore::new(&scratch.0);
    store.set_submitted(true).unwrap();
    store.set_submitted(true).unwrap();
    assert!(store.submitted().unwrap());
    store.set_submitted(false).unwrap();
    assert!(!store.submitted().unwrap());
}

#[test]
fn an_unreadable_answers_file_is_an_error_naming_the_file() {
    let scratch = Scratch::new();
    let store = DirStore::new(&scratch.0);
    fs::create_dir_all(scratch.0.join("answers")).unwrap();
    fs::write(scratch.0.join("answers/broken.json"), "{").unwrap();
    let error = store.load_answers().unwrap_err();
    assert!(error.path.ends_with("broken.json"));
}

#[test]
fn piece_file_names_differ_for_pieces_that_differ_only_in_punctuation() {
    assert_ne!(file_name("routes/a+b"), file_name("routes/a_b"));
    assert_eq!(file_name("routes/a+b"), "routes_2fa_2bb");
}
