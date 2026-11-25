//! Runs Cargo and git for real, with the pinned toolchain and the driver.

use crate::{Cargo, Record};
use project_graph::GraphingError;
use project_graph_rust_records::{RECORDS_DIR_VAR, read_records};
use std::fs::{self, File};
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

/// Runs the project's build with `toolchain` through `rustup run`, using the
/// driver at `driver`. Build products and records go under `work_dir`, which
/// is reused between runs so dependencies are not rebuilt each time.
pub struct ToolchainCargo {
    pub toolchain: String,
    pub driver: PathBuf,
    pub work_dir: PathBuf,
}

impl ToolchainCargo {
    fn cargo(&self, root: &Path) -> Command {
        let mut command = Command::new("rustup");
        command
            .args(["run", &self.toolchain, "cargo"])
            .current_dir(root);
        command
    }

    fn target_dir(&self) -> PathBuf {
        self.work_dir.join("target")
    }

    fn records_dir(&self) -> PathBuf {
        self.work_dir.join("records")
    }
}

pub(crate) fn run(command: &mut Command, what: &str) -> Result<Output, GraphingError> {
    let output = command.output().map_err(|e| GraphingError {
        message: format!("{what}: {e}"),
    })?;
    if !output.status.success() {
        return Err(GraphingError {
            message: format!(
                "{what} failed ({}):\n{}",
                output.status,
                String::from_utf8_lossy(&output.stderr)
            ),
        });
    }
    Ok(output)
}

fn io_error(what: &str, path: &Path, error: impl std::fmt::Display) -> GraphingError {
    GraphingError {
        message: format!("{what} {}: {error}", path.display()),
    }
}

/// The `cargo clean` flags that clear `members` for the same build as
/// `cargo_args`. Cargo keeps each `--target` platform's products apart, so
/// the platform is passed on.
fn clean_args(cargo_args: &[String], members: &[String]) -> Vec<String> {
    let mut args: Vec<String> = members
        .iter()
        .flat_map(|m| ["-p".to_string(), m.clone()])
        .collect();
    if let Some(at) = cargo_args.iter().position(|a| a == "--target")
        && let Some(triple) = cargo_args.get(at + 1)
    {
        args.extend(["--target".to_string(), triple.clone()]);
    }
    args
}

impl Cargo for ToolchainCargo {
    fn metadata(&self, root: &Path) -> Result<String, GraphingError> {
        let output = run(
            self.cargo(root)
                .args(["metadata", "--format-version", "1", "--locked"]),
            "cargo metadata",
        )?;
        String::from_utf8(output.stdout).map_err(|e| GraphingError {
            message: format!("cargo metadata: {e}"),
        })
    }

    fn revision(&self, root: &Path) -> Result<String, GraphingError> {
        let git = |args: &[&str]| -> Result<String, GraphingError> {
            let output = run(Command::new("git").args(args).current_dir(root), "git")?;
            Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
        };
        let head = git(&["rev-parse", "HEAD"])?;
        let changed = !git(&["status", "--porcelain", "--untracked-files=normal", "."])?.is_empty();
        Ok(if changed {
            format!("{head}+local-changes")
        } else {
            head
        })
    }

    fn compile(
        &self,
        root: &Path,
        cargo_args: &[String],
        members: &[String],
    ) -> Result<Vec<Vec<Record>>, GraphingError> {
        let records = self.records_dir();
        if records.exists() {
            fs::remove_dir_all(&records).map_err(|e| io_error("cannot clear", &records, e))?;
        }
        fs::create_dir_all(&records).map_err(|e| io_error("cannot create", &records, e))?;
        // Cargo skips crates it has already checked, and a skipped crate
        // writes no records. Clearing the members forces them through the
        // driver while dependencies stay cached.
        run(
            self.cargo(root)
                .args(["clean", "--target-dir"])
                .arg(self.target_dir())
                .args(clean_args(cargo_args, members)),
            "cargo clean",
        )?;
        run(
            self.cargo(root)
                .args(["check", "--locked", "--target-dir"])
                .arg(self.target_dir())
                .args(cargo_args)
                .env("RUSTC_WORKSPACE_WRAPPER", &self.driver)
                .env(RECORDS_DIR_VAR, &records),
            "cargo check",
        )?;
        let mut crates = Vec::new();
        let mut files: Vec<PathBuf> = fs::read_dir(&records)
            .map_err(|e| io_error("cannot read", &records, e))?
            .map(|entry| entry.map(|e| e.path()))
            .collect::<Result<_, _>>()
            .map_err(|e| io_error("cannot read", &records, e))?;
        files.sort();
        for file in files {
            let reader = File::open(&file).map_err(|e| io_error("cannot open", &file, e))?;
            crates.push(
                read_records(BufReader::new(reader))
                    .map_err(|e| io_error("cannot read", &file, e))?,
            );
        }
        Ok(crates)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strings(words: &[&str]) -> Vec<String> {
        words.iter().map(|w| w.to_string()).collect()
    }

    #[test]
    fn clean_clears_every_member_for_the_selected_platform() {
        assert_eq!(
            clean_args(
                &strings(&[
                    "-p",
                    "app",
                    "--features",
                    "x",
                    "--target",
                    "x86_64-unknown-linux-gnu"
                ]),
                &strings(&["app", "lib"]),
            ),
            strings(&[
                "-p",
                "app",
                "-p",
                "lib",
                "--target",
                "x86_64-unknown-linux-gnu"
            ])
        );
        assert_eq!(
            clean_args(&strings(&["-p", "app"]), &strings(&["app"])),
            strings(&["-p", "app"])
        );
    }
}
