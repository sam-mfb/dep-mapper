use super::*;
use crate::fixture::Scratch;

fn span(file: &str, bytes: std::ops::Range<usize>) -> SourceSpan {
    SourceSpan {
        file: file.into(),
        bytes,
    }
}

#[test]
fn byte_offsets_become_the_lines_they_fall_on() {
    let scratch = Scratch::new();
    std::fs::create_dir_all(scratch.0.join("src")).unwrap();
    // Lines start at bytes 0, 4 and 8.
    std::fs::write(scratch.0.join("src/lib.rs"), "one\ntwo\nthree\n").unwrap();
    let lines = CheckoutLines::new(&scratch.0);
    assert_eq!(lines.lines(&span("src/lib.rs", 0..3)), Some((1, 1)));
    assert_eq!(lines.lines(&span("src/lib.rs", 4..13)), Some((2, 3)));
    assert_eq!(lines.lines(&span("src/lib.rs", 4..8)), Some((2, 2)));
    assert_eq!(lines.lines(&span("src/lib.rs", 9..9)), Some((3, 3)));
}

#[test]
fn missing_files_and_spans_past_the_end_give_no_lines() {
    let scratch = Scratch::new();
    std::fs::create_dir_all(&scratch.0).unwrap();
    std::fs::write(scratch.0.join("a.rs"), "x\n").unwrap();
    let lines = CheckoutLines::new(&scratch.0);
    assert_eq!(lines.lines(&span("missing.rs", 0..1)), None);
    assert_eq!(lines.lines(&span("a.rs", 0..10)), None);
}
