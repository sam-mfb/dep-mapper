use super::*;

#[test]
fn a_plain_workbook_path_is_used_as_is() {
    assert_eq!(
        eval_command(Path::new("/tmp/srs-sample/work_book.1")),
        "srs eval --workbook /tmp/srs-sample/work_book.1"
    );
}

#[test]
fn other_paths_are_quoted_for_the_shell() {
    assert_eq!(
        eval_command(Path::new("/tmp/my work")),
        "srs eval --workbook '/tmp/my work'"
    );
    assert_eq!(
        eval_command(Path::new("/tmp/it's $HOME")),
        r"srs eval --workbook '/tmp/it'\''s $HOME'"
    );
}
