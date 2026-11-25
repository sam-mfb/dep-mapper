//! Asks a language model, through the Claude API, to evaluate a project
//! graph. This first part holds the model connection; the evaluator that
//! implements `GraphEvaluator` on top of it follows.
//!
//! The parts, each behind its own contract:
//! - `ModelClient` asks a model one question and gets one answer in JSON
//!   that follows a schema. `AnthropicClient` implements it over the Claude
//!   Messages API. `RetryingClient` adds retries after transient failures.
//! - `HttpTransport` sends one HTTP request. `UreqTransport` implements it
//!   with the `ureq` library. `AnthropicClient` takes one at construction.
//!
//! Tests use fakes for both contracts. `examples/live_check.rs` makes one real
//! request when `ANTHROPIC_API_KEY` is set.

mod anthropic;
mod http;
mod model;
mod retry;

pub use anthropic::{AnthropicClient, AnthropicConfig};
pub use http::{HttpRequest, HttpResponse, HttpTransport, TransportError, UreqTransport};
pub use model::{
    AssistantTurn, ModelClient, ModelError, ModelReply, ModelRequest, ProviderRecord, Turn,
};
pub use retry::{Pause, RetryingClient, ThreadPause};
