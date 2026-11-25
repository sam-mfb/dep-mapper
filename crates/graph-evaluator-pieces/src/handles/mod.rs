//! Written names for the things an answer can mention. A handle is a kind
//! prefix and an identifier from the graph, such as `route:R12`. Handles let
//! an agent outside the process cite evidence; only this crate turns them
//! into `GraphRef`s.

use project_graph::{FactId, GraphRef, ItemId, ModuleId, RouteId};
use std::fmt;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Handle {
    Module(ModuleId),
    Route(RouteId),
    Fact(FactId),
    Item(ItemId),
}

/// A handle that could not be read.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("{text:?} is not a handle: write module:<id>, route:<id>, fact:<id> or item:<id>")]
pub struct MalformedHandle {
    pub text: String,
}

impl Handle {
    pub fn parse(text: &str) -> Result<Handle, MalformedHandle> {
        let malformed = || MalformedHandle { text: text.into() };
        let (kind, id) = text.split_once(':').ok_or_else(malformed)?;
        if id.is_empty() {
            return Err(malformed());
        }
        let id = id.to_string();
        match kind {
            "module" => Ok(Handle::Module(ModuleId(id))),
            "route" => Ok(Handle::Route(RouteId(id))),
            "fact" => Ok(Handle::Fact(FactId(id))),
            "item" => Ok(Handle::Item(ItemId(id))),
            _ => Err(malformed()),
        }
    }

    /// The graph reference this handle cites. Modules are subjects of
    /// judgments, not evidence, so they have none.
    pub fn graph_ref(&self) -> Option<GraphRef> {
        match self {
            Handle::Module(_) => None,
            Handle::Route(id) => Some(id.clone().into()),
            Handle::Fact(id) => Some(id.clone().into()),
            Handle::Item(id) => Some(id.clone().into()),
        }
    }
}

impl fmt::Display for Handle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Handle::Module(id) => write!(f, "module:{}", id.0),
            Handle::Route(id) => write!(f, "route:{}", id.0),
            Handle::Fact(id) => write!(f, "fact:{}", id.0),
            Handle::Item(id) => write!(f, "item:{}", id.0),
        }
    }
}

#[cfg(test)]
mod tests;
