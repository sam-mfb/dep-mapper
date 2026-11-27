//! Checks answers against a workbook and assembles the evaluation.
//!
//! A problem stops the evaluation: a handle that does not resolve, a module
//! that is not in the graph, an unknown kind, an empty reason or concern, a
//! judgment or relationship with no citations, or a cluster that is not
//! listed. A warning is kept with the answers for a reviewer: a module
//! judgment whose citations never touch the module, a relationship that
//! cites no route, a kind its evidence does not show, a module that uses a
//! module in a cluster listed above its own, or a recorded cluster list that
//! a fixed list replaces.
//!
//! The cluster list is the workbook's fixed list when it has one, and
//! otherwise the list recorded in the clusters piece. Clusters run from top
//! to bottom, so a module's cluster should sit at or above every cluster it
//! uses.

use crate::answers::{Answers, ModuleAnswer, RelationshipAnswer};
use crate::handles::Handle;
use crate::workbook::{PieceId, PieceSubject, Workbook};
use graph_evaluator::{
    ClusterName, Clusters, Decision, GraphEvaluation, ModuleJudgment, ModuleKind, Relationship,
    RelationshipKind,
};
use project_graph::{GraphId, GraphRef, ModuleId};
use std::collections::{BTreeMap, BTreeSet};

/// What a finding is about.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Subject {
    /// A module's decision, by module identifier.
    Module(String),
    Clusters,
    /// A relationship, by its identifier.
    Relationship(String),
    /// A withdrawn relationship, by its identifier.
    Withdrawn(String),
    /// A route set aside, by its handle.
    SetAside(String),
    /// A piece's answers as a whole.
    Piece(PieceId),
}

/// The check that produced a finding.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Rule {
    // Problems.
    UnknownSubject,
    MalformedHandle,
    UnknownHandle,
    ModuleCited,
    WrongPiece,
    UnknownModuleKind,
    UnknownRelationshipKind,
    EmptyText,
    NoCitations,
    UnlistedCluster,
    DuplicateCluster,
    MissingClusterList,
    /// A cluster list was recorded for a workbook whose list is fixed.
    FixedClusterList,
    // Warnings.
    CitationOffSubject,
    NoRouteCited,
    KindNotShown,
    /// A module uses a module whose cluster is listed above its own.
    UpwardDependency,
    /// A recorded cluster list is ignored because the workbook fixes the list.
    RecordedListIgnored,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Finding {
    pub subject: Subject,
    pub rule: Rule,
    pub message: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Review {
    pub problems: Vec<Finding>,
    pub warnings: Vec<Finding>,
}

/// An evaluation assembled from answers with no problems.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Assembled {
    pub evaluation: GraphEvaluation,
    pub warnings: Vec<Finding>,
}

/// Every problem and warning in the answers.
pub fn review(workbook: &Workbook, answers: &Answers) -> Review {
    let mut review = Review::default();
    let clusters = cluster_list(workbook, answers);
    for (piece_id, piece) in answers {
        let Some(found) = workbook.piece(piece_id) else {
            review.problem(
                Subject::Piece(piece_id.clone()),
                Rule::UnknownSubject,
                format!("{} is not a piece of this workbook", piece_id.0),
            );
            continue;
        };
        for (module, answer) in &piece.modules {
            check_module(
                workbook,
                &found.subject,
                module,
                answer,
                clusters,
                &mut review,
            );
        }
        if piece.clusters.is_some() && found.subject != PieceSubject::Clusters {
            review.problem(
                Subject::Piece(piece_id.clone()),
                Rule::WrongPiece,
                "the cluster list belongs in the clusters piece".into(),
            );
        }
        for (id, relationship) in &piece.relationships {
            check_relationship(workbook, id, relationship, &mut review);
        }
        for (route, reason) in &piece.set_aside {
            let subject = Subject::SetAside(route.clone());
            let holds = matches!(&found.subject, PieceSubject::Routes { routes, .. } if routes.contains(route));
            if !holds {
                review.problem(
                    subject.clone(),
                    Rule::WrongPiece,
                    format!("{route} is not a route of {}", piece_id.0),
                );
            }
            if reason.trim().is_empty() {
                review.problem(
                    subject,
                    Rule::EmptyText,
                    "give a reason for setting the route aside".into(),
                );
            }
        }
    }
    if workbook.fixed_clusters.is_some() && recorded_list(answers).is_some() {
        review.warning(
            Subject::Clusters,
            Rule::RecordedListIgnored,
            "the recorded cluster list is ignored: this workbook's list is fixed".into(),
        );
    }
    for upward in upward_dependencies(workbook, answers) {
        review.warnings.push(upward.finding);
    }
    if let Some(list) = clusters {
        let mut seen = BTreeSet::new();
        for name in list {
            if !seen.insert(name) {
                review.problem(
                    Subject::Clusters,
                    Rule::DuplicateCluster,
                    format!("cluster {name:?} is listed more than once"),
                );
            }
            if name.trim().is_empty() {
                review.problem(
                    Subject::Clusters,
                    Rule::EmptyText,
                    "a cluster name is empty".into(),
                );
            }
        }
    } else if judged_modules(answers).next().is_some() {
        review.problem(
            Subject::Clusters,
            Rule::MissingClusterList,
            "modules are judged but no cluster list is recorded".into(),
        );
    }
    review
}

/// The evaluation, if the answers have no problems; otherwise the problems.
pub fn assemble(workbook: &Workbook, answers: &Answers) -> Result<Assembled, Vec<Finding>> {
    let review = review(workbook, answers);
    if !review.problems.is_empty() {
        return Err(review.problems);
    }
    let refs = |cite: &[String]| -> Vec<GraphRef> {
        cite.iter()
            .filter_map(|text| Handle::parse(text).ok()?.graph_ref())
            .collect()
    };
    let list = cluster_list(workbook, answers).cloned().unwrap_or_default();
    let clusters = Clusters::new(list.into_iter().map(ClusterName).collect())
        .expect("the review rejects duplicate clusters");
    let mut modules = BTreeMap::new();
    let mut relationships = Vec::new();
    for piece in answers.values() {
        for (module, answer) in &piece.modules {
            let decision = match answer {
                ModuleAnswer::Judged {
                    concern,
                    kind,
                    cluster,
                    reason,
                    cite,
                } => Decision::Judged {
                    judgment: ModuleJudgment {
                        concern: concern.clone(),
                        kind: module_kind(kind).expect("the review rejects unknown kinds"),
                        cluster: ClusterName(cluster.clone()),
                    },
                    reason: reason.clone(),
                    evidence: refs(cite),
                },
                ModuleAnswer::Deferred { reason } => Decision::Deferred {
                    reason: reason.clone(),
                },
            };
            modules.insert(ModuleId(module.clone()), decision);
        }
        for relationship in piece.relationships.values() {
            relationships.push(Relationship {
                kind: relationship_kind(&relationship.kind)
                    .expect("the review rejects unknown kinds"),
                reason: relationship.reason.clone(),
                evidence: refs(&relationship.cite),
            });
        }
    }
    Ok(Assembled {
        evaluation: GraphEvaluation {
            graph: GraphId(workbook.graph.clone()),
            clusters,
            modules,
            relationships,
        },
        warnings: review.warnings,
    })
}

/// How far each piece has got.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Progress {
    NotStarted,
    /// Some of the piece's routes are covered.
    Partly {
        done: usize,
        of: usize,
    },
    Done,
}

/// Each piece's progress, in workbook order. A routes piece is done when
/// every route in it is cited by a relationship, in any piece, or set aside.
pub fn progress(workbook: &Workbook, answers: &Answers) -> Vec<(PieceId, Progress)> {
    let cited: BTreeSet<&str> = answers
        .values()
        .flat_map(|a| a.relationships.values())
        .flat_map(|r| r.cite.iter().map(String::as_str))
        .collect();
    let set_aside: BTreeSet<&str> = answers
        .values()
        .flat_map(|a| a.set_aside.keys().map(String::as_str))
        .collect();
    workbook
        .pieces
        .iter()
        .map(|piece| {
            let progress = match &piece.subject {
                PieceSubject::Module { module } => {
                    let decided = answers
                        .get(&piece.id)
                        .is_some_and(|a| a.modules.contains_key(module));
                    if decided {
                        Progress::Done
                    } else {
                        Progress::NotStarted
                    }
                }
                PieceSubject::Clusters => {
                    if cluster_list(workbook, answers).is_some() {
                        Progress::Done
                    } else {
                        Progress::NotStarted
                    }
                }
                PieceSubject::Routes { routes, .. } => {
                    let done = routes
                        .iter()
                        .filter(|r| cited.contains(r.as_str()) || set_aside.contains(r.as_str()))
                        .count();
                    match done {
                        0 => Progress::NotStarted,
                        d if d == routes.len() => Progress::Done,
                        d => Progress::Partly {
                            done: d,
                            of: routes.len(),
                        },
                    }
                }
            };
            (piece.id.clone(), progress)
        })
        .collect()
}

pub fn module_kind(text: &str) -> Option<ModuleKind> {
    match text {
        "logic" => Some(ModuleKind::Logic),
        "effect" => Some(ModuleKind::Effect),
        "composition" => Some(ModuleKind::Composition),
        "contract" => Some(ModuleKind::Contract),
        _ => None,
    }
}

pub fn relationship_kind(text: &str) -> Option<RelationshipKind> {
    match text {
        "direct-use" => Some(RelationshipKind::DirectUse),
        "implements" => Some(RelationshipKind::Implements),
        "wires" => Some(RelationshipKind::Wires),
        "supplies-input" => Some(RelationshipKind::SuppliesInput),
        "full-injection" => Some(RelationshipKind::FullInjection),
        "adapted-injection" => Some(RelationshipKind::AdaptedInjection),
        "incidental" => Some(RelationshipKind::Incidental),
        _ => match text.strip_prefix("other:") {
            Some(name) if !name.trim().is_empty() => Some(RelationshipKind::Other(name.into())),
            _ => None,
        },
    }
}

impl Review {
    fn problem(&mut self, subject: Subject, rule: Rule, message: String) {
        self.problems.push(Finding {
            subject,
            rule,
            message,
        });
    }

    fn warning(&mut self, subject: Subject, rule: Rule, message: String) {
        self.warnings.push(Finding {
            subject,
            rule,
            message,
        });
    }
}

/// The cluster list in force: the workbook's fixed list, or else the
/// recorded one.
fn cluster_list<'a>(workbook: &'a Workbook, answers: &'a Answers) -> Option<&'a Vec<String>> {
    workbook
        .fixed_clusters
        .as_ref()
        .or_else(|| recorded_list(answers))
}

fn recorded_list(answers: &Answers) -> Option<&Vec<String>> {
    answers
        .get(&PieceId::clusters())
        .and_then(|a| a.clusters.as_ref())
}

/// A warning that a module uses a module in a higher cluster.
pub(crate) struct Upward {
    /// The used module. The finding's subject is the user.
    pub(crate) used: String,
    pub(crate) finding: Finding,
}

/// Each use of a module whose cluster is listed above the user's own, among
/// judged modules whose clusters are listed.
pub(crate) fn upward_dependencies(workbook: &Workbook, answers: &Answers) -> Vec<Upward> {
    let Some(list) = cluster_list(workbook, answers) else {
        return vec![];
    };
    let position = |cluster: &String| list.iter().position(|listed| listed == cluster);
    let placed: BTreeMap<&String, (&String, usize)> = judged_modules(answers)
        .filter_map(|(module, cluster)| Some((module, (cluster, position(cluster)?))))
        .collect();
    let mut upward = vec![];
    for (user, used_modules) in &workbook.uses {
        let Some((user_cluster, user_at)) = placed.get(user) else {
            continue;
        };
        for (used, fact) in used_modules {
            let Some((used_cluster, used_at)) = placed.get(used) else {
                continue;
            };
            if used_at < user_at {
                upward.push(Upward {
                    used: used.clone(),
                    finding: Finding {
                        subject: Subject::Module(user.clone()),
                        rule: Rule::UpwardDependency,
                        message: format!(
                            "module:{user} in cluster {user_cluster:?} uses module:{used} in cluster {used_cluster:?}, which is listed above it ({fact}); clusters run from top to bottom, so a module's cluster should sit at or above every cluster it uses"
                        ),
                    },
                });
            }
        }
    }
    upward
}

fn judged_modules(answers: &Answers) -> impl Iterator<Item = (&String, &String)> {
    answers
        .values()
        .flat_map(|a| &a.modules)
        .filter_map(|(module, answer)| match answer {
            ModuleAnswer::Judged { cluster, .. } => Some((module, cluster)),
            ModuleAnswer::Deferred { .. } => None,
        })
}

fn check_module(
    workbook: &Workbook,
    piece: &PieceSubject,
    module: &str,
    answer: &ModuleAnswer,
    clusters: Option<&Vec<String>>,
    review: &mut Review,
) {
    let subject = Subject::Module(module.into());
    if !workbook.modules.contains_key(module) {
        review.problem(
            subject,
            Rule::UnknownSubject,
            format!("module:{module} is not in this graph"),
        );
        return;
    }
    if !matches!(piece, PieceSubject::Module { module: own } if own == module) {
        review.problem(
            subject.clone(),
            Rule::WrongPiece,
            format!("module:{module}'s decision belongs in module/{module}"),
        );
    }
    match answer {
        ModuleAnswer::Deferred { reason } => {
            if reason.trim().is_empty() {
                review.problem(
                    subject,
                    Rule::EmptyText,
                    "give a reason for deferring".into(),
                );
            }
        }
        ModuleAnswer::Judged {
            concern,
            kind,
            cluster,
            reason,
            cite,
        } => {
            if concern.trim().is_empty() {
                review.problem(
                    subject.clone(),
                    Rule::EmptyText,
                    "the concern is empty".into(),
                );
            }
            if reason.trim().is_empty() {
                review.problem(
                    subject.clone(),
                    Rule::EmptyText,
                    "the reason is empty".into(),
                );
            }
            if module_kind(kind).is_none() {
                review.problem(
                    subject.clone(),
                    Rule::UnknownModuleKind,
                    format!(
                        "{kind:?} is not a module kind: use logic, effect, composition or contract"
                    ),
                );
            }
            match clusters {
                Some(list) if !list.contains(cluster) => review.problem(
                    subject.clone(),
                    Rule::UnlistedCluster,
                    format!("cluster {cluster:?} is not in the cluster list"),
                ),
                _ if cluster.trim().is_empty() => review.problem(
                    subject.clone(),
                    Rule::EmptyText,
                    "the cluster is empty".into(),
                ),
                _ => {}
            }
            let modules = check_citations(workbook, &subject, cite, review);
            if !modules.is_empty() && !modules.contains(module) {
                review.warning(
                    subject,
                    Rule::CitationOffSubject,
                    format!("none of the citations touches module:{module}"),
                );
            }
        }
    }
}

fn check_relationship(
    workbook: &Workbook,
    id: &str,
    relationship: &RelationshipAnswer,
    review: &mut Review,
) {
    let subject = Subject::Relationship(id.into());
    let kind = relationship_kind(&relationship.kind);
    if kind.is_none() {
        review.problem(
            subject.clone(),
            Rule::UnknownRelationshipKind,
            format!(
                "{:?} is not a relationship kind: use direct-use, implements, wires, supplies-input, full-injection, adapted-injection, incidental or other:<name>",
                relationship.kind
            ),
        );
    }
    if relationship.reason.trim().is_empty() {
        review.problem(
            subject.clone(),
            Rule::EmptyText,
            "the reason is empty".into(),
        );
    }
    check_citations(workbook, &subject, &relationship.cite, review);
    let resolved: Vec<_> = relationship
        .cite
        .iter()
        .filter_map(|c| workbook.refs.get(c).map(|info| (c, info)))
        .collect();
    if !resolved.is_empty() && !resolved.iter().any(|(c, _)| c.starts_with("route:")) {
        review.warning(
            subject.clone(),
            Rule::NoRouteCited,
            "the relationship cites no route".into(),
        );
    }
    let implements = kind == Some(RelationshipKind::Implements);
    let shows_implements = resolved
        .iter()
        .any(|(_, info)| info.fact_kinds.contains("implements"));
    if implements && !resolved.is_empty() && !shows_implements {
        review.warning(
            subject,
            Rule::KindNotShown,
            "implements is claimed but no cited evidence holds an implements fact".into(),
        );
    }
}

/// Checks each citation and returns the modules the resolved ones touch.
fn check_citations(
    workbook: &Workbook,
    subject: &Subject,
    cite: &[String],
    review: &mut Review,
) -> BTreeSet<String> {
    if cite.is_empty() {
        review.problem(
            subject.clone(),
            Rule::NoCitations,
            "cite at least one route, fact or item".into(),
        );
    }
    let mut modules = BTreeSet::new();
    for text in cite {
        match Handle::parse(text) {
            Err(error) => review.problem(subject.clone(), Rule::MalformedHandle, error.to_string()),
            Ok(Handle::Module(_)) => review.problem(
                subject.clone(),
                Rule::ModuleCited,
                format!("{text} is a module; cite a route, fact or item as evidence"),
            ),
            Ok(_) => match workbook.refs.get(text) {
                Some(info) => modules.extend(info.modules.iter().cloned()),
                None => review.problem(
                    subject.clone(),
                    Rule::UnknownHandle,
                    format!("{text} is not in this graph"),
                ),
            },
        }
    }
    modules
}

#[cfg(test)]
mod tests;
