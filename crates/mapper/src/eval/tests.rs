use super::*;
use std::io::{Error, ErrorKind};

#[test]
fn a_closed_reader_is_not_an_error() {
    assert!(reader_left(&CommandError::Output(Error::from(
        ErrorKind::BrokenPipe
    ))));
}

#[test]
fn other_output_failures_are_errors() {
    assert!(!reader_left(&CommandError::Output(Error::from(
        ErrorKind::PermissionDenied
    ))));
    assert!(!reader_left(&CommandError::NoWorkbook("wb".into())));
}
