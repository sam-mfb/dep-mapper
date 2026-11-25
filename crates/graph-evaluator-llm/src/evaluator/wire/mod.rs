//! The JSON the model writes for each sort of piece, and its schema. Each
//! answer becomes edits to the shared answers, which check it.

use graph_evaluator_pieces::answers::Edit;
use graph_evaluator_pieces::workbook::PieceId;
use serde::Deserialize;
use serde_json::{Value, json};

fn citations() -> Value {
    json!({
        "type": "array",
        "items": {"type": "string"},
        "description": "Handles such as route:R1, fact:F2 or item:I3 for the evidence the answer rests on.",
    })
}

/// Structured output needs an object at the root, so the decision sits
/// under `answer`.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModuleReply {
    pub answer: ModuleAnswer,
}

#[derive(Deserialize)]
#[serde(tag = "decision", rename_all = "snake_case", deny_unknown_fields)]
pub enum ModuleAnswer {
    Judge {
        concern: String,
        kind: String,
        cluster: String,
        reason: String,
        evidence: Vec<String>,
    },
    Defer {
        reason: String,
    },
}

pub fn module_schema() -> Value {
    let decision = json!({
        "anyOf": [
            {
                "type": "object",
                "properties": {
                    "decision": {"type": "string", "enum": ["judge"]},
                    "concern": {"type": "string", "description": "The module's responsibility, in one or two sentences."},
                    "kind": {"type": "string", "enum": ["logic", "effect", "composition", "contract"]},
                    "cluster": {"type": "string", "description": "The name of the cluster the module belongs in."},
                    "reason": {"type": "string", "description": "Why, including how each citation bears on the judgment."},
                    "evidence": citations(),
                },
                "required": ["decision", "concern", "kind", "cluster", "reason", "evidence"],
                "additionalProperties": false,
            },
            {
                "type": "object",
                "properties": {
                    "decision": {"type": "string", "enum": ["defer"]},
                    "reason": {"type": "string", "description": "What is missing to decide."},
                },
                "required": ["decision", "reason"],
                "additionalProperties": false,
            },
        ],
    });
    json!({
        "type": "object",
        "properties": {"answer": decision},
        "required": ["answer"],
        "additionalProperties": false,
    })
}

impl ModuleAnswer {
    pub fn into_edit(self, module: &str) -> Edit {
        match self {
            ModuleAnswer::Judge {
                concern,
                kind,
                cluster,
                reason,
                evidence,
            } => Edit::Judge {
                module: module.into(),
                concern,
                kind,
                cluster,
                reason,
                cite: evidence,
            },
            ModuleAnswer::Defer { reason } => Edit::Defer {
                module: module.into(),
                reason,
            },
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClustersAnswer {
    pub clusters: Vec<ClusterAnswer>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClusterAnswer {
    pub name: String,
    pub modules: Vec<String>,
}

pub fn clusters_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "clusters": {
                "type": "array",
                "description": "Clusters in the order a reader should meet them.",
                "items": {
                    "type": "object",
                    "properties": {
                        "name": {"type": "string"},
                        "modules": {"type": "array", "items": {"type": "string"}, "description": "Module handles such as module:M1."},
                    },
                    "required": ["name", "modules"],
                    "additionalProperties": false,
                },
            },
        },
        "required": ["clusters"],
        "additionalProperties": false,
    })
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RoutesAnswer {
    relationships: Vec<RelationshipAnswer>,
    set_aside: Vec<SetAsideAnswer>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RelationshipAnswer {
    kind: String,
    other_kind: String,
    reason: String,
    evidence: Vec<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SetAsideAnswer {
    route: String,
    reason: String,
}

pub fn routes_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "relationships": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "kind": {
                            "type": "string",
                            "enum": ["direct-use", "implements", "wires", "supplies-input", "full-injection", "adapted-injection", "incidental", "other"],
                        },
                        "other_kind": {"type": "string", "description": "A short name for the relationship when kind is other; otherwise empty."},
                        "reason": {"type": "string", "description": "Why the evidence means this, including how each citation bears on it."},
                        "evidence": citations(),
                    },
                    "required": ["kind", "other_kind", "reason", "evidence"],
                    "additionalProperties": false,
                },
            },
            "set_aside": {
                "type": "array",
                "description": "Routes of this page considered and given no relationship.",
                "items": {
                    "type": "object",
                    "properties": {
                        "route": {"type": "string", "description": "The route's handle."},
                        "reason": {"type": "string"},
                    },
                    "required": ["route", "reason"],
                    "additionalProperties": false,
                },
            },
        },
        "required": ["relationships", "set_aside"],
        "additionalProperties": false,
    })
}

impl RoutesAnswer {
    /// The edits, or the problems that keep the answer from becoming edits.
    pub fn into_edits(self, piece: &PieceId) -> Result<Vec<Edit>, Vec<String>> {
        let mut problems = vec![];
        let mut edits = vec![];
        for (index, relationship) in self.relationships.into_iter().enumerate() {
            let other = relationship.other_kind.trim();
            let kind = match (relationship.kind.as_str(), other.is_empty()) {
                ("other", true) => {
                    problems.push(format!(
                        "relationship {}: kind other needs other_kind",
                        index + 1
                    ));
                    continue;
                }
                ("other", false) => format!("other:{other}"),
                (kind, true) => kind.to_string(),
                (_, false) => {
                    problems.push(format!(
                        "relationship {}: other_kind must be empty unless kind is other",
                        index + 1
                    ));
                    continue;
                }
            };
            edits.push(Edit::Relate {
                piece: piece.clone(),
                kind,
                reason: relationship.reason,
                cite: relationship.evidence,
            });
        }
        edits.extend(self.set_aside.into_iter().map(|set_aside| Edit::SetAside {
            route: set_aside.route,
            reason: set_aside.reason,
        }));
        if problems.is_empty() {
            Ok(edits)
        } else {
            Err(problems)
        }
    }
}

#[cfg(test)]
mod tests;

/// Reads the model's text as `T`, or says why it could not.
pub fn parse<T: for<'de> Deserialize<'de>>(text: &str) -> Result<T, Vec<String>> {
    serde_json::from_str(text)
        .map_err(|error| vec![format!("the answer does not follow its schema: {error}")])
}
