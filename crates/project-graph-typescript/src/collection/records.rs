//! The records the collector prints, one JSON object per line. They mirror
//! `collector/src/records.ts`; the two definitions change together.

use serde::Deserialize;
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(tag = "record", rename_all = "snake_case")]
pub enum Record {
    Project {
        name: String,
        settings: BTreeMap<String, String>,
    },
    Module {
        id: String,
        name: String,
        root: String,
    },
    Item {
        id: String,
        module: String,
        name: String,
        kind: ItemKind,
        /// For a method, the class or interface that declares it.
        parent: Option<String>,
        visibility: Visibility,
        signature: Option<String>,
        declaration: Option<Span>,
    },
    Fact {
        id: String,
        module: String,
        subject: Endpoint,
        fact: FactKind,
        object: Endpoint,
        site: Option<Span>,
        /// Set when the move happens only on one branch of a conditional.
        condition: Option<String>,
    },
    /// A method call whose receiver the collector followed into the
    /// callee's `self` place, with the moves that put it there.
    Receiver { call: String, moves: Vec<String> },
    Gap {
        module: Option<String>,
        site: Option<Span>,
        reason: String,
    },
    /// The last record. Output without it is incomplete.
    End,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct Span {
    pub file: String,
    pub start: usize,
    pub end: usize,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Endpoint {
    Module(String),
    Item(String),
    Place { owner: String, projection: String },
    External(String),
    Unresolved(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ItemKind {
    Type,
    Value,
    Function,
    Method,
    Module,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Visibility {
    Exported,
    Internal,
    Unknown,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum FactKind {
    DeclaredDependency,
    Import {
        local_name: String,
        usage: ImportUse,
    },
    Reexport {
        exported_name: String,
    },
    TypeReference,
    ValueReference,
    Implements,
    Call {
        dispatch: Dispatch,
    },
    Construct,
    Store,
    Read,
    Pass,
    Return,
    Coerce,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImportUse {
    Unused,
    TypeOnly,
    Value,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Dispatch {
    Direct,
    Contract,
    Indirect,
    Foreign,
}

/// Parses one line of collector output.
pub fn parse(line: &str) -> Result<Record, serde_json::Error> {
    serde_json::from_str(line)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fact_line_parses_with_its_kind_and_endpoints() {
        let line = r#"{"record":"fact","id":"pass@a.ts:3","module":"m","subject":{"place":{"owner":"m/a.ts#f","projection":"x"}},"fact":{"kind":"pass"},"object":{"external":"clock#now(x)"},"site":{"file":"a.ts","start":3,"end":4},"condition":null}"#;
        let Record::Fact {
            subject,
            fact,
            object,
            ..
        } = parse(line).unwrap()
        else {
            panic!("expected a fact")
        };
        assert_eq!(
            subject,
            Endpoint::Place {
                owner: "m/a.ts#f".into(),
                projection: "x".into()
            }
        );
        assert_eq!(fact, FactKind::Pass);
        assert_eq!(object, Endpoint::External("clock#now(x)".into()));
    }

    #[test]
    fn an_import_line_keeps_its_local_name_and_usage() {
        let line = r#"{"kind":"import","local_name":"Label","usage":"unused"}"#;
        assert_eq!(
            serde_json::from_str::<FactKind>(line).unwrap(),
            FactKind::Import {
                local_name: "Label".into(),
                usage: ImportUse::Unused
            }
        );
    }

    #[test]
    fn an_unknown_record_is_an_error() {
        assert!(parse(r#"{"record":"mystery"}"#).is_err());
    }
}
