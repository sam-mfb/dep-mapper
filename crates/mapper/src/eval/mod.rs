//! Running `srs eval`: the manual evaluator's answer commands, for the
//! workbook in one directory.

use crate::implementations::eval_command;
use graph_evaluator_manual::commands::{self, CommandError, Outcome};
use graph_evaluator_manual::store::DirStore;
use std::path::PathBuf;

/// One answer command for one workbook.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EvalCommand {
    /// The directory holding the workbook, as given to `srs map --workbook`.
    pub workbook: PathBuf,
    /// The command and its arguments, such as `show module:core`.
    pub arguments: Vec<String>,
}

/// Runs the command against the workbook, writing what it shows to `out`.
pub fn eval(command: &EvalCommand, out: &mut dyn std::io::Write) -> Result<Outcome, CommandError> {
    commands::run(
        &eval_command(&command.workbook),
        &command.arguments,
        &DirStore::new(command.workbook.clone()),
        out,
    )
}
