//! The command line: `srs <command> ...`. Each subcommand parses into one of
//! the program's own commands; the argument parser stays inside this module.

use crate::draft::DraftCommand;
use crate::implementations::{EvaluatorChoice, Language};
use crate::run::MapCommand;
use clap::{Args, Parser, Subcommand, ValueEnum};
use project_graph::BuildSelection;
use std::net::SocketAddr;
use std::path::PathBuf;

/// A command the program can run.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Command {
    Map(MapCommand),
    Draft(DraftCommand),
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

/// Parses the program's arguments, starting with the program name.
pub fn parse<I, T>(args: I) -> Result<Command, ParseOutcome>
where
    I: IntoIterator<Item = T>,
    T: Into<std::ffi::OsString> + Clone,
{
    let cli = Cli::try_parse_from(args).map_err(|error| ParseOutcome {
        text: error.render().to_string(),
        requested: !error.use_stderr(),
    })?;
    Ok(match cli.command {
        CliCommand::Map(args) => Command::Map(args.into()),
        CliCommand::Draft(args) => Command::Draft(args.command.into()),
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
    /// Work with a draft: proposed changes to a saved map, or a map made
    /// only of proposals.
    Draft(DraftArgs),
}

#[derive(Args)]
struct DraftArgs {
    #[command(subcommand)]
    command: DraftSubcommand,
}

#[derive(Subcommand)]
enum DraftSubcommand {
    /// Report the draft's problems, with file, line and column where known.
    /// Exits with status 1 if there are any. A saved map is checked as it is.
    Check {
        /// The draft file.
        draft: PathBuf,
    },
    /// Write the draft's map page to a file. Refuses a draft with problems.
    /// Given a saved map, writes its page as it is.
    Render {
        /// The draft file.
        draft: PathBuf,
        /// Where to write the page.
        #[arg(long)]
        out: PathBuf,
    },
    /// Serve a live preview of the draft, rebuilt and reloaded whenever the
    /// draft or its base map changes, until stopped. Given a saved map,
    /// previews it as it is.
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
    /// The build target, in the terms of the language's build tool.
    #[arg(long)]
    target: String,
    /// The settings that select the build, such as features or a platform.
    #[arg(long)]
    configuration: String,
    /// Who judges the graph.
    #[arg(long, value_enum)]
    evaluator: EvaluatorArg,
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
enum EvaluatorArg {
    Manual,
    Llm,
}

impl From<MapArgs> for MapCommand {
    fn from(args: MapArgs) -> Self {
        MapCommand {
            selection: BuildSelection {
                root: args.root,
                target: args.target,
                configuration: args.configuration,
            },
            language: match args.language {
                LanguageArg::Rust => Language::Rust,
                LanguageArg::Typescript => Language::TypeScript,
            },
            evaluator: match args.evaluator {
                EvaluatorArg::Manual => EvaluatorChoice::Manual,
                EvaluatorArg::Llm => EvaluatorChoice::Llm,
            },
            out: args.out,
            page: args.page,
            serve: args.serve,
        }
    }
}

#[cfg(test)]
mod tests;
