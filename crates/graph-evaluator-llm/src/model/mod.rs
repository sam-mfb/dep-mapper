//! The contract for asking a language model one question and getting one
//! answer. A conversation is a system prompt and alternating turns. The model
//! answers in JSON that follows the schema in the request. Provider details,
//! such as how a reply is encoded, stay behind this contract.

use std::time::Duration;
use thiserror::Error;

pub trait ModelClient {
    /// Sends the conversation and returns the model's next turn.
    fn reply(&self, request: &ModelRequest) -> Result<ModelReply, ModelError>;
}

#[derive(Clone, Debug, PartialEq)]
pub struct ModelRequest {
    /// Instructions and context that stay the same for every request in an
    /// evaluation, so a provider can cache them.
    pub system: String,
    /// Starts and ends with a user turn.
    pub turns: Vec<Turn>,
    /// The JSON Schema the answer must follow.
    pub answer_schema: serde_json::Value,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Turn {
    User(String),
    Assistant(AssistantTurn),
}

/// A turn the model wrote, to be sent back unchanged when the conversation
/// continues.
#[derive(Clone, Debug, PartialEq)]
pub struct AssistantTurn {
    /// The answer text.
    pub text: String,
    /// The provider's own record of the turn, such as reasoning blocks it
    /// needs to see again. `None` when there is nothing beyond the text.
    pub provider_record: Option<ProviderRecord>,
}

/// Opaque to everything except the client that produced it.
#[derive(Clone, Debug, PartialEq)]
pub struct ProviderRecord(pub(crate) serde_json::Value);

#[derive(Clone, Debug, PartialEq)]
pub enum ModelReply {
    Answer(AssistantTurn),
    /// The model declined to answer.
    Refused {
        explanation: String,
    },
    /// The answer reached the output limit before it was complete.
    Truncated,
}

#[derive(Clone, Debug, Error, PartialEq)]
pub enum ModelError {
    /// Worth trying again, such as a rate limit, an overloaded service or a
    /// lost connection.
    #[error("temporary model failure: {message}")]
    Transient {
        message: String,
        /// How long the provider asked the caller to wait, when it said.
        retry_after: Option<Duration>,
    },
    /// Trying again will not help, such as a rejected request or a missing
    /// credential.
    #[error("model request failed: {message}")]
    Permanent { message: String },
}
