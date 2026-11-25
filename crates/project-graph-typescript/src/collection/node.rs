use super::records::{self, Record};
use super::{CollectError, CollectRequest, Collector};
use std::path::{Path, PathBuf};
use std::process::Command;

/// Runs `collector/src/main.ts` with Node. The collector's dependencies,
/// pinned by its package-lock.json, must be installed (`npm ci` in the
/// collector directory).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NodeCollector {
    /// The Node executable, by path or by name on `PATH`.
    pub node: PathBuf,
    /// The directory holding the collector's package.json.
    pub collector: PathBuf,
}

impl NodeCollector {
    /// The collector shipped in this crate's source tree.
    pub fn bundled(node: PathBuf) -> Self {
        NodeCollector {
            node,
            collector: Path::new(env!("CARGO_MANIFEST_DIR")).join("collector"),
        }
    }
}

impl Collector for NodeCollector {
    fn collect(&self, request: &CollectRequest) -> Result<Vec<Record>, CollectError> {
        if !self.collector.join("node_modules/typescript").is_dir() {
            return Err(CollectError::Start(format!(
                "TypeScript is not installed for the collector; run `npm ci` in {}",
                self.collector.display()
            )));
        }
        let output = Command::new(&self.node)
            .arg(self.collector.join("src/main.ts"))
            .arg("--root")
            .arg(&request.root)
            .arg("--tsconfig")
            .arg(&request.tsconfig)
            .output()
            .map_err(|e| CollectError::Start(format!("{}: {e}", self.node.display())))?;
        if !output.status.success() {
            return Err(CollectError::Failed(
                String::from_utf8_lossy(&output.stderr).trim().to_string(),
            ));
        }
        let stdout = String::from_utf8(output.stdout)
            .map_err(|e| CollectError::Failed(format!("output is not UTF-8: {e}")))?;
        stdout
            .lines()
            .enumerate()
            .map(|(index, line)| {
                records::parse(line).map_err(|e| CollectError::Unreadable {
                    line: index + 1,
                    message: e.to_string(),
                })
            })
            .collect()
    }
}
