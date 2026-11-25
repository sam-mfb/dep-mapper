//! `ModelClient` over the Claude Messages API. This module alone knows the
//! API's request and response format. It sends each request through an
//! injected `HttpTransport`.
//!
//! Each request asks for an answer in JSON that follows the request's schema
//! (structured output), lets the model decide how much to think, caches the
//! system prompt, and opts into server-side fallback: if the model declines,
//! the API retries on the model Anthropic recommends for that kind of refusal.

use crate::http::{HttpRequest, HttpResponse, HttpTransport};
use crate::model::{
    AssistantTurn, ModelClient, ModelError, ModelReply, ModelRequest, ProviderRecord, Turn,
};
use serde_json::{Value, json};
use std::time::Duration;

/// The API version this module's request and response format follows.
const API_VERSION: &str = "2026-10-21";
/// Enables `fallbacks: "default"`.
const FALLBACK_BETA: &str = "server-side-fallback-2026-07-01";

#[derive(Clone, Debug, PartialEq)]
pub struct AnthropicConfig {
    pub api_key: String,
    /// For example `https://api.anthropic.com`.
    pub base_url: String,
    pub model: String,
    /// The most output tokens one answer may use, reasoning included.
    pub max_tokens: u32,
    /// How thoroughly the model works: `low`, `medium`, `high`, `xhigh` or `max`.
    pub effort: String,
}

impl AnthropicConfig {
    /// The settings this crate recommends: the most capable Opus model at
    /// high effort, with room for a long answer. Every value is stated here so
    /// no API default applies silently.
    pub fn recommended(api_key: String) -> Self {
        AnthropicConfig {
            api_key,
            base_url: "https://api.anthropic.com".into(),
            model: "claude-opus-5-5".into(),
            max_tokens: 32_000,
            effort: "high".into(),
        }
    }
}

pub struct AnthropicClient<T> {
    config: AnthropicConfig,
    transport: T,
}

impl<T: HttpTransport> AnthropicClient<T> {
    pub fn new(config: AnthropicConfig, transport: T) -> Self {
        AnthropicClient { config, transport }
    }
}

impl<T: HttpTransport> ModelClient for AnthropicClient<T> {
    fn reply(&self, request: &ModelRequest) -> Result<ModelReply, ModelError> {
        let http = HttpRequest {
            url: format!("{}/v1/messages", self.config.base_url.trim_end_matches('/')),
            headers: vec![
                ("content-type".into(), "application/json".into()),
                ("x-api-key".into(), self.config.api_key.clone()),
                ("anthropic-version".into(), API_VERSION.into()),
                ("anthropic-beta".into(), FALLBACK_BETA.into()),
            ],
            body: request_body(&self.config, request).to_string(),
        };
        let response = self
            .transport
            .post(&http)
            .map_err(|error| ModelError::Transient {
                message: format!("no response from the Claude API: {error}"),
                retry_after: None,
            })?;
        if response.status != 200 {
            return Err(status_error(&response));
        }
        parse_reply(&response.body)
    }
}

pub(crate) fn request_body(config: &AnthropicConfig, request: &ModelRequest) -> Value {
    let messages: Vec<Value> = request
        .turns
        .iter()
        .map(|turn| match turn {
            Turn::User(text) => json!({"role": "user", "content": text}),
            Turn::Assistant(turn) => {
                let content = match &turn.provider_record {
                    Some(ProviderRecord(content)) => content.clone(),
                    None => json!([{"type": "text", "text": turn.text}]),
                };
                json!({"role": "assistant", "content": content})
            }
        })
        .collect();
    json!({
        "model": config.model,
        "max_tokens": config.max_tokens,
        "system": [{
            "type": "text",
            "text": request.system,
            "cache_control": {"type": "ephemeral"},
        }],
        "messages": messages,
        "thinking": {"type": "adaptive"},
        "output_config": {
            "effort": config.effort,
            "format": {"type": "json_schema", "schema": request.answer_schema},
        },
        "fallbacks": "default",
    })
}

fn status_error(response: &HttpResponse) -> ModelError {
    let detail = serde_json::from_str::<Value>(&response.body)
        .ok()
        .and_then(|body| {
            let error = body.get("error")?;
            Some(format!(
                "{}: {}",
                error.get("type")?.as_str()?,
                error.get("message")?.as_str()?
            ))
        })
        .unwrap_or_else(|| response.body.chars().take(500).collect());
    let message = format!("Claude API returned {}: {detail}", response.status);
    let retryable = match response.header("x-should-retry") {
        Some("true") => true,
        Some("false") => false,
        _ => matches!(response.status, 408 | 409 | 429) || response.status >= 500,
    };
    if retryable {
        ModelError::Transient {
            message,
            retry_after: response
                .header("retry-after")
                .and_then(|value| value.trim().parse::<u64>().ok())
                .map(Duration::from_secs),
        }
    } else {
        ModelError::Permanent { message }
    }
}

pub(crate) fn parse_reply(body: &str) -> Result<ModelReply, ModelError> {
    let malformed = |what: &str| ModelError::Permanent {
        message: format!("unexpected Claude API response: {what}"),
    };
    let body: Value = serde_json::from_str(body).map_err(|error| malformed(&error.to_string()))?;
    let stop_reason = body
        .get("stop_reason")
        .and_then(Value::as_str)
        .ok_or_else(|| malformed("no stop_reason"))?;
    match stop_reason {
        "end_turn" => {
            let content = body
                .get("content")
                .filter(|content| content.is_array())
                .ok_or_else(|| malformed("no content"))?;
            let text: String = content
                .as_array()
                .into_iter()
                .flatten()
                .filter(|block| block.get("type").and_then(Value::as_str) == Some("text"))
                .filter_map(|block| block.get("text").and_then(Value::as_str))
                .collect();
            Ok(ModelReply::Answer(AssistantTurn {
                text,
                provider_record: Some(ProviderRecord(content.clone())),
            }))
        }
        "refusal" => {
            let details = body.get("stop_details");
            let field = |name: &str| {
                details
                    .and_then(|details| details.get(name))
                    .and_then(Value::as_str)
            };
            let explanation = match (field("category"), field("explanation")) {
                (Some(category), Some(text)) => format!("{category}: {text}"),
                (Some(category), None) => category.to_string(),
                (None, Some(text)) => text.to_string(),
                (None, None) => "no explanation given".to_string(),
            };
            Ok(ModelReply::Refused { explanation })
        }
        "max_tokens" | "model_context_window_exceeded" => Ok(ModelReply::Truncated),
        other => Err(malformed(&format!("stop_reason {other}"))),
    }
}

#[cfg(test)]
mod tests;
