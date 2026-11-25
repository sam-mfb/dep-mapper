//! Traces a value back from the place a contract operation uses it to the
//! places its implementation entered the program. Any language can supply
//! the value moves; the tracer knows nothing about the language.
//!
//! The input is a list of [`Flow`]s, each a move of a value from one endpoint
//! to another, and a list of [`Use`]s, each a call whose receiver is the value
//! to explain. A flow whose source is an item, such as a type being
//! constructed or converted to a trait object or interface, is an origin.
//!
//! For each use, the tracer searches backward from the receiver and returns
//! one witness path to each origin it reaches, or why it stopped.
//!
//! - A flow that carries a call site, a parameter or a returned value, is
//!   matched to its call: a value that came back from a call is followed
//!   into that call's own arguments, not into every caller's.
//! - A flow marked `joins_instances` merges every instance of a place, such
//!   as a field of a type. A path through one is possible, not established:
//!   the value read may come from a different instance than the one written.
//! - Flows without a fact are moves the language crate does not report, such
//!   as copies between temporaries. They connect a path but are not hops.
//!
//! The search is bounded by [`Limits`]. A trace that hits a bound says so.

use project_graph::{Endpoint, Fact};
use std::collections::{BTreeMap, HashMap, VecDeque};

/// One move of a value from `from` to `to`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Flow {
    pub from: Endpoint,
    pub to: Endpoint,
    /// The fact that reports this move, or `None` for a move that connects a
    /// path without being worth a hop of its own.
    pub fact: Option<Fact>,
    pub call: Option<CallLink>,
    /// The destination stands for every instance of a place, so a read from
    /// it may see a different instance than this write.
    pub joins_instances: bool,
}

/// How a flow crosses a call.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum CallLink {
    /// An argument entering the callee at this call site.
    Argument(String),
    /// A returned value leaving the callee at this call site.
    Returned(String),
}

/// A call whose receiver should be explained.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Use {
    pub receiver: Endpoint,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Limits {
    /// The most search states explored for one use.
    pub max_states: usize,
    /// The deepest nesting of calls a path may return through.
    pub max_call_depth: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Trace {
    /// A path from an origin to the receiver. `path` lists flow indexes in
    /// the order the value moved, starting with the origin flow.
    Reached {
        origin: Endpoint,
        path: Vec<usize>,
        /// The path joins instances somewhere.
        possible: bool,
    },
    /// No origin was reached.
    Stopped { reason: String },
}

/// Traces each use. The result has one entry per use, in order, each with
/// one trace per origin reached, or a single `Stopped` trace.
pub fn trace(flows: &[Flow], uses: &[Use], limits: Limits) -> Vec<Vec<Trace>> {
    let mut nodes = Nodes::default();
    let mut into: HashMap<usize, Vec<usize>> = HashMap::new();
    let ends: Vec<(usize, usize)> = flows
        .iter()
        .map(|flow| (nodes.number(&flow.from), nodes.number(&flow.to)))
        .collect();
    for (index, (_, to)) in ends.iter().enumerate() {
        into.entry(*to).or_default().push(index);
    }
    let graph = Graph {
        flows,
        ends: &ends,
        into: &into,
    };
    uses.iter()
        .map(|u| match nodes.find(&u.receiver) {
            Some(receiver) => graph.trace_one(receiver, limits),
            None => vec![Trace::Stopped {
                reason: "no flow reaches the receiver from a known origin".to_string(),
            }],
        })
        .collect()
}

/// Numbers endpoints, which are compared by their full contents.
#[derive(Default)]
struct Nodes {
    numbers: HashMap<String, usize>,
}

impl Nodes {
    fn key(endpoint: &Endpoint) -> String {
        format!("{endpoint:?}")
    }

    fn number(&mut self, endpoint: &Endpoint) -> usize {
        let next = self.numbers.len();
        *self.numbers.entry(Self::key(endpoint)).or_insert(next)
    }

    fn find(&self, endpoint: &Endpoint) -> Option<usize> {
        self.numbers.get(&Self::key(endpoint)).copied()
    }
}

struct Graph<'a> {
    flows: &'a [Flow],
    /// For each flow, the numbers of its source and destination.
    ends: &'a [(usize, usize)],
    /// For each destination, the flows into it.
    into: &'a HashMap<usize, Vec<usize>>,
}

/// A search state: where the value is, the calls it is known to have come
/// back from, innermost last, and whether the path from here to the
/// receiver joins instances. Keeping the last apart means a shorter possible
/// path never hides an established one.
#[derive(Clone, PartialEq, Eq, Hash)]
struct State<'a> {
    at: usize,
    returns: Vec<&'a str>,
    possible: bool,
}

impl<'a> Graph<'a> {
    fn trace_one(&self, receiver: usize, limits: Limits) -> Vec<Trace> {
        let start = State {
            at: receiver,
            returns: Vec::new(),
            possible: false,
        };
        // For each state reached, the flow taken from it toward the receiver.
        let mut came_by: HashMap<State<'a>, Option<(usize, State<'a>)>> = HashMap::new();
        came_by.insert(start.clone(), None);
        let mut queue = VecDeque::from([start]);
        let mut origins: BTreeMap<String, Trace> = BTreeMap::new();
        let mut deepest = false;
        let mut explored = 0;
        while let Some(state) = queue.pop_front() {
            explored += 1;
            if explored > limits.max_states {
                return stopped_or(
                    origins,
                    format!("search stopped after {} states", limits.max_states),
                );
            }
            for &index in self
                .into
                .get(&state.at)
                .map(Vec::as_slice)
                .unwrap_or_default()
            {
                let flow = &self.flows[index];
                let mut returns = state.returns.clone();
                match &flow.call {
                    Some(CallLink::Returned(site)) => {
                        if returns.len() == limits.max_call_depth {
                            deepest = true;
                            continue;
                        }
                        returns.push(site.as_str());
                    }
                    Some(CallLink::Argument(site)) => match returns.last() {
                        Some(last) if *last == site.as_str() => {
                            returns.pop();
                        }
                        Some(_) => continue,
                        None => {}
                    },
                    None => {}
                }
                if matches!(flow.from, Endpoint::Item(_)) {
                    let path = path_from(&came_by, &state, index);
                    let possible = state.possible || flow.joins_instances;
                    let key = Nodes::key(&flow.from);
                    // Keep a path that does not join instances over one that does.
                    let better = match origins.get(&key) {
                        Some(Trace::Reached { possible: true, .. }) => !possible,
                        Some(_) => false,
                        None => true,
                    };
                    if better {
                        origins.insert(
                            key,
                            Trace::Reached {
                                origin: flow.from.clone(),
                                path,
                                possible,
                            },
                        );
                    }
                    continue;
                }
                let next = State {
                    at: self.ends[index].0,
                    returns,
                    possible: state.possible || flow.joins_instances,
                };
                if !came_by.contains_key(&next) {
                    came_by.insert(next.clone(), Some((index, state.clone())));
                    queue.push_back(next);
                }
            }
        }
        let reason = if deepest {
            format!(
                "no origin found within {} nested calls",
                limits.max_call_depth
            )
        } else {
            "no flow reaches the receiver from a known origin".to_string()
        };
        stopped_or(origins, reason)
    }
}

fn stopped_or(origins: BTreeMap<String, Trace>, reason: String) -> Vec<Trace> {
    if origins.is_empty() {
        vec![Trace::Stopped { reason }]
    } else {
        origins.into_values().collect()
    }
}

/// The flows from `first` through `state` to the receiver, in the order the
/// value moved.
fn path_from<'a>(
    came_by: &HashMap<State<'a>, Option<(usize, State<'a>)>>,
    state: &State<'a>,
    first: usize,
) -> Vec<usize> {
    // `came_by` links each state to one discovered before it, so this ends.
    // A flow can appear twice: one function's move, used inside two calls.
    let mut path = vec![first];
    let mut at = state;
    while let Some(Some((index, toward))) = came_by.get(at) {
        path.push(*index);
        at = toward;
    }
    path
}

#[cfg(test)]
mod tests;
