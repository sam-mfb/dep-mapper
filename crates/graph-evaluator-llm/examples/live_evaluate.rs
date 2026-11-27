//! Evaluates a small fixture graph with the live Claude API and prints the
//! result. Needs `ANTHROPIC_API_KEY`. It makes about seven requests.
//!
//! cargo run -p graph-evaluator-llm --example live_evaluate

use graph_evaluator::GraphEvaluator;
use graph_evaluator_llm::{
    AnthropicClient, AnthropicConfig, ClusterPlan, EvaluatorSettings, LlmEvaluator, NoSource,
    RetryingClient, StderrLog, ThreadPause, UreqTransport,
};
use std::process::ExitCode;
use std::time::Duration;

fn main() -> ExitCode {
    let Some(api_key) = std::env::var("ANTHROPIC_API_KEY")
        .ok()
        .filter(|key| !key.is_empty())
    else {
        eprintln!("Set ANTHROPIC_API_KEY to run this example.");
        return ExitCode::FAILURE;
    };
    let model = RetryingClient::new(
        AnthropicClient::new(
            AnthropicConfig::recommended(api_key),
            UreqTransport::new(Duration::from_secs(600)),
        ),
        ThreadPause,
        vec![
            Duration::from_secs(2),
            Duration::from_secs(8),
            Duration::from_secs(30),
        ],
    );
    let evaluator = LlmEvaluator::new(
        model,
        NoSource,
        StderrLog,
        EvaluatorSettings::recommended(ClusterPlan::proposed()),
    );
    match evaluator.evaluate(&graph_evaluation_assembly::testing::graph()) {
        Ok(evaluation) => {
            println!("{evaluation:#?}");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}
