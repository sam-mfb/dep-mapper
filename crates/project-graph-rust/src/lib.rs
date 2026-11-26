//! Builds a `ProjectGraph` for one Cargo build from what rustc resolved.
//!
//! The project's own crates are compiled with a rustc wrapper (the driver in
//! `driver/`) that records items, `use` items, paths into other crates, trait
//! impls and calls. This crate turns those records into the graph. Modules
//! are the workspace's packages. Facts are kept when they cross from one
//! module to another or into a crate outside the workspace, including the
//! toolchain's own crates such as `std`.
//!
//! Every such fact is also a route of one hop, from the thing used to its
//! user. A call through a trait object or generic receiver also gets traced
//! routes: the driver records value moves in each body (constructions,
//! conversions to trait objects, stores and reads of struct fields,
//! arguments and returned values), and the `value-routes` tracer follows the
//! receiver back to the types supplied for it.
//!
//! ## Excluding crates
//!
//! [`RustGrapherSettings::excluded`] names outside crates whose routes the
//! graph leaves out, such as `std`. A route is left out when its origin's name starts
//! with one of them: the dependency the project declares, or the toolchain
//! crate. So `serde` also covers `serde::serde_core::de::Deserialize`, which
//! the project reaches through `serde`. Names match with `-` and `_` taken
//! as the same, as Cargo takes them. The facts only those routes cite go
//! too, except facts between the project's own modules. The graph records
//! the list and how much it removed as a coverage gap, and its ID ends with
//! `excluding` and the list. An empty list keeps everything and records
//! nothing. See the `route-exclusion` crate.
//!
//! The driver links the compiler, so it only works with the toolchain named
//! by [`PINNED_TOOLCHAIN`], and the analyzed project must build with it.
//!
//! ## Selecting a build
//!
//! [`BuildSelection::target`] is a package name, optionally with one target
//! as `package/kind:name`, where kind is `lib`, `bin`, `test`, `example` or
//! `bench`. For example `app` or `app/bin:app`.
//!
//! [`BuildSelection::configuration`] is `default features`, or Cargo flags
//! from this list: `--features <list>`, `--no-default-features`,
//! `--all-features`, `--target <triple>`.

mod assemble;
mod cargo;
mod driver;
mod metadata;
mod selection;

pub use cargo::ToolchainCargo;
pub use driver::build_driver;
pub use project_graph_rust_records::Record;

use project_graph::{BuildSelection, GraphingError, ProjectGraph, ProjectGrapher};
use std::path::Path;

/// The toolchain the driver is written against.
pub const PINNED_TOOLCHAIN: &str = "1.98.1";

/// What the grapher needs from Cargo and the project's checkout.
pub trait Cargo {
    /// `cargo metadata --format-version 1` for the project at `root`.
    fn metadata(&self, root: &Path) -> Result<String, GraphingError>;
    /// The checked-out revision, marked when the checkout has local changes.
    fn revision(&self, root: &Path) -> Result<String, GraphingError>;
    /// Compiles the selected build so that every crate in `members` is
    /// compiled through the driver. Returns the records of each compiled
    /// crate, one list per crate.
    fn compile(
        &self,
        root: &Path,
        cargo_args: &[String],
        members: &[String],
    ) -> Result<Vec<Vec<Record>>, GraphingError>;
}

/// How the grapher shapes the graph, given at construction.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RustGrapherSettings {
    /// Outside crates whose routes the graph leaves out, by package or crate
    /// name, as described under "Excluding crates" above. Empty keeps every
    /// route.
    pub excluded: Vec<String>,
}

/// The Rust implementation of `ProjectGrapher`.
pub struct RustGrapher<C> {
    cargo: C,
    settings: RustGrapherSettings,
}

impl<C: Cargo> RustGrapher<C> {
    pub fn new(cargo: C, settings: RustGrapherSettings) -> Self {
        RustGrapher { cargo, settings }
    }
}

impl<C: Cargo> ProjectGrapher for RustGrapher<C> {
    fn build_graph(&self, selection: &BuildSelection) -> Result<ProjectGraph, GraphingError> {
        let args = selection::cargo_args(selection)?;
        let workspace = metadata::parse(&self.cargo.metadata(&selection.root)?)?;
        let package = selection::package(selection);
        if !workspace.members.iter().any(|m| m.name == package) {
            return Err(GraphingError {
                message: format!("`{package}` is not a package in this workspace"),
            });
        }
        let revision = self.cargo.revision(&selection.root)?;
        let names: Vec<String> = workspace.members.iter().map(|m| m.name.clone()).collect();
        let crates = self.cargo.compile(&selection.root, &args, &names)?;
        // A build that compiled nothing through the driver, for example
        // because Cargo found it up to date, would give an empty graph.
        let compiled = crates.iter().any(|records| {
            matches!(records.first(), Some(Record::Unit(unit)) if unit.package == package)
        });
        if !compiled {
            return Err(GraphingError {
                message: format!("the build produced no records for `{package}`"),
            });
        }
        let mut graph = assemble::graph(
            &workspace,
            &crates,
            assemble::Snapshot {
                project: package.to_string(),
                source_revision: revision,
                target: selection.target.clone(),
                configuration: selection.configuration.clone(),
            },
        );
        let excluded = &self.settings.excluded;
        route_exclusion::exclude(&mut graph, excluded, |name| in_crates(excluded, name));
        // Graphs of one build with different lists hold different evidence.
        if !excluded.is_empty() {
            graph.id.0 = format!("{} excluding {}", graph.id.0, excluded.join(","));
        }
        Ok(graph)
    }
}

/// Whether the outside endpoint `name` starts with one of `crates`, with
/// `-` and `_` taken as the same.
fn in_crates(crates: &[String], name: &str) -> bool {
    let first = name.split("::").next().unwrap_or(name).replace('-', "_");
    crates.iter().any(|c| c.replace('-', "_") == first)
}

#[cfg(test)]
mod tests;
