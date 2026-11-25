//! Where a workbook and its answers are kept between commands. The contract
//! is `WorkbookStore`; `DirStore` keeps them in a directory:
//!
//! - `workbook.json`: the workbook.
//! - `INSTRUCTIONS.md`: how to work through it.
//! - `pieces/<n>-<name>.txt`: each piece's pages, for reading directly.
//! - `answers/<name>.json`: one file per piece with answers, so agents
//!   working on different pieces never write the same file.
//! - `submitted`: present once the answers are submitted.
//!
//! Writes go to a temporary file first and are then renamed into place.

use crate::answers::{Answers, PieceAnswers};
use crate::workbook::{PieceId, Workbook};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

pub trait WorkbookStore {
    /// Where the workbook is, for people and agents to find it.
    fn location(&self) -> String;
    /// The stored workbook, or `None` if there is none yet.
    fn load_workbook(&self) -> Result<Option<Workbook>, StoreError>;
    /// Stores a workbook and its instructions. Stored answers are kept.
    fn save_workbook(&self, workbook: &Workbook, instructions: &str) -> Result<(), StoreError>;
    fn load_answers(&self) -> Result<Answers, StoreError>;
    fn save_answers(&self, piece: &PieceId, answers: &PieceAnswers) -> Result<(), StoreError>;
    fn submitted(&self) -> Result<bool, StoreError>;
    fn set_submitted(&self, submitted: bool) -> Result<(), StoreError>;
}

/// A failure to read or write the store.
#[derive(Debug, thiserror::Error)]
#[error("{action} {path}: {cause}")]
pub struct StoreError {
    pub action: &'static str,
    pub path: String,
    pub cause: String,
}

/// Keeps a workbook in a directory.
pub struct DirStore {
    root: PathBuf,
}

/// An answers file names its piece, so file names need not be read back.
#[derive(Serialize, Deserialize)]
struct AnswersFile {
    piece: PieceId,
    answers: PieceAnswers,
}

impl DirStore {
    pub fn new(root: impl Into<PathBuf>) -> DirStore {
        DirStore { root: root.into() }
    }

    fn path(&self, parts: &[&str]) -> PathBuf {
        parts
            .iter()
            .fold(self.root.clone(), |path, part| path.join(part))
    }
}

impl WorkbookStore for DirStore {
    fn location(&self) -> String {
        self.root.display().to_string()
    }

    fn load_workbook(&self) -> Result<Option<Workbook>, StoreError> {
        let path = self.path(&["workbook.json"]);
        if !path.exists() {
            return Ok(None);
        }
        let text = read(&path)?;
        serde_json::from_str(&text)
            .map(Some)
            .map_err(|e| error("read", &path, e))
    }

    fn save_workbook(&self, workbook: &Workbook, instructions: &str) -> Result<(), StoreError> {
        let pieces = self.path(&["pieces"]);
        if pieces.exists() {
            fs::remove_dir_all(&pieces).map_err(|e| error("clear", &pieces, e))?;
        }
        for (index, piece) in workbook.pieces.iter().enumerate() {
            let name = format!("{:03}-{}.txt", index + 1, file_name(&piece.id.0));
            write(&pieces.join(name), &piece.pages.join("\n\n----\n\n"))?;
        }
        write(&self.path(&["INSTRUCTIONS.md"]), instructions)?;
        let text =
            serde_json::to_string_pretty(workbook).map_err(|e| error("encode", &self.root, e))?;
        write(&self.path(&["workbook.json"]), &text)
    }

    fn load_answers(&self) -> Result<Answers, StoreError> {
        let dir = self.path(&["answers"]);
        let mut answers = Answers::new();
        if !dir.exists() {
            return Ok(answers);
        }
        let mut paths: Vec<PathBuf> = fs::read_dir(&dir)
            .map_err(|e| error("list", &dir, e))?
            .map(|entry| entry.map(|e| e.path()).map_err(|e| error("list", &dir, e)))
            .collect::<Result<_, _>>()?;
        paths.sort();
        for path in paths
            .into_iter()
            .filter(|p| p.extension().is_some_and(|e| e == "json"))
        {
            let file: AnswersFile =
                serde_json::from_str(&read(&path)?).map_err(|e| error("read", &path, e))?;
            if answers.insert(file.piece.clone(), file.answers).is_some() {
                return Err(error(
                    "read",
                    &path,
                    format!("a second answers file for {}", file.piece.0),
                ));
            }
        }
        Ok(answers)
    }

    fn save_answers(&self, piece: &PieceId, answers: &PieceAnswers) -> Result<(), StoreError> {
        let path = self.path(&["answers", &format!("{}.json", file_name(&piece.0))]);
        if path.exists() {
            let existing: AnswersFile =
                serde_json::from_str(&read(&path)?).map_err(|e| error("read", &path, e))?;
            if &existing.piece != piece {
                let cause = format!("the file holds {}, not {}", existing.piece.0, piece.0);
                return Err(error("write", &path, cause));
            }
        }
        let file = AnswersFile {
            piece: piece.clone(),
            answers: answers.clone(),
        };
        let text = serde_json::to_string_pretty(&file).map_err(|e| error("encode", &path, e))?;
        write(&path, &text)
    }

    fn submitted(&self) -> Result<bool, StoreError> {
        Ok(self.path(&["submitted"]).exists())
    }

    fn set_submitted(&self, submitted: bool) -> Result<(), StoreError> {
        let path = self.path(&["submitted"]);
        match (submitted, path.exists()) {
            (true, false) => write(&path, ""),
            (false, true) => fs::remove_file(&path).map_err(|e| error("remove", &path, e)),
            _ => Ok(()),
        }
    }
}

/// A file name for a piece. Lowercase letters, digits, `-` and `.` stay as
/// they are. Every other byte of the name's UTF-8, including uppercase
/// letters and `_`, becomes `_` and two hex digits. Each `_` therefore starts
/// exactly one escape, so distinct pieces get distinct names, even on file
/// systems that ignore case.
fn file_name(piece: &str) -> String {
    piece
        .bytes()
        .map(|b| match b {
            b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' => (b as char).to_string(),
            other => format!("_{other:02x}"),
        })
        .collect()
}

fn error(action: &'static str, path: &Path, cause: impl ToString) -> StoreError {
    StoreError {
        action,
        path: path.display().to_string(),
        cause: cause.to_string(),
    }
}

fn read(path: &Path) -> Result<String, StoreError> {
    fs::read_to_string(path).map_err(|e| error("read", path, e))
}

fn write(path: &Path, text: &str) -> Result<(), StoreError> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(|e| error("create", dir, e))?;
    }
    let temporary = path.with_extension("partial");
    fs::write(&temporary, text).map_err(|e| error("write", &temporary, e))?;
    fs::rename(&temporary, path).map_err(|e| error("write", path, e))
}

#[cfg(test)]
mod tests;
