//! The command line: `srs <command> ...`. Each subcommand parses into one of
//! the program's own commands; the argument parser stays inside this module.

use crate::eval::EvalCommand;
use crate::implementations::{Agent, EvaluatorChoice, Language};
use crate::run::MapCommand;
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
struct MapArgs {
    /// The directory holding the project's source.
    root: PathBuf,
    /// The project's language.
    #[arg(long, value_enum)]
    language: LanguageArg,
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
        Ok(MapCommand {
            selection: BuildSelection {
                root: self.root,
                target: self.target,
                configuration: self.configuration,
            },
            language: match self.language {
                LanguageArg::Rust => Language::Rust,
                LanguageArg::Typescript => Language::TypeScript,
            },
            evaluator,
            out: self.out,
            page: self.page,
            serve: self.serve,
        })
    }
}

#[cfg(test)]
mod tests;
