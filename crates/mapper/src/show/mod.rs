//! Running `srs show`: reopens a saved map, without analysis or evaluation,
//! and writes or serves its page.

use crate::draft::describe_map;
use crate::run::{MapError, MapOutputs, present};
use map_persist::{LoadError, MapDocument};
use project_map::MapProblem;
use std::net::SocketAddr;
use std::path::PathBuf;
use thiserror::Error;

/// Which saved map to show, and where.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ShowCommand {
    /// A map saved by `srs map --out`.
    pub map: PathBuf,
    pub page: Option<PathBuf>,
    pub serve: Option<SocketAddr>,
}

#[derive(Debug, Error)]
pub enum ShowError {
    #[error("nothing to do: give --page, --serve or both")]
    NothingToShow,
    #[error("{path}: {error}")]
    Load { path: PathBuf, error: LoadError },
    #[error("{0} is a draft; show it with `srs draft render` or `srs draft preview`")]
    Draft(PathBuf),
    /// The map loaded but its parts do not fit together, so it is not shown.
    #[error(
        "{path} is not shown, because its parts do not fit together:{}",
        list(problems)
    )]
    Problems {
        path: PathBuf,
        problems: Vec<MapProblem>,
    },
    #[error(transparent)]
    Present(#[from] MapError),
}

/// Shows the saved map whose file holds `text`. A map whose parts do not fit
/// together, as `ProjectMap::problems` checks, is refused before anything is
/// written or served.
pub fn show(
    command: &ShowCommand,
    text: &str,
    outputs: &mut dyn MapOutputs,
) -> Result<(), ShowError> {
    if command.page.is_none() && command.serve.is_none() {
        return Err(ShowError::NothingToShow);
    }
    let map = match map_persist::load(text) {
        Ok(MapDocument::Map(map)) => map,
        Ok(MapDocument::Draft(_)) => return Err(ShowError::Draft(command.map.clone())),
        Err(error) => {
            return Err(ShowError::Load {
                path: command.map.clone(),
                error,
            });
        }
    };
    let problems = map.problems();
    if !problems.is_empty() {
        return Err(ShowError::Problems {
            path: command.map.clone(),
            problems,
        });
    }
    Ok(present(
        &map,
        command.page.as_deref(),
        command.serve,
        outputs,
    )?)
}

fn list(problems: &[MapProblem]) -> String {
    problems
        .iter()
        .map(|problem| format!("\n  {}", describe_map(problem)))
        .collect()
}

#[cfg(test)]
mod tests;
