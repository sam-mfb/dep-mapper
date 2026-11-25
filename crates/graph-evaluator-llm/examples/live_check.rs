//! Sends one small request to the Claude API and prints the answer, to check
//! that the client works against the live service. Needs `ANTHROPIC_API_KEY`.
//!
//! cargo run -p graph-evaluator-llm --example live_check

use graph_evaluator_llm::{
    AnthropicClient, AnthropicConfig, ModelClient, ModelReply, ModelRequest, RetryingClient,
    ThreadPause, Turn, UreqTransport,
};
use std::process::ExitCode;
use std::time::Duration;

fn main() -> ExitCode {
    let Some(api_key) = std::env::var("ANTHROPIC_API_KEY")
        .ok()
        .filter(|key| !key.is_empty())
    else {
        eprintln!("Set ANTHROPIC_API_KEY to run this check.");
        return ExitCode::FAILURE;
    };
    let config = AnthropicConfig::recommended(api_key);
    let model = config.model.clone();
    let client = RetryingClient::new(
        AnthropicClient::new(config, UreqTransport::new(Duration::from_secs(600))),
        ThreadPause,
        vec![Duration::from_secs(2), Duration::from_secs(8)],
    );
    let request = ModelRequest {
        system: "You answer questions about software architecture briefly.".into(),
        turns: vec![Turn::User(
            "A module only defines traits that other modules implement. Is its kind logic, effect, composition or contract?".into(),
        )],
        answer_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "kind": {"type": "string", "enum": ["logic", "effect", "composition", "contract"]},
                "reason": {"type": "string"},
            },
            "required": ["kind", "reason"],
            "additionalProperties": false,
        }),
    };
    match client.reply(&request) {
        Ok(ModelReply::Answer(turn)) => {
            println!("{model} answered: {}", turn.text);
            match serde_json::from_str::<serde_json::Value>(&turn.text) {
                Ok(_) => ExitCode::SUCCESS,
                Err(error) => {
                    eprintln!("The answer is not JSON: {error}");
                    ExitCode::FAILURE
                }
            }
        }
        Ok(ModelReply::Refused { explanation }) => {
            eprintln!("The model declined: {explanation}");
            ExitCode::FAILURE
        }
        Ok(ModelReply::Truncated) => {
            eprintln!("The answer reached the output limit.");
            ExitCode::FAILURE
        }
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}
