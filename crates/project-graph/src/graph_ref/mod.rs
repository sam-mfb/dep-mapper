//! References to the things in a graph that can be cited. A reference does
//! not reveal what kind of thing it names. Holders can compare, store and pass
//! it along; only the graph can say what it points to.

use crate::{Connection, ConnectionId, Evidence, EvidenceId, Item, ItemId, ProjectGraph};

/// Names something in one graph that a judgment can cite.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct GraphRef(Target);

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Target {
    Connection(ConnectionId),
    Evidence(EvidenceId),
    Item(ItemId),
}

impl From<ConnectionId> for GraphRef {
    fn from(id: ConnectionId) -> Self {
        GraphRef(Target::Connection(id))
    }
}

impl From<EvidenceId> for GraphRef {
    fn from(id: EvidenceId) -> Self {
        GraphRef(Target::Evidence(id))
    }
}

impl From<ItemId> for GraphRef {
    fn from(id: ItemId) -> Self {
        GraphRef(Target::Item(id))
    }
}

/// What a reference points to in the graph.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cited<'a> {
    Connection(&'a Connection),
    Evidence(&'a Evidence),
    Item(&'a Item),
}

impl ProjectGraph {
    /// What the reference points to, or `None` if this graph has no such thing.
    pub fn resolve(&self, reference: &GraphRef) -> Option<Cited<'_>> {
        match &reference.0 {
            Target::Connection(id) => self.connections.get(id).map(Cited::Connection),
            Target::Evidence(id) => self.evidence.get(id).map(Cited::Evidence),
            Target::Item(id) => self.items.get(id).map(Cited::Item),
        }
    }
}

#[cfg(test)]
mod tests;
