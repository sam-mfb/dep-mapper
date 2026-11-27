//! The commands an agent or a person uses to read a workbook and record
//! answers. A program mounts them under a name of its own, such as
//! `srs eval`, and passes the remaining arguments to [`run`].

mod args;

use crate::answers::{self, Edit, ModuleAnswer};
use crate::checking::{self, Finding, Progress};
use crate::store::{StoreError, WorkbookStore};
use crate::workbook::{PieceId, PieceSubject, Workbook};
use args::Args;
use std::io::Write;

/// How a command ended, other than by failing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    Done,
    /// An answer was refused, or a check found problems. The output says why.
    Refused,
}

#[derive(Debug, thiserror::Error)]
pub enum CommandError {
    #[error("{0}\nRun `help` for the list of commands.")]
    Usage(String),
    #[error("there is no workbook at {0}")]
    NoWorkbook(String),
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error("could not write the output: {0}")]
    Output(#[from] std::io::Error),
}

/// Runs one command. `command` is how the reader invokes these commands,
/// for help text, such as `srs eval`.
pub fn run(
    command: &str,
    arguments: &[String],
    store: &dyn WorkbookStore,
    out: &mut dyn Write,
) -> Result<Outcome, CommandError> {
    let mut args = Args::new(arguments);
    let Some(name) = args.next_positional() else {
        writeln!(out, "{}", help(command))?;
        return Ok(Outcome::Done);
    };
    if name == "help" {
        writeln!(out, "{}", help(command))?;
        return Ok(Outcome::Done);
    }
    let workbook = store
        .load_workbook()?
        .ok_or_else(|| CommandError::NoWorkbook(store.location()))?;
    let answers = store.load_answers()?;
    let edit = match name.as_str() {
        "index" => return index(&workbook, &answers, &mut args, out),
        "show" => return show(&workbook, &answers, &mut args, out),
        "check" => return check(&workbook, &answers, &mut args, out),
        "submit" => return submit(&workbook, &answers, &mut args, store, out),
        "judge" => Edit::Judge {
            module: args.required_positional("a module")?,
            concern: args.required("--concern")?,
            kind: args.required("--kind")?,
            cluster: args.required("--cluster")?,
            reason: args.required("--reason")?,
            cite: args.all("--cite"),
        },
        "defer" => Edit::Defer {
            module: args.required_positional("a module")?,
            reason: args.required("--reason")?,
        },
        "clusters" => Edit::Clusters(args.rest_positional()),
        "relate" => Edit::Relate {
            piece: PieceId(args.required_positional("a routes piece")?),
            kind: args.required("--kind")?,
            reason: args.required("--reason")?,
            cite: args.all("--cite"),
        },
        "withdraw" => Edit::Withdraw {
            relationship: args.required_positional("a relationship identifier")?,
        },
        "skip-route" => Edit::SetAside {
            route: args.required_positional("a route handle")?,
            reason: args.required("--reason")?,
        },
        "skip-rest" => Edit::SetAsideRest {
            piece: PieceId(args.required_positional("a routes piece")?),
            reason: args.required("--reason")?,
        },
        other => return Err(CommandError::Usage(format!("unknown command {other:?}"))),
    };
    args.finish()?;
    match answers::record(&workbook, &answers, edit) {
        Ok(recorded) => {
            store.save_answers(&recorded.piece, &recorded.answers[&recorded.piece])?;
            store.set_submitted(false)?;
            writeln!(out, "Recorded {}.", subject_text(&recorded.subject))?;
            if let checking::Subject::Piece(piece) = &recorded.subject {
                let before = answers.get(piece).map_or(0, |a| a.set_aside.len());
                let after = recorded.answers[piece].set_aside.len();
                writeln!(out, "{} routes set aside.", after - before)?;
            }
            print_findings(out, "Warning", &recorded.warnings)?;
            Ok(Outcome::Done)
        }
        Err(refused) => {
            writeln!(out, "Not recorded:")?;
            print_findings(out, "Problem", &refused.problems)?;
            Ok(Outcome::Refused)
        }
    }
}

/// The most lines of a handle's detail `show` prints at once. A route used
/// at many sites names a fact on each line.
const HANDLE_PAGE_LINES: usize = 80;

/// What the commands are and how to use them.
pub fn help(command: &str) -> String {
    format!(
        "Commands, run as `{c} <command>`:
  index                      list the pieces and how far each has got
  show <piece> [--page N]    show a piece, such as module/core, clusters or routes/a+b
  show <handle> [--page N]   show a route, fact or item in full, such as route:R12
  judge <module> --concern <text> --kind logic|effect|composition|contract
        --cluster <name> --reason <text> --cite <handle> [--cite <handle>...]
                             record a module's decision; recording again replaces it
  defer <module> --reason <text>
                             record that a module was considered and left undecided
  clusters <name>...         record the cluster list, top to bottom; replaces it
  relate <routes piece> --kind <kind> --reason <text> --cite <handle>...
                             record a relationship; prints its identifier
        kinds: direct-use, implements, wires, supplies-input, full-injection,
               adapted-injection, incidental, other:<name>
  withdraw <relationship>    remove a relationship by its identifier
  skip-route <route> --reason <text>
                             record that a route was considered and carries no relationship
  skip-rest <routes piece> --reason <text>
                             set aside every route of the piece that no relationship
                             cites and that is not set aside already
  check                      list every problem and warning
  submit                     check, then mark the answers as finished
Each answer is checked when it is recorded. A problem refuses it; a warning keeps it.",
        c = command
    )
}

/// The instructions' step for the cluster list.
fn clusters_step(workbook: &Workbook) -> String {
    match &workbook.fixed_clusters {
        Some(fixed) => format!(
            "2. The clusters are fixed, from top to bottom: {}. Every module
   judgment names one of them. Defer a module that fits none, and say why.
   The `clusters` piece only states the list; there is nothing to record.",
            fixed.join(", ")
        ),
        None => "2. The `clusters` piece. List every cluster name you used, from top to
   bottom: first the program's cluster, which uses the others, and last the
   foundations. A module's cluster should sit at or above every cluster it
   uses; `check` warns when a module uses one listed above its own."
            .to_string(),
    }
}

/// How to work through a workbook, written beside it.
pub fn instructions(command: &str, workbook: &Workbook) -> String {
    format!(
        "# Evaluating {project}

This workbook presents the source evidence for {project} (graph `{graph}`).
Your job is to judge what each module is for and what its connections mean,
citing the evidence for each answer.

Work in this order:

1. Module pieces (`module/...`). For each module, record its concern (what it
   is responsible for, without naming who uses it), its kind, and the cluster
   it belongs to. Defer it, with a reason, if the evidence does not settle it.
{clusters}
3. Routes pieces (`routes/...`). For the routes in each piece, record the
   relationships they show, or set a route aside with a reason. One
   relationship can cite routes from several pieces. A relationship needs
   only the routes that show it, not every route of the same kind.

A large project has far more routes than anyone can judge one by one. Every
module decision and the cluster list matter most. Then judge the pieces
between two modules, since they show how the architecture is joined. A route
you leave alone stays on the map as unreviewed, which is allowed. When the
rest of a piece is routine, such as formatting calls or reads of a
dependency's helpers, `skip-rest` sets it aside in one step with one reason.
Each module piece counts its routes by where they lead and names the piece
that lists them.

Cite evidence by handle: `route:<id>`, `fact:<id>` or `item:<id>`. Each piece
lists the handles it covers; `{c} show <handle>` shows one in full. Read the
source in the project checkout when the evidence needs it.

Pieces are independent once the module pieces are done, so several workers
can take different pieces at the same time.

When `{c} index` shows every piece done and `{c} check` finds no problems,
run `{c} submit`.

{help}
",
        project = workbook.project,
        graph = workbook.graph,
        c = command,
        help = help(command),
        clusters = clusters_step(workbook)
    )
}

fn index(
    workbook: &Workbook,
    answers: &answers::Answers,
    args: &mut Args,
    out: &mut dyn Write,
) -> Result<Outcome, CommandError> {
    args.finish()?;
    writeln!(
        out,
        "Workbook for {} (graph {})",
        workbook.project, workbook.graph
    )?;
    for ((id, progress), piece) in checking::progress(workbook, answers)
        .iter()
        .zip(&workbook.pieces)
    {
        let state = match progress {
            Progress::NotStarted => "not started".to_string(),
            Progress::Partly { done, of } => format!("{done} of {of} routes"),
            Progress::Done => "done".to_string(),
        };
        let pages = match piece.pages.len() {
            1 => String::new(),
            n => format!(", {n} pages"),
        };
        writeln!(out, "  {:<12} {}  {}{pages}", state, id.0, piece.title)?;
    }
    Ok(Outcome::Done)
}

fn show(
    workbook: &Workbook,
    answers: &answers::Answers,
    args: &mut Args,
    out: &mut dyn Write,
) -> Result<Outcome, CommandError> {
    let target = args.required_positional("a piece or handle")?;
    let page = match args.optional("--page")? {
        Some(text) => text
            .parse::<usize>()
            .ok()
            .filter(|n| *n >= 1)
            .ok_or_else(|| {
                CommandError::Usage(format!("--page takes a page number, not {text:?}"))
            })?,
        None => 1,
    };
    args.finish()?;
    if let Some(info) = workbook.refs.get(&target) {
        let lines: Vec<&str> = info.detail.lines().collect();
        let pages: Vec<&[&str]> = lines.chunks(HANDLE_PAGE_LINES).collect();
        let Some(shown) = pages.get(page - 1) else {
            return Err(CommandError::Usage(format!(
                "{target} has {} pages",
                pages.len()
            )));
        };
        writeln!(out, "{}", shown.join("\n"))?;
        if pages.len() > 1 {
            writeln!(
                out,
                "(page {page} of {}; `show {target} --page N` shows another)",
                pages.len()
            )?;
        }
        return Ok(Outcome::Done);
    }
    let Some(piece) = workbook.piece(&PieceId(target.clone())) else {
        return Err(CommandError::Usage(format!(
            "{target:?} is neither a piece nor a route, fact or item handle; `index` lists the pieces"
        )));
    };
    let Some(text) = piece.pages.get(page - 1) else {
        return Err(CommandError::Usage(format!(
            "{} has {} pages",
            piece.id.0,
            piece.pages.len()
        )));
    };
    writeln!(out, "{text}")?;
    let recorded = answers.get(&piece.id);
    match &piece.subject {
        PieceSubject::Module { module } => {
            writeln!(out, "Recorded decision:")?;
            match recorded.and_then(|a| a.modules.get(module)) {
                Some(answer) => writeln!(out, "  {}", decision_text(answer))?,
                None => writeln!(out, "  none yet")?,
            }
        }
        PieceSubject::Clusters => {
            writeln!(out, "Module decisions so far:")?;
            for (module, answer) in answers.values().flat_map(|a| &a.modules) {
                writeln!(out, "  module:{module}: {}", decision_text(answer))?;
            }
            match (
                &workbook.fixed_clusters,
                recorded.and_then(|a| a.clusters.as_ref()),
            ) {
                (Some(fixed), _) => writeln!(out, "Fixed cluster list: {}", fixed.join(", "))?,
                (None, Some(list)) => writeln!(out, "Recorded cluster list: {}", list.join(", "))?,
                (None, None) => writeln!(out, "No cluster list recorded yet.")?,
            }
        }
        PieceSubject::Routes { modules, .. } => {
            writeln!(out, "Decisions for these modules:")?;
            for module in modules {
                let answer = answers
                    .get(&PieceId::module(module))
                    .and_then(|a| a.modules.get(module));
                let text = answer
                    .map(decision_text)
                    .unwrap_or_else(|| "none yet".into());
                writeln!(out, "  module:{module}: {text}")?;
            }
            if let Some(recorded) = recorded {
                for (id, relationship) in &recorded.relationships {
                    writeln!(
                        out,
                        "Relationship {id}: {}, citing {}. {}",
                        relationship.kind,
                        relationship.cite.join(", "),
                        relationship.reason
                    )?;
                }
                for (route, reason) in &recorded.set_aside {
                    writeln!(out, "Set aside {route}: {reason}")?;
                }
            }
        }
    }
    Ok(Outcome::Done)
}

fn check(
    workbook: &Workbook,
    answers: &answers::Answers,
    args: &mut Args,
    out: &mut dyn Write,
) -> Result<Outcome, CommandError> {
    args.finish()?;
    let review = checking::review(workbook, answers);
    print_findings(out, "Problem", &review.problems)?;
    print_findings(out, "Warning", &review.warnings)?;
    let progress = checking::progress(workbook, answers);
    let open = progress
        .iter()
        .filter(|(_, p)| *p != Progress::Done)
        .count();
    writeln!(
        out,
        "{} problems, {} warnings; {open} of {} pieces not done.",
        review.problems.len(),
        review.warnings.len(),
        progress.len()
    )?;
    Ok(if review.problems.is_empty() {
        Outcome::Done
    } else {
        Outcome::Refused
    })
}

fn submit(
    workbook: &Workbook,
    answers: &answers::Answers,
    args: &mut Args,
    store: &dyn WorkbookStore,
    out: &mut dyn Write,
) -> Result<Outcome, CommandError> {
    if check(workbook, answers, args, out)? == Outcome::Refused {
        writeln!(
            out,
            "Not submitted. Fix the problems above, then submit again."
        )?;
        return Ok(Outcome::Refused);
    }
    store.set_submitted(true)?;
    writeln!(out, "Submitted.")?;
    Ok(Outcome::Done)
}

fn print_findings(out: &mut dyn Write, label: &str, findings: &[Finding]) -> std::io::Result<()> {
    for finding in findings {
        writeln!(
            out,
            "  {label} ({}): {}",
            subject_text(&finding.subject),
            finding.message
        )?;
    }
    Ok(())
}

fn subject_text(subject: &checking::Subject) -> String {
    use checking::Subject;
    match subject {
        Subject::Module(module) => format!("module:{module}"),
        Subject::Clusters => "the cluster list".into(),
        Subject::Relationship(id) => format!("relationship {id}"),
        Subject::Withdrawn(id) => format!("the withdrawal of {id}"),
        Subject::SetAside(route) => format!("{route} as set aside"),
        Subject::Piece(piece) => piece.0.clone(),
    }
}

fn decision_text(answer: &ModuleAnswer) -> String {
    match answer {
        ModuleAnswer::Judged {
            concern,
            kind,
            cluster,
            ..
        } => format!("{kind} in {cluster}: {concern}"),
        ModuleAnswer::Deferred { reason } => format!("deferred: {reason}"),
    }
}

#[cfg(test)]
mod tests;
