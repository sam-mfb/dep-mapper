use super::*;
use crate::model::{AssistantTurn, Turn};
use std::cell::RefCell;

struct Scripted(RefCell<Vec<Result<ModelReply, ModelError>>>);

impl ModelClient for Scripted {
    fn reply(&self, _: &ModelRequest) -> Result<ModelReply, ModelError> {
        self.0.borrow_mut().remove(0)
    }
}

#[derive(Default)]
struct RecordedPauses(RefCell<Vec<Duration>>);

impl Pause for &RecordedPauses {
    fn pause(&self, duration: Duration) {
        self.0.borrow_mut().push(duration);
    }
}

fn request() -> ModelRequest {
    ModelRequest {
        system: String::new(),
        turns: vec![Turn::User("q".into())],
        answer_schema: serde_json::json!({}),
    }
}

fn answer() -> Result<ModelReply, ModelError> {
    Ok(ModelReply::Answer(AssistantTurn {
        text: "{}".into(),
        provider_record: None,
    }))
}

fn transient(retry_after: Option<u64>) -> Result<ModelReply, ModelError> {
    Err(ModelError::Transient {
        message: "busy".into(),
        retry_after: retry_after.map(Duration::from_secs),
    })
}

fn secs(n: u64) -> Duration {
    Duration::from_secs(n)
}

#[test]
fn retries_transient_failures_with_the_configured_delays() {
    let pauses = RecordedPauses::default();
    let client = RetryingClient::new(
        Scripted(RefCell::new(vec![
            transient(None),
            transient(Some(9)),
            answer(),
        ])),
        &pauses,
        vec![secs(2), secs(4), secs(8)],
    );
    assert_eq!(client.reply(&request()), answer());
    assert_eq!(*pauses.0.borrow(), vec![secs(2), secs(9)]);
}

#[test]
fn gives_up_after_the_last_delay() {
    let pauses = RecordedPauses::default();
    let client = RetryingClient::new(
        Scripted(RefCell::new(vec![transient(None), transient(None)])),
        &pauses,
        vec![secs(1)],
    );
    assert!(matches!(
        client.reply(&request()),
        Err(ModelError::Transient { message, .. }) if message.contains("after 1 retries")
    ));
}

#[test]
fn does_not_retry_permanent_failures() {
    let pauses = RecordedPauses::default();
    let permanent = Err(ModelError::Permanent {
        message: "bad".into(),
    });
    let client = RetryingClient::new(
        Scripted(RefCell::new(vec![permanent.clone(), answer()])),
        &pauses,
        vec![secs(1)],
    );
    assert_eq!(client.reply(&request()), permanent);
    assert!(pauses.0.borrow().is_empty());
}
