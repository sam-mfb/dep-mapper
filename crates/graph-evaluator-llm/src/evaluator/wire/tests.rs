use super::*;

/// Structured output takes an object at the root, and every object must be closed.
#[test]
fn every_schema_is_a_closed_object_at_the_root() {
    fn closed(schema: &Value) -> bool {
        match schema {
            Value::Object(map) => {
                let object_ok = map.get("type") != Some(&json!("object"))
                    || map.get("additionalProperties") == Some(&json!(false));
                object_ok && map.values().all(closed)
            }
            Value::Array(items) => items.iter().all(closed),
            _ => true,
        }
    }
    for schema in [
        module_schema(None),
        module_schema(Some(&["Top".to_string()])),
        clusters_schema(),
        routes_schema(),
    ] {
        assert_eq!(schema["type"], "object", "{schema}");
        for combinator in ["anyOf", "oneOf", "allOf"] {
            assert!(schema.get(combinator).is_none(), "{schema}");
        }
        assert!(closed(&schema), "{schema}");
    }
}

#[test]
fn a_module_reply_reads_either_decision() {
    let judged: ModuleReply = parse(
        &json!({"answer": {"decision": "judge", "concern": "c", "kind": "logic", "cluster": "K", "reason": "r", "evidence": ["item:a"]}}).to_string(),
    )
    .unwrap();
    assert!(matches!(judged.answer, ModuleAnswer::Judge { .. }));
    let deferred: ModuleReply =
        parse(&json!({"answer": {"decision": "defer", "reason": "r"}}).to_string()).unwrap();
    assert!(matches!(deferred.answer, ModuleAnswer::Defer { .. }));
    assert!(
        parse::<ModuleReply>(&json!({"decision": "defer", "reason": "r"}).to_string()).is_err()
    );
}
