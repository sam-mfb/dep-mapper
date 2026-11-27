//! The answers recorded in a workbook, one set per piece, and the edits that
//! change them. An edit is checked before it is kept: `record` applies it to
//! a copy and refuses it if the checks find a problem with what it changed.
//!
//! Answers name things by handle and kinds by their written names, such as
//! `effect` or `adapted-injection`, so they can be stored as text. A missing
//! field in stored answers means an empty map or no cluster list.

use crate::checking::{self, Finding, Rule, Subject};
use crate::workbook::{PieceId, PieceSubject, Workbook};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Every piece's answers, by piece.
pub type Answers = BTreeMap<PieceId, PieceAnswers>;

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PieceAnswers {
    #[serde(default)]
    pub modules: BTreeMap<String, ModuleAnswer>,
    #[serde(default)]
    pub clusters: Option<Vec<String>>,
    /// By relationship identifier, which the workbook assigns.
    #[serde(default)]
    pub relationships: BTreeMap<String, RelationshipAnswer>,
    /// Routes considered and given no relationship, by route handle, with the reason.
    #[serde(default)]
    pub set_aside: BTreeMap<String, String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "decision", rename_all = "kebab-case")]
pub enum ModuleAnswer {
    Judged {
        concern: String,
        kind: String,
        cluster: String,
        reason: String,
        cite: Vec<String>,
    },
    Deferred {
        reason: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RelationshipAnswer {
    pub kind: String,
    pub reason: String,
    pub cite: Vec<String>,
}

/// One change to the answers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Edit {
    Judge {
        module: String,
        concern: String,
        kind: String,
        cluster: String,
        reason: String,
        cite: Vec<String>,
    },
    Defer {
        module: String,
        reason: String,
    },
    Clusters(Vec<String>),
    Relate {
        piece: PieceId,
        kind: String,
        reason: String,
        cite: Vec<String>,
    },
    Withdraw {
        relationship: String,
    },
    SetAside {
        route: String,
        reason: String,
    },
    /// Sets aside, with one reason, every route of a routes piece that no
    /// relationship cites and that is not already set aside.
    SetAsideRest {
        piece: PieceId,
        reason: String,
    },
}

/// An edit that was kept.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Recorded {
    pub answers: Answers,
    /// The piece whose answers changed.
    pub piece: PieceId,
    pub subject: Subject,
    /// Warnings about the changed subject. The edit was kept.
    pub warnings: Vec<Finding>,
}

/// Why an edit was refused. The answers are unchanged.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Refused {
    pub problems: Vec<Finding>,
}

/// Applies an edit to a copy of the answers and checks it.
pub fn record(workbook: &Workbook, answers: &Answers, edit: Edit) -> Result<Recorded, Refused> {
    let mut answers = answers.clone();
    let (piece, subject) = apply(workbook, &mut answers, edit)?;
    let review = checking::review(workbook, &answers);
    let about = |findings: Vec<Finding>| -> Vec<Finding> {
        findings
            .into_iter()
            .filter(|finding| finding.subject == subject || drops_a_cluster(&subject, finding))
            .collect()
    };
    let problems = about(review.problems);
    if !problems.is_empty() {
        return Err(Refused { problems });
    }
    let mut warnings = about(review.warnings);
    warnings.extend(upward_from_others(workbook, &answers, &subject));
    Ok(Recorded {
        warnings,
        answers,
        piece,
        subject,
    })
}

/// A new cluster list is refused if it drops a cluster a judgment names.
fn drops_a_cluster(subject: &Subject, finding: &Finding) -> bool {
    *subject == Subject::Clusters && finding.rule == Rule::UnlistedCluster
}

/// Warnings about modules that use a higher cluster, whose subject is another
/// module but which the edit bears on: a use of the judged module, or any
/// use once a new cluster list is recorded.
fn upward_from_others(workbook: &Workbook, answers: &Answers, subject: &Subject) -> Vec<Finding> {
    checking::upward_dependencies(workbook, answers)
        .into_iter()
        .filter(|upward| upward.finding.subject != *subject)
        .filter(|upward| match subject {
            Subject::Clusters => true,
            Subject::Module(module) => upward.used == *module,
            _ => false,
        })
        .map(|upward| upward.finding)
        .collect()
}

fn refused(subject: Subject, message: String) -> Refused {
    Refused {
        problems: vec![Finding {
            subject,
            rule: Rule::UnknownSubject,
            message,
        }],
    }
}

fn apply(
    workbook: &Workbook,
    answers: &mut Answers,
    edit: Edit,
) -> Result<(PieceId, Subject), Refused> {
    let module_piece = |module: &str| -> Result<PieceId, Refused> {
        let piece = PieceId::module(module);
        match workbook.piece(&piece) {
            Some(_) => Ok(piece),
            None => Err(refused(
                Subject::Module(module.into()),
                format!("module:{module} is not in this graph"),
            )),
        }
    };
    match edit {
        Edit::Judge {
            module,
            concern,
            kind,
            cluster,
            reason,
            cite,
        } => {
            let module = strip_module(&module);
            let piece = module_piece(&module)?;
            let answer = ModuleAnswer::Judged {
                concern,
                kind,
                cluster,
                reason,
                cite,
            };
            answers
                .entry(piece.clone())
                .or_default()
                .modules
                .insert(module.clone(), answer);
            Ok((piece, Subject::Module(module)))
        }
        Edit::Defer { module, reason } => {
            let module = strip_module(&module);
            let piece = module_piece(&module)?;
            let answer = ModuleAnswer::Deferred { reason };
            answers
                .entry(piece.clone())
                .or_default()
                .modules
                .insert(module.clone(), answer);
            Ok((piece, Subject::Module(module)))
        }
        Edit::Clusters(names) => {
            if let Some(fixed) = &workbook.fixed_clusters {
                return Err(Refused {
                    problems: vec![Finding {
                        subject: Subject::Clusters,
                        rule: Rule::FixedClusterList,
                        message: format!(
                            "the cluster list is fixed for this workbook: {}",
                            fixed.join(", ")
                        ),
                    }],
                });
            }
            let piece = PieceId::clusters();
            answers.entry(piece.clone()).or_default().clusters = Some(names);
            Ok((piece, Subject::Clusters))
        }
        Edit::Relate {
            piece,
            kind,
            reason,
            cite,
        } => {
            match workbook.piece(&piece).map(|p| &p.subject) {
                Some(PieceSubject::Routes { .. }) => {}
                _ => {
                    return Err(refused(
                        Subject::Piece(piece.clone()),
                        format!("{} is not a routes piece", piece.0),
                    ));
                }
            }
            let entry = answers.entry(piece.clone()).or_default();
            let id = next_relationship(&piece, entry);
            entry
                .relationships
                .insert(id.clone(), RelationshipAnswer { kind, reason, cite });
            Ok((piece, Subject::Relationship(id)))
        }
        Edit::Withdraw { relationship } => {
            let holder = answers
                .iter_mut()
                .find(|(_, a)| a.relationships.contains_key(&relationship));
            match holder {
                Some((piece, a)) => {
                    a.relationships.remove(&relationship);
                    Ok((piece.clone(), Subject::Withdrawn(relationship)))
                }
                None => Err(refused(
                    Subject::Relationship(relationship.clone()),
                    format!("there is no relationship {relationship}"),
                )),
            }
        }
        Edit::SetAside { route, reason } => match workbook.piece_of_route(&route) {
            Some(piece) => {
                let id = piece.id.clone();
                answers
                    .entry(id.clone())
                    .or_default()
                    .set_aside
                    .insert(route.clone(), reason);
                Ok((id, Subject::SetAside(route)))
            }
            None => Err(refused(
                Subject::SetAside(route.clone()),
                format!("{route} is not a route in this graph"),
            )),
        },
        Edit::SetAsideRest { piece, reason } => {
            let subject = Subject::Piece(piece.clone());
            let Some(PieceSubject::Routes { routes, .. }) =
                workbook.piece(&piece).map(|p| &p.subject)
            else {
                return Err(refused(
                    subject,
                    format!("{} is not a routes piece", piece.0),
                ));
            };
            if reason.trim().is_empty() {
                return Err(refused(
                    subject,
                    "give a reason for setting the routes aside".into(),
                ));
            }
            let cited: std::collections::BTreeSet<String> = answers
                .values()
                .flat_map(|a| a.relationships.values())
                .flat_map(|r| r.cite.iter().cloned())
                .collect();
            let entry = answers.entry(piece.clone()).or_default();
            for route in routes {
                if !cited.contains(route) && !entry.set_aside.contains_key(route) {
                    entry.set_aside.insert(route.clone(), reason.clone());
                }
            }
            Ok((piece, subject))
        }
    }
}

/// Accepts a module given either as its identifier or as its handle.
fn strip_module(module: &str) -> String {
    module.strip_prefix("module:").unwrap_or(module).to_string()
}

/// The next free identifier in a piece, such as `routes/a+b#3`.
fn next_relationship(piece: &PieceId, answers: &PieceAnswers) -> String {
    let prefix = format!("{}#", piece.0);
    let highest = answers
        .relationships
        .keys()
        .filter_map(|id| id.strip_prefix(&prefix)?.parse::<u32>().ok())
        .max()
        .unwrap_or(0);
    format!("{prefix}{}", highest + 1)
}

#[cfg(test)]
mod tests;
