use super::*;

#[test]
fn a_handle_reads_back_as_written_and_keeps_colons_in_its_identifier() {
    for text in ["module:core", "route:R1", "fact:a:b", "item:crate::Type"] {
        assert_eq!(Handle::parse(text).unwrap().to_string(), text);
    }
}

#[test]
fn unknown_kinds_and_empty_identifiers_are_rejected() {
    for text in ["core", "crate:x", "route:", ""] {
        assert_eq!(
            Handle::parse(text),
            Err(MalformedHandle { text: text.into() })
        );
    }
}

#[test]
fn modules_are_not_evidence_and_other_handles_cite_the_same_graph_reference() {
    assert_eq!(Handle::parse("module:core").unwrap().graph_ref(), None);
    assert_eq!(
        Handle::parse("route:R1").unwrap().graph_ref(),
        Some(GraphRef::from(RouteId("R1".into())))
    );
    assert_eq!(
        Handle::parse("fact:F").unwrap().graph_ref(),
        Some(GraphRef::from(FactId("F".into())))
    );
}
