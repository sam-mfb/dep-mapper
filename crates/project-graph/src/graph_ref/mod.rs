//! References to the things in a graph that can be cited. A reference does
//! not reveal what kind of thing it names. Holders can compare, store and pass
//! it along; only the graph can say what it points to.

use crate::{Fact, FactId, Item, ItemId, ProjectGraph, Route, RouteId};

/// Names something in one graph that a judgment can cite.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct GraphRef(Target);

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Target {
    Route(RouteId),
    Fact(FactId),
    Item(ItemId),
}

impl From<RouteId> for GraphRef {
    fn from(id: RouteId) -> Self {
        GraphRef(Target::Route(id))
    }
}

impl From<FactId> for GraphRef {
    fn from(id: FactId) -> Self {
        GraphRef(Target::Fact(id))
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
    Route(&'a Route),
    Fact(&'a Fact),
    Item(&'a Item),
}

impl ProjectGraph {
    /// What the reference points to, or `None` if this graph has no such thing.
    pub fn resolve(&self, reference: &GraphRef) -> Option<Cited<'_>> {
        match &reference.0 {
            Target::Route(id) => self.routes.get(id).map(Cited::Route),
            Target::Fact(id) => self.facts.get(id).map(Cited::Fact),
            Target::Item(id) => self.items.get(id).map(Cited::Item),
        }
    }
}

#[cfg(test)]
mod tests;
