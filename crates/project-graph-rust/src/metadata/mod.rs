//! Reads the parts of `cargo metadata` the graph needs: the workspace's
//! packages and the dependency names each one uses.

use project_graph::GraphingError;
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[derive(Debug, PartialEq, Eq)]
pub struct Workspace {
    pub root: PathBuf,
    pub members: Vec<Member>,
    /// Library crate names of packages outside the workspace, mapped to their
    /// package names. Crates in neither list come with the toolchain.
    pub third_party: BTreeMap<String, String>,
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
    Ok(Workspace {
        root,
        members,
        third_party,
    })
}

#[cfg(test)]
mod tests;
