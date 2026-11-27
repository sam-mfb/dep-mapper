//! A `GraphEvaluator` that asks a language model, through the Claude API, to
//! judge the modules of a project graph and the relationships its routes
//! show.
//!
//! `LlmEvaluator` presents the graph with `graph-evaluation-assembly`, the same
//! pieces, handles and checks the manual evaluator uses. It asks the model
//! about one piece at a time and sends refused answers back with their
//! problems. Its parts, each behind its own contract and injected:
//! - `ModelClient` asks a model one question and gets one answer in JSON
//!   that follows a schema. `AnthropicClient` implements it over the Claude
//!   Messages API. `RetryingClient` adds retries after transient failures.
//! - `HttpTransport` sends one HTTP request. `UreqTransport` implements it
//!   with the `ureq` library. `AnthropicClient` takes one at construction.
//! - `SourceText` gives source excerpts for the spans in the graph.
//!   `ProjectFiles` reads them from the project's checkout.
//! - `EvaluationLog` receives progress and warnings. `StderrLog` prints them.
//!
//! Tests use fakes for every contract. `examples/live_check.rs` makes one
//! real request when `ANTHROPIC_API_KEY` is set, and `examples/live_evaluate.rs`
//! evaluates a small graph.

mod anthropic;
mod context;
mod evaluator;
mod http;
mod model;
mod retry;
mod source;

pub use anthropic::{AnthropicClient, AnthropicConfig};
pub use evaluator::{EvaluationLog, EvaluatorSettings, LlmEvaluator, LogEvent, StderrLog};
pub use http::{HttpRequest, HttpResponse, HttpTransport, TransportError, UreqTransport};
pub use model::{
    AssistantTurn, ModelClient, ModelError, ModelReply, ModelRequest, ProviderRecord, Turn,
};
pub use retry::{Pause, RetryingClient, ThreadPause};
pub use source::{Excerpt, NoSource, ProjectFiles, SourceError, SourceText};
