//! Builds a `ProjectGraph` for one Cargo build from what rustc resolved.
//!
//! The project's own crates are compiled with a rustc wrapper (the driver in
//! `driver/`) that records items, `use` items, paths into other crates, trait
//! impls and calls. This crate turns those records into the graph. Modules
//! are the workspace's packages. Facts are kept when they cross from one
//! module to another or into a third-party crate. The standard library is
//! left out.
//!
//! Every fact is also a route of one hop, from the thing used to its user.
//! Multi-hop routes need value-move facts, which come later.
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

/// The Rust implementation of `ProjectGrapher`.
pub struct RustGrapher<C> {
    cargo: C,
}

impl<C: Cargo> RustGrapher<C> {
    pub fn new(cargo: C) -> Self {
        RustGrapher { cargo }
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
        Ok(assemble::graph(
            &workspace,
            &crates,
            assemble::Snapshot {
                project: package.to_string(),
                source_revision: revision,
                target: selection.target.clone(),
                configuration: selection.configuration.clone(),
            },
        ))
    }
}

#[cfg(test)]
mod tests;
