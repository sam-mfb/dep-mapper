//! Reads the parts of `cargo metadata` the graph needs: the workspace's
//! packages and the dependency names each one uses.

use project_graph::GraphingError;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

#[derive(Debug, PartialEq, Eq)]
pub struct Workspace {
    pub root: PathBuf,
    pub members: Vec<Member>,
    /// Library crate names of packages outside the workspace, mapped to their
    /// package names. Crates in neither list come with the toolchain.
    pub third_party: BTreeMap<String, String>,
    /// For every package in the build, the packages it depends on directly.
    /// Every version of a package is merged under its name.
    pub depends_on: BTreeMap<String, BTreeSet<String>>,
}

impl Workspace {
    /// The direct dependency of `member` through which it reaches
    /// `package`: `package` itself when `member` depends on it directly,
    /// otherwise the one direct dependency that depends on it, directly or
    /// not. When several do, the one whose name `package` starts with, as
    /// `serde` for `serde_core` or `clap` for `clap_builder`, if there is
    /// exactly one such. Otherwise `None`.
    pub fn reached_through(&self, member: &str, package: &str) -> Option<String> {
        // Third-party dependencies only: a workspace member is a module of
        // the graph, not a name for an outside package. Each package once,
        // however many names the source gives it.
        let direct: BTreeSet<&String> = self
            .members
            .iter()
            .find(|m| m.name == member)?
            .dependencies
            .values()
            .filter(|d| !d.member)
            .map(|d| &d.package)
            .collect();
        if direct.iter().any(|d| *d == package) {
            return Some(package.to_string());
        }
        let through: Vec<&String> = direct
            .into_iter()
            .filter(|d| self.reaches(d, package))
            .collect();
        let only = |candidates: Vec<&String>| match candidates.as_slice() {
            [one] => Some(one.to_string()),
            _ => None,
        };
        only(through.clone()).or_else(|| {
            only(
                through
                    .into_iter()
                    .filter(|d| {
                        package
                            .strip_prefix(d.as_str())
                            .is_some_and(|rest| rest.starts_with(['_', '-']))
                    })
                    .collect(),
            )
        })
    }

    fn reaches(&self, from: &str, to: &str) -> bool {
        let mut seen = std::collections::BTreeSet::new();
        let mut stack = vec![from];
        while let Some(at) = stack.pop() {
            if at == to {
                return true;
            }
            if seen.insert(at) {
                stack.extend(
                    self.depends_on
                        .get(at)
                        .into_iter()
                        .flatten()
                        .map(String::as_str),
                );
            }
        }
        false
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct Member {
    pub name: String,
    /// The package directory, relative to the workspace root.
    pub root: String,
    /// For each name the package's source uses for a dependency, the
    /// dependency's package name and whether it is a workspace member.
    pub dependencies: BTreeMap<String, Dependency>,
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct Dependency {
    pub package: String,
    pub member: bool,
}

pub fn parse(json: &str) -> Result<Workspace, GraphingError> {
    let fail = |what: &str| GraphingError {
        message: format!("cargo metadata: {what}"),
    };
    let value: Value = serde_json::from_str(json).map_err(|e| fail(&e.to_string()))?;
    let root = PathBuf::from(
        value["workspace_root"]
            .as_str()
            .ok_or_else(|| fail("no workspace_root"))?,
    );
    let member_ids: Vec<&str> = value["workspace_members"]
        .as_array()
        .ok_or_else(|| fail("no workspace_members"))?
        .iter()
        .filter_map(Value::as_str)
        .collect();
    let packages = value["packages"]
        .as_array()
        .ok_or_else(|| fail("no packages"))?;
    let name_of: BTreeMap<&str, &str> = packages
        .iter()
        .filter_map(|p| Some((p["id"].as_str()?, p["name"].as_str()?)))
        .collect();
    let mut third_party = BTreeMap::new();
    for package in packages {
        let (Some(id), Some(name)) = (package["id"].as_str(), package["name"].as_str()) else {
            return Err(fail("a package without an id or name"));
        };
        if member_ids.contains(&id) {
            continue;
        }
        for target in package["targets"].as_array().into_iter().flatten() {
            let is_lib = target["kind"].as_array().is_some_and(|kinds| {
                kinds.iter().any(|k| {
                    k.as_str()
                        .is_some_and(|k| k.ends_with("lib") || k == "proc-macro")
                })
            });
            if is_lib && let Some(crate_name) = target["name"].as_str() {
                third_party.insert(crate_name.replace('-', "_"), name.to_string());
            }
        }
    }
    let nodes = value["resolve"]["nodes"]
        .as_array()
        .ok_or_else(|| fail("no resolve graph"))?;
    let mut members = Vec::new();
    for id in &member_ids {
        let package = packages
            .iter()
            .find(|p| p["id"].as_str() == Some(id))
            .ok_or_else(|| fail("a workspace member missing from packages"))?;
        let manifest = package["manifest_path"]
            .as_str()
            .ok_or_else(|| fail("no manifest_path"))?;
        let dir = Path::new(manifest).parent().unwrap_or(Path::new(""));
        let relative = dir.strip_prefix(&root).unwrap_or(dir);
        let node = nodes
            .iter()
            .find(|n| n["id"].as_str() == Some(id))
            .ok_or_else(|| fail("a workspace member missing from the resolve graph"))?;
        let dependencies = node["deps"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|dep| {
                let pkg = dep["pkg"].as_str()?;
                Some((
                    dep["name"].as_str()?.to_string(),
                    Dependency {
                        package: name_of.get(pkg)?.to_string(),
                        member: member_ids.contains(&pkg),
                    },
                ))
            })
            .collect();
        members.push(Member {
            name: name_of[id].to_string(),
            root: relative.to_string_lossy().into_owned(),
            dependencies,
        });
    }
    let mut depends_on: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for node in nodes {
        let Some(name) = node["id"].as_str().and_then(|id| name_of.get(id)) else {
            continue;
        };
        depends_on.entry(name.to_string()).or_default().extend(
            node["deps"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|dep| name_of.get(dep["pkg"].as_str()?).map(|n| n.to_string())),
        );
    }
    Ok(Workspace {
        root,
        members,
        third_party,
        depends_on,
    })
}

#[cfg(test)]
mod tests;
