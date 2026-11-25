use super::*;

fn def(path: &str) -> Def {
    Def {
        crate_name: "supplier".into(),
        crate_id: "1a2b".into(),
        path: path.into(),
        name: format!("supplier::{path}"),
    }
}

#[test]
fn records_survive_a_write_and_read() {
    let records = vec![
        Record::Unit(UnitRecord {
            package: "supplier".into(),
            crate_name: "supplier".into(),
            crate_id: "1a2b".into(),
            crate_types: vec!["lib".into()],
            test: false,
            features: vec!["default".into()],
            target_triple: "aarch64-unknown-linux-gnu".into(),
            externs: vec![],
        }),
        Record::Call(CallRecord {
            owner: def("consume"),
            callee: None,
            dispatch: Dispatch::Indirect,
            span: Span {
                file: "src/lib.rs".into(),
                lo: 4,
                hi: 9,
            },
        }),
    ];
    let mut bytes = Vec::new();
    for record in &records {
        write_record(&mut bytes, record).unwrap();
    }
    assert_eq!(read_records(bytes.as_slice()).unwrap(), records);
}

#[test]
fn a_line_that_is_not_a_record_names_its_line() {
    let input = "\n{\"record\":\"gap\",\"span\":null,\"reason\":\"x\"}\nnot json\n";
    match read_records(input.as_bytes()) {
        Err(ReadError::Json { line, .. }) => assert_eq!(line, 3),
        other => panic!("expected a JSON error, got {other:?}"),
    }
}
