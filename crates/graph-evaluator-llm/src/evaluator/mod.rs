//! `LlmEvaluator` implements `GraphEvaluator` by asking a model about one
//! piece of the shared workbook at a time: each module, then the clusters,
//! then each page of routes. Each answer becomes edits to the shared answers,
//! whose checks accept or refuse them. A refused answer goes back to the
//! model with its problems, up to a set number of times; if it is still
//! refused, `evaluate` fails and lists them. Warnings and progress go to an
//! injected `EvaluationLog`.

mod wire;

use crate::context::{Limits, supplement};
use crate::model::{ModelClient, ModelReply, ModelRequest, Turn};
use crate::source::{LinesFrom, SourceText};
use graph_evaluation_assembly::answers::{Answers, Edit, ModuleAnswer, record};
use graph_evaluation_assembly::checking::assemble;
use graph_evaluation_assembly::workbook::{
    Piece, PieceId, PieceSubject, Presentation, Workbook, present,
};
use graph_evaluator::{EvaluationError, GraphEvaluation, GraphEvaluator};
use project_graph::ProjectGraph;
use std::collections::BTreeSet;
use std::fmt::Write;

const INSTRUCTIONS: &str = include_str!("instructions.md");

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EvaluatorSettings {
    /// How many times a refused answer goes back to the model.
    pub max_repairs: u32,
    /// The most routes on one page of a piece.
    pub routes_per_page: usize,
    /// The most source excerpts added to one page.
    pub excerpts_per_page: usize,
}

impl EvaluatorSettings {
    /// Every value is stated here so no default applies silently.
    pub fn recommended() -> Self {
        EvaluatorSettings {
            max_repairs: 2,
            routes_per_page: 40,
            excerpts_per_page: 60,
        }
    }
}

/// What happened during an evaluation, for the person running it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LogEvent {
    Asking {
        piece: String,
    },
    /// The answer was refused and is going back to the model.
    Repairing {
        piece: String,
        problems: Vec<String>,
    },
    /// The answer was kept, but the reader should know this.
    Warning {
        piece: String,
        message: String,
    },
    /// The piece was shown without this source text.
    SourceUnavailable {
        piece: String,
        message: String,
    },
}

pub trait EvaluationLog {
    fn record(&self, event: &LogEvent);
}

/// Writes each event as a line on standard error.
pub struct StderrLog;

impl EvaluationLog for StderrLog {
    fn record(&self, event: &LogEvent) {
        match event {
            LogEvent::Asking { piece } => eprintln!("evaluating {piece}"),
            LogEvent::Repairing { piece, problems } => {
                eprintln!(
                    "{piece}: answer refused, asking again: {}",
                    problems.join("; ")
                )
            }
            LogEvent::Warning { piece, message } => eprintln!("{piece}: warning: {message}"),
            LogEvent::SourceUnavailable { piece, message } => {
                eprintln!("{piece}: source not shown: {message}")
            }
        }
    }
}

pub struct LlmEvaluator<M, S, L> {
    model: M,
    source: S,
    log: L,
    settings: EvaluatorSettings,
}

/// Answers that passed their checks, and the warnings about them.
struct Kept {
    answers: Answers,
    warnings: Vec<String>,
}

/// Applies the edits in order to a copy of the answers. Every refused edit's
/// problems are collected, so the model can fix them all at once.
fn apply(workbook: &Workbook, answers: &Answers, edits: Vec<Edit>) -> Result<Kept, Vec<String>> {
    let mut answers = answers.clone();
    let mut warnings = vec![];
    let mut problems = vec![];
    for edit in edits {
        match record(workbook, &answers, edit) {
            Ok(recorded) => {
                answers = recorded.answers;
                warnings.extend(recorded.warnings.into_iter().map(|w| w.message));
            }
            Err(refused) => problems.extend(refused.problems.into_iter().map(|p| p.message)),
        }
    }
    if problems.is_empty() {
        Ok(Kept { answers, warnings })
    } else {
        Err(problems)
    }
}

/// One module's decision so far. Before the clusters piece, its cluster is
/// only proposed.
fn module_line(answers: &Answers, module: &str) -> String {
    let settled = answers
        .get(&PieceId::clusters())
        .is_some_and(|piece| piece.clusters.is_some());
    let cluster_label = if settled {
        "cluster"
    } else {
        "proposed cluster"
    };
    match answers
        .get(&PieceId::module(module))
        .and_then(|piece| piece.modules.get(module))
    {
        Some(ModuleAnswer::Judged {
            concern,
            kind,
            cluster,
            ..
        }) => format!("- module:{module} ({kind}, {cluster_label} \"{cluster}\"): {concern}"),
        Some(ModuleAnswer::Deferred { reason }) => {
            format!("- module:{module} (undecided): {reason}")
        }
        None => format!("- module:{module} (not yet decided)"),
    }
}

/// The edits that carry out the model's cluster answer: each moved module is
/// judged again with its new cluster, then the list is recorded.
fn cluster_edits(
    answers: &Answers,
    judged: &[String],
    reply: wire::ClustersAnswer,
) -> Result<Vec<Edit>, Vec<String>> {
    let mut problems = vec![];
    let mut placed = BTreeSet::new();
    let mut names = vec![];
    let mut edits = vec![];
    for cluster in reply.clusters {
        for module in &cluster.modules {
            let module = module.trim();
            let module = module.strip_prefix("module:").unwrap_or(module).to_string();
            if !judged.contains(&module) {
                problems.push(format!("module:{module} is not one of the judged modules"));
                continue;
            }
            if !placed.insert(module.clone()) {
                problems.push(format!(
                    "module:{module} is placed in more than one cluster"
                ));
                continue;
            }
            if let Some(ModuleAnswer::Judged {
                concern,
                kind,
                cluster: proposed,
                reason,
                cite,
            }) = answers
                .get(&PieceId::module(&module))
                .and_then(|piece| piece.modules.get(&module))
                && proposed != &cluster.name
            {
                edits.push(Edit::Judge {
                    module: module.clone(),
                    concern: concern.clone(),
                    kind: kind.clone(),
                    cluster: cluster.name.clone(),
                    reason: reason.clone(),
                    cite: cite.clone(),
                });
            }
        }
        names.push(cluster.name);
    }
    for module in judged {
        if !placed.contains(module) {
            problems.push(format!("module:{module} is in no cluster"));
        }
    }
    edits.push(Edit::Clusters(names));
    if problems.is_empty() {
        Ok(edits)
    } else {
        Err(problems)
    }
}

fn overview(graph: &ProjectGraph) -> String {
    let build = &graph.build;
    let mut text = format!(
        "Project {}, source revision {}, target {}, configuration {}.\nThe graph has {} modules, {} items, {} facts and {} routes.\nModules:\n",
        build.project,
        build.source_revision,
        build.target,
        build.configuration,
        graph.modules.len(),
        graph.items.len(),
        graph.facts.len(),
        graph.routes.len()
    );
    for (id, module) in &graph.modules {
        let _ = writeln!(
            text,
            "- module:{} \"{}\" at {}",
            id.0, module.name, module.root
        );
    }
    text
}

impl<M: ModelClient, S: SourceText, L: EvaluationLog> LlmEvaluator<M, S, L> {
    pub fn new(model: M, source: S, log: L, settings: EvaluatorSettings) -> Self {
        LlmEvaluator {
            model,
            source,
            log,
            settings,
        }
    }

    /// The page as the model reads it: the shared text, route details and
    /// source excerpts.
    fn page_text(
        &self,
        graph: &ProjectGraph,
        workbook: &Workbook,
        piece: &Piece,
        page: &str,
        name: &str,
    ) -> String {
        let limits = Limits {
            excerpts_per_page: self.settings.excerpts_per_page,
        };
        let supplemented = supplement(graph, workbook, piece, page, &self.source, &limits);
        for message in supplemented.source_problems {
            self.log.record(&LogEvent::SourceUnavailable {
                piece: name.into(),
                message,
            });
        }
        supplemented.text
    }

    /// Asks about one piece until its edits are kept, and returns the
    /// answers with them.
    fn ask(
        &self,
        name: &str,
        system: &str,
        text: String,
        schema: serde_json::Value,
        check: impl Fn(&str) -> Result<Kept, Vec<String>>,
    ) -> Result<Answers, EvaluationError> {
        self.log.record(&LogEvent::Asking { piece: name.into() });
        let failure = |message: String| EvaluationError {
            message: format!("{name}: {message}"),
        };
        let mut request = ModelRequest {
            system: system.into(),
            turns: vec![Turn::User(text)],
            answer_schema: schema,
        };
        let mut repairs = 0;
        loop {
            let turn = match self.model.reply(&request) {
                Err(error) => return Err(failure(error.to_string())),
                Ok(ModelReply::Refused { explanation }) => {
                    return Err(failure(format!("the model declined: {explanation}")));
                }
                Ok(ModelReply::Truncated) => {
                    return Err(failure("the answer exceeded the output limit".into()));
                }
                Ok(ModelReply::Answer(turn)) => turn,
            };
            match check(&turn.text) {
                Ok(kept) => {
                    for message in kept.warnings {
                        self.log.record(&LogEvent::Warning {
                            piece: name.into(),
                            message,
                        });
                    }
                    return Ok(kept.answers);
                }
                Err(problems) if repairs < self.settings.max_repairs => {
                    repairs += 1;
                    self.log.record(&LogEvent::Repairing {
                        piece: name.into(),
                        problems: problems.clone(),
                    });
                    request.turns.push(Turn::Assistant(turn));
                    request.turns.push(Turn::User(format!(
                        "The answer has these problems:\n- {}\nSend the whole corrected answer.",
                        problems.join("\n- ")
                    )));
                }
                Err(problems) => {
                    return Err(failure(format!(
                        "the answer was still refused after {repairs} repairs: {}",
                        problems.join("; ")
                    )));
                }
            }
        }
    }
}

impl<M: ModelClient, S: SourceText, L: EvaluationLog> GraphEvaluator for LlmEvaluator<M, S, L> {
    fn evaluate(&self, graph: &ProjectGraph) -> Result<GraphEvaluation, EvaluationError> {
        let lines = LinesFrom(&self.source);
        let workbook = present(
            graph,
            &Presentation {
                page_size: self.settings.routes_per_page.max(1),
                source: &lines,
                command: None,
            },
        );
        let system = format!("{INSTRUCTIONS}\n{}", overview(graph));
        let mut answers = Answers::new();

        // Modules first, then clusters, then routes: the cluster piece needs
        // every module decision, and the routes pieces show them as context.
        let mut order: Vec<&Piece> = workbook.pieces.iter().collect();
        order.sort_by_key(|piece| match piece.subject {
            PieceSubject::Module { .. } => 0,
            PieceSubject::Clusters => 1,
            PieceSubject::Routes { .. } => 2,
        });
        for piece in order {
            match &piece.subject {
                PieceSubject::Module { module } => {
                    let name = format!("module:{module}");
                    let page = piece.pages.join("\n");
                    let text = format!(
                        "Module piece. Judge or defer this module.\n\n{}",
                        self.page_text(graph, &workbook, piece, &page, &name)
                    );
                    answers = self.ask(&name, &system, text, wire::module_schema(), |reply| {
                        let reply: wire::ModuleReply = wire::parse(reply)?;
                        apply(&workbook, &answers, vec![reply.answer.into_edit(module)])
                    })?;
                }
                PieceSubject::Clusters => {
                    let judged: Vec<String> = answers
                        .values()
                        .flat_map(|piece| &piece.modules)
                        .filter(|(_, answer)| matches!(answer, ModuleAnswer::Judged { .. }))
                        .map(|(module, _)| module.clone())
                        .collect();
                    if judged.is_empty() {
                        continue;
                    }
                    let listing: Vec<String> =
                        judged.iter().map(|m| module_line(&answers, m)).collect();
                    let text = format!(
                        "Clusters piece. {}\nThe judged modules:\n{}\n",
                        piece.pages.join("\n"),
                        listing.join("\n")
                    );
                    answers = self.ask(
                        "clusters",
                        &system,
                        text,
                        wire::clusters_schema(),
                        |reply| {
                            let reply: wire::ClustersAnswer = wire::parse(reply)?;
                            apply(
                                &workbook,
                                &answers,
                                cluster_edits(&answers, &judged, reply)?,
                            )
                        },
                    )?;
                }
                PieceSubject::Routes { modules, .. } => {
                    let context: Vec<String> =
                        modules.iter().map(|m| module_line(&answers, m)).collect();
                    for (index, page) in piece.pages.iter().enumerate() {
                        let name = if piece.pages.len() == 1 {
                            piece.id.0.clone()
                        } else {
                            format!("{} page {}", piece.id.0, index + 1)
                        };
                        let text = format!(
                            "Routes piece. Give the relationships these routes show.\n\nDecisions about these modules:\n{}\n\n{}",
                            context.join("\n"),
                            self.page_text(graph, &workbook, piece, page, &name)
                        );
                        answers =
                            self.ask(&name, &system, text, wire::routes_schema(), |reply| {
                                let reply: wire::RoutesAnswer = wire::parse(reply)?;
                                apply(&workbook, &answers, reply.into_edits(&piece.id)?)
                            })?;
                    }
                }
            }
        }

        let assembled = assemble(&workbook, &answers).map_err(|problems| EvaluationError {
            message: format!(
                "the answers have problems: {}",
                problems
                    .into_iter()
                    .map(|p| p.message)
                    .collect::<Vec<_>>()
                    .join("; ")
            ),
        })?;
        Ok(assembled.evaluation)
    }
}

#[cfg(test)]
mod tests;
