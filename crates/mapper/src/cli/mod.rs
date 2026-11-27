//! The command line: `srs <command> ...`. Each subcommand parses into one of
//! the program's own commands; the argument parser stays inside this module.

use crate::draft::DraftCommand;
use crate::eval::EvalCommand;
use crate::implementations::{
    Agent, DEFAULT_RUST_EXCLUDED, DEFAULT_TYPESCRIPT_EXCLUDED, EvaluatorChoice, Language,
};
use crate::run::{LayerSource, MapCommand};
use crate::show::ShowCommand;
use clap::error::ErrorKind;
use clap::{Args, CommandFactory, Parser, Subcommand, ValueEnum};
use project_graph::BuildSelection;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};

/// A command the program can run.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Command {
    Map(MapCommand),
    Eval(EvalCommand),
    Draft(DraftCommand),
    Show(ShowCommand),
}

/// Why the arguments could not be parsed, or a request for help or the
/// version. Its text is ready to print as is.
#[derive(Debug)]
pub struct ParseOutcome {
    pub text: String,
    /// Whether the text answers a request (help or version) rather than
    /// reporting a mistake.
    pub requested: bool,
}

/// Parses the program's arguments, starting with the program name. A
/// relative workbook path is taken from `current_dir`, so that a worker
/// started in another directory, such as the project root, finds it.
pub fn parse<I, T>(args: I, current_dir: &Path) -> Result<Command, ParseOutcome>
where
    I: IntoIterator<Item = T>,
    T: Into<std::ffi::OsString> + Clone,
{
    let outcome = |error: clap::Error| ParseOutcome {
        text: error.render().to_string(),
        requested: !error.use_stderr(),
    };
    let cli = Cli::try_parse_from(args).map_err(outcome)?;
    Ok(match cli.command {
        CliCommand::Map(args) => Command::Map(args.into_command(current_dir).map_err(outcome)?),
        CliCommand::Eval(args) => Command::Eval(EvalCommand {
            workbook: current_dir.join(args.workbook),
            arguments: args.arguments,
        }),
        CliCommand::Draft(args) => Command::Draft(args.command.into()),
        CliCommand::Show(args) => Command::Show(ShowCommand {
            map: args.map,
            page: args.page,
            serve: args.serve,
        }),
    })
}

/// Maps a project's packages and how their work reaches each other.
#[derive(Parser)]
#[command(name = "srs", version)]
struct Cli {
    #[command(subcommand)]
    command: CliCommand,
}

#[derive(Subcommand)]
enum CliCommand {
    /// Analyze a project build, evaluate it and show its map.
    Map(MapArgs),
    /// Read a manual evaluation's workbook and record answers. Run
    /// `srs eval --workbook <dir> help` for its commands.
    Eval(EvalArgs),
    /// Work with a draft: proposed changes to a saved map, or a map made
    /// only of proposals.
    Draft(DraftArgs),
    /// Reopen a map saved by `srs map --out`, without analysis, and write or
    /// serve its page.
    Show(ShowArgs),
}

#[derive(Args)]
struct ShowArgs {
    /// The saved map.
    map: PathBuf,
    /// Write the map's page to this file.
    #[arg(long)]
    page: Option<PathBuf>,
    /// Serve the map's page at this address, such as 127.0.0.1:8080, until stopped.
    #[arg(long)]
    serve: Option<SocketAddr>,
}

#[derive(Args)]
struct EvalArgs {
    /// The workbook directory given to `srs map --workbook`.
    #[arg(long)]
    workbook: PathBuf,
    /// The answer command and its arguments.
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    arguments: Vec<String>,
}

#[derive(Args)]
struct DraftArgs {
    #[command(subcommand)]
    command: DraftSubcommand,
}

#[derive(Subcommand)]
enum DraftSubcommand {
    /// Report the draft's problems, with file, line and column where known.
    /// Exits with status 1 if there are any.
    Check {
        /// The draft file.
        draft: PathBuf,
    },
    /// Write the draft's map page to a file. Refuses a draft with problems.
    Render {
        /// The draft file.
        draft: PathBuf,
        /// Where to write the page.
        #[arg(long)]
        out: PathBuf,
    },
    /// Serve a live preview of the draft, rebuilt and reloaded whenever the
    /// draft or its base map changes, until stopped.
    Preview {
        /// The draft file.
        draft: PathBuf,
        /// Where to serve the preview.
        #[arg(long, default_value = "127.0.0.1:8000")]
        listen: SocketAddr,
    },
}

impl From<DraftSubcommand> for DraftCommand {
    fn from(command: DraftSubcommand) -> Self {
        match command {
            DraftSubcommand::Check { draft } => DraftCommand::Check { draft },
            DraftSubcommand::Render { draft, out } => DraftCommand::Render { draft, out },
            DraftSubcommand::Preview { draft, listen } => DraftCommand::Preview { draft, listen },
        }
    }
}

#[derive(Args)]
struct MapArgs {
    /// The directory holding the project's source.
    root: PathBuf,
    /// The project's language.
    #[arg(long, value_enum)]
    language: LanguageArg,
    /// For Rust: where to keep the grapher's driver and build output between
    /// runs. Without it, `<root>/target/srs`.
    #[arg(long)]
    cache: Option<PathBuf>,
    /// Outside crates or packages whose routes the graph leaves out,
    /// separated by commas. Replaces the default list: for Rust, std, core,
    /// alloc, thiserror, futures, serde, serde_json, tokio and tokio-util;
    /// for TypeScript, none. `--exclude ''` keeps every route.
    #[arg(long)]
    exclude: Option<String>,
    /// The build target, in the terms of the language's build tool.
    #[arg(long)]
    target: String,
    /// The settings that select the build, such as features or a platform.
    #[arg(long)]
    configuration: String,
    /// Who judges the graph.
    #[arg(long, value_enum)]
    evaluator: EvaluatorArg,
    /// For the manual evaluator: the directory for the workbook and its
    /// answers. Running again with the same directory resumes it.
    #[arg(long)]
    workbook: Option<PathBuf>,
    /// For the manual evaluator: who answers while this command runs. `none`
    /// stops after writing the workbook; run again once it is submitted.
    /// `wait` waits for a submission. `claude` runs Claude Code.
    #[arg(long, value_enum)]
    agent: Option<AgentArg>,
    /// Fix the map's layers in advance, from top to bottom, separated by
    /// commas. Every module judgment must name one of them. Without this or
    /// --draft, the evaluator proposes the layers.
    #[arg(long, conflicts_with = "draft")]
    layers: Option<String>,
    /// Fix the map's layers in advance from this draft, laid over its base
    /// map: the combined map's layers, from top to bottom. Each module's
    /// layer there is shown to the evaluator as a hint.
    #[arg(long)]
    draft: Option<PathBuf>,
    /// Save the map to this file, to reopen without analysis.
    #[arg(long)]
    out: PathBuf,
    /// Write the map's page to this file.
    #[arg(long)]
    page: Option<PathBuf>,
    /// Serve the map's page at this address, such as 127.0.0.1:8080, until stopped.
    #[arg(long)]
    serve: Option<SocketAddr>,
}

#[derive(Clone, Copy, ValueEnum)]
enum LanguageArg {
    Rust,
    Typescript,
}

#[derive(Clone, Copy, ValueEnum)]
enum AgentArg {
    None,
    Wait,
    Claude,
}

#[derive(Clone, Copy, ValueEnum)]
enum EvaluatorArg {
    Manual,
    Llm,
}

/// The Rust grapher's cache when `--cache` is not given, under the root.
const DEFAULT_RUST_CACHE: &str = "target/srs";

impl MapArgs {
    fn into_command(self, current_dir: &Path) -> Result<MapCommand, clap::Error> {
        let evaluator = match (self.evaluator, self.workbook) {
            (EvaluatorArg::Manual, Some(workbook)) => EvaluatorChoice::Manual {
                workbook: current_dir.join(workbook),
                // Without --agent, the workbook is left for a later run.
                agent: match self.agent {
                    None | Some(AgentArg::None) => Agent::None,
                    Some(AgentArg::Wait) => Agent::Wait,
                    Some(AgentArg::Claude) => Agent::Claude,
                },
            },
            (EvaluatorArg::Manual, None) => {
                return Err(Cli::command().error(
                    ErrorKind::MissingRequiredArgument,
                    "--evaluator manual needs --workbook <dir>",
                ));
            }
            (EvaluatorArg::Llm, None) if self.agent.is_none() => EvaluatorChoice::Llm,
            (EvaluatorArg::Llm, _) => {
                return Err(Cli::command().error(
                    ErrorKind::ArgumentConflict,
                    "--workbook and --agent apply only to --evaluator manual",
                ));
            }
        };
        let excluded = |default: &[&str]| match &self.exclude {
            Some(list) => list
                .split(',')
                .map(str::trim)
                .filter(|name| !name.is_empty())
                .map(String::from)
                .collect(),
            None => default.iter().map(|name| name.to_string()).collect(),
        };
        let language = match (self.language, &self.cache) {
            (LanguageArg::Rust, Some(cache)) => Language::Rust {
                cache: current_dir.join(cache),
                excluded: excluded(&DEFAULT_RUST_EXCLUDED),
            },
            (LanguageArg::Rust, None) => Language::Rust {
                cache: current_dir.join(&self.root).join(DEFAULT_RUST_CACHE),
                excluded: excluded(&DEFAULT_RUST_EXCLUDED),
            },
            (LanguageArg::Typescript, None) => Language::TypeScript {
                excluded: excluded(&DEFAULT_TYPESCRIPT_EXCLUDED),
            },
            (LanguageArg::Typescript, Some(_)) => {
                return Err(Cli::command().error(
                    ErrorKind::ArgumentConflict,
                    "--cache applies only to --language rust",
                ));
            }
        };
        let layers = match (self.layers, self.draft) {
            (Some(list), _) => LayerSource::Listed(layer_list(&list)?),
            (None, Some(draft)) => LayerSource::Draft(current_dir.join(draft)),
            (None, None) => LayerSource::Proposed,
        };
        Ok(MapCommand {
            selection: BuildSelection {
                root: self.root,
                target: self.target,
                configuration: self.configuration,
            },
            language,
            evaluator,
            layers,
            out: self.out,
            page: self.page,
            serve: self.serve,
        })
    }
}

/// The names in a `--layers` list: at least one, none empty or repeated.
fn layer_list(list: &str) -> Result<Vec<String>, clap::Error> {
    let names: Vec<String> = list
        .split(',')
        .map(|name| name.trim().to_string())
        .collect();
    let invalid = |message: String| Cli::command().error(ErrorKind::InvalidValue, message);
    if names.iter().any(String::is_empty) {
        return Err(invalid(
            "--layers needs layer names separated by commas, none of them empty".into(),
        ));
    }
    for (index, name) in names.iter().enumerate() {
        if names[..index].contains(name) {
            return Err(invalid(format!("--layers lists {name:?} more than once")));
        }
    }
    Ok(names)
}

#[cfg(test)]
mod tests;
