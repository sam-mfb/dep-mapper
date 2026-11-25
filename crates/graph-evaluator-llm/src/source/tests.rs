use super::*;

const TEXT: &str = "one\ntwo\nthree\nfour\n";

fn range(text: &str) -> std::ops::Range<usize> {
    let start = TEXT.find(text).unwrap();
    start..start + text.len()
}

#[test]
fn a_span_within_one_line() {
    let excerpt = locate(TEXT, &range("hre"), 10).unwrap();
    assert_eq!((excerpt.first_line, excerpt.last_line), (3, 3));
    assert_eq!(excerpt.text, "three");
    assert!(!excerpt.cut);
}

#[test]
fn a_span_across_lines_keeps_whole_lines() {
    let excerpt = locate(TEXT, &range("wo\nth"), 10).unwrap();
    assert_eq!((excerpt.first_line, excerpt.last_line), (2, 3));
    assert_eq!(excerpt.text, "two\nthree");
}

#[test]
fn a_span_ending_with_a_newline_ends_on_that_line() {
    let excerpt = locate(TEXT, &range("two\n"), 10).unwrap();
    assert_eq!((excerpt.first_line, excerpt.last_line), (2, 2));
}

#[test]
fn an_empty_span_names_its_line() {
    let excerpt = locate(TEXT, &(4..4), 10).unwrap();
    assert_eq!((excerpt.first_line, excerpt.last_line), (2, 2));
}

#[test]
fn long_excerpts_are_cut_to_the_limit() {
    let excerpt = locate(TEXT, &(0..TEXT.len()), 2).unwrap();
    assert_eq!((excerpt.first_line, excerpt.last_line), (1, 4));
    assert_eq!(excerpt.text, "one\ntwo");
    assert!(excerpt.cut);
}

#[test]
fn a_range_outside_the_file_is_an_error() {
    assert!(locate(TEXT, &(3..99), 10).is_err());
    assert!(locate("é", &(1..2), 10).is_err());
}

#[test]
fn project_files_reads_relative_to_the_root_and_reports_missing_files() {
    let root = std::env::temp_dir().join(format!("srs-llm-source-{}", std::process::id()));
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(root.join("src/lib.rs"), TEXT).unwrap();
    let files = ProjectFiles::new(root.clone(), 5);
    let span = |file: &str| SourceSpan {
        file: file.into(),
        bytes: range("four"),
    };
    assert_eq!(files.excerpt(&span("src/lib.rs")).unwrap().text, "four");
    assert!(files.excerpt(&span("src/missing.rs")).is_err());
    std::fs::remove_dir_all(root).unwrap();
}
