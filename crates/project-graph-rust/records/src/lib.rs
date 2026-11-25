//! What the sharsheret rustc driver writes about one compiled crate, and how
//! the records are framed on disk. The driver writes one JSON object per line
//! to a file of its own in the records directory; the grapher reads every file
//! in that directory once the build has finished.
//!
//! The driver writes what the compiler resolved. It does not decide which
//! records matter for a map: it does not know which crates belong to the
//! project or which are third-party.

use serde::{Deserialize, Serialize};
use std::io::{self, BufRead, Write};

/// The name of the environment variable holding the directory the driver
/// writes its record files into.
pub const RECORDS_DIR_VAR: &str = "SRS_RECORDS_DIR";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "record", rename_all = "kebab-case")]
pub enum Record {
    /// Always the first record in a file: the crate this file describes.
    Unit(UnitRecord),
    Item(ItemRecord),
    Use(UseRecord),
    Reference(ReferenceRecord),
    Impl(ImplRecord),
    Call(CallRecord),
    Flow(FlowRecord),
    Receiver(ReceiverRecord),
    /// A body or item the driver could not describe.
    Gap(GapRecord),
}

/// One compiled crate: a Cargo package target with its settings.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnitRecord {
    pub package: String,
    pub crate_name: String,
    /// The compiler's stable identity for this crate; `Def::crate_id` names it.
    pub crate_id: String,
    /// For example `lib`, `bin` or `proc-macro`.
    pub crate_types: Vec<String>,
    /// Compiled as a test harness.
    pub test: bool,
    pub features: Vec<String>,
    pub target_triple: String,
    /// The names this crate's source uses for its dependencies.
    pub externs: Vec<String>,
}

/// A definition, local or in another crate.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Def {
    pub crate_name: String,
    pub crate_id: String,
    /// The definition path without the crate name, unique within the crate.
    pub path: String,
    /// The compiler's readable name for the definition.
    pub name: String,
}

/// A half-open range of UTF-8 byte offsets in a file as the compiler named it.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Span {
    pub file: String,
    pub lo: usize,
    pub hi: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ItemKind {
    Type,
    Value,
    Function,
    Method,
    Module,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ItemRecord {
    pub def: Def,
    pub kind: ItemKind,
    /// Reachable from outside the crate.
    pub exported: bool,
    pub signature: Option<String>,
    pub span: Option<Span>,
}

/// A `use` item naming a definition in another crate.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct UseRecord {
    /// The module that holds the `use` item.
    pub module: Def,
    pub target: Def,
    /// The name the item binds, which differs from the target's when renamed.
    pub local_name: String,
    /// Other crates can name it: a `pub use` reachable from the crate root.
    pub public: bool,
    pub span: Span,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ReferenceKind {
    Type,
    Value,
}

/// A path outside `use` items that resolves to a definition in another crate.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReferenceRecord {
    /// The item whose source contains the path.
    pub owner: Def,
    pub target: Def,
    pub kind: ReferenceKind,
    pub span: Span,
}

/// A local `impl Trait for Type`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImplRecord {
    /// The implementing type when it is a named type, such as a struct.
    pub self_type: Option<Def>,
    pub contract: Def,
    pub span: Span,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Dispatch {
    /// A named function or inherent method.
    Direct,
    /// A trait method for which the compiler selected the implementation.
    SelectedImplementation,
    /// A trait method left at the trait: a trait object or a generic receiver.
    Contract,
    /// A call through a function pointer.
    Indirect,
    /// A function declared in an `extern` block.
    Foreign,
}

/// A call in a function body, including closures, which are reported under
/// the item that contains them.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CallRecord {
    pub owner: Def,
    /// The selected implementation, or the declared operation when none was
    /// selected. Absent for indirect calls.
    pub callee: Option<Def>,
    pub dispatch: Dispatch,
    pub span: Span,
}

/// Where a value is held, as the compiler sees it in a function body.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "node", rename_all = "kebab-case")]
pub enum Node {
    /// A local variable or temporary of `function`.
    Local {
        function: Def,
        local: u32,
        name: Option<String>,
    },
    /// The argument at `index`, from zero, of `function`.
    Param { function: Def, index: u32 },
    /// The value `function` returns.
    Returned { function: Def },
    /// A named field of a struct, standing for that field in every instance.
    Field { owner: Def, field: String },
    /// A value a closure captured, standing for every instance of the closure.
    Capture { closure: Def, index: u32 },
    /// A type whose value is created or converted here.
    Type(Def),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum FlowKind {
    /// A copy, move or borrow between places in one body.
    Copy,
    /// A value of the type is created, such as a struct literal.
    Construct,
    /// A value of the type becomes a trait object.
    Coerce,
    Store,
    Read,
    /// An argument to a call.
    Pass,
    /// The value a call returns, at the call.
    Return,
    Capture,
}

/// One move of a value in a function body.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FlowRecord {
    pub from: Node,
    pub to: Node,
    pub kind: FlowKind,
    /// For `Pass` and `Return`, an identity of the call, unique in the build.
    pub call_site: Option<String>,
    pub span: Option<Span>,
}

/// A call through a trait whose receiver the compiler did not resolve: the
/// receiver to explain.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReceiverRecord {
    pub owner: Def,
    /// The trait method, as in the matching `CallRecord`.
    pub contract: Def,
    pub receiver: Node,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GapRecord {
    pub span: Option<Span>,
    pub reason: String,
}

/// Writes one record as a line.
pub fn write_record(out: &mut impl Write, record: &Record) -> io::Result<()> {
    serde_json::to_writer(&mut *out, record)?;
    out.write_all(b"\n")
}

/// Reads every record in one file, failing at the first line that is not a record.
pub fn read_records(input: impl BufRead) -> Result<Vec<Record>, ReadError> {
    input
        .lines()
        .enumerate()
        .filter(|(_, line)| !matches!(line, Ok(text) if text.is_empty()))
        .map(|(index, line)| {
            let text = line.map_err(|source| ReadError::Io {
                line: index + 1,
                source,
            })?;
            serde_json::from_str(&text).map_err(|source| ReadError::Json {
                line: index + 1,
                source,
            })
        })
        .collect()
}

#[derive(Debug)]
pub enum ReadError {
    Io {
        line: usize,
        source: io::Error,
    },
    Json {
        line: usize,
        source: serde_json::Error,
    },
}

impl std::fmt::Display for ReadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ReadError::Io { line, source } => write!(f, "line {line}: {source}"),
            ReadError::Json { line, source } => write!(f, "line {line}: {source}"),
        }
    }
}

impl std::error::Error for ReadError {}

#[cfg(test)]
mod tests;
