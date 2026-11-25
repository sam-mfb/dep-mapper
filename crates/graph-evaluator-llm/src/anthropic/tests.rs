use super::*;
use crate::http::TransportError;
use std::cell::RefCell;

/// Returns a fixed response and keeps the request it was sent.
struct FakeTransport {
    response: Result<HttpResponse, TransportError>,
    sent: RefCell<Vec<HttpRequest>>,
}

impl FakeTransport {
    fn new(response: Result<HttpResponse, TransportError>) -> Self {
        FakeTransport {
            response,
            sent: RefCell::new(vec![]),
        }
    }
}

impl HttpTransport for &FakeTransport {
    fn post(&self, request: &HttpRequest) -> Result<HttpResponse, TransportError> {
        self.sent.borrow_mut().push(request.clone());
        self.response.clone()
    }
}

fn ok(body: Value) -> Result<HttpResponse, TransportError> {
    Ok(HttpResponse {
        status: 200,
        headers: vec![],
        body: body.to_string(),
    })
}

fn status(code: u16, headers: Vec<(&str, &str)>) -> Result<HttpResponse, TransportError> {
    Ok(HttpResponse {
        status: code,
        headers: headers
            .into_iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect(),
        body: json!({"type": "error", "error": {"type": "some_error", "message": "details"}})
            .to_string(),
    })
}

fn request() -> ModelRequest {
    ModelRequest {
        system: "be careful".into(),
        turns: vec![Turn::User("judge this".into())],
        answer_schema: json!({"type": "object"}),
    }
}

fn config() -> AnthropicConfig {
    AnthropicConfig::recommended("key-123".into())
}

#[test]
fn sends_the_request_with_credentials_version_schema_and_fallback() {
    let transport = FakeTransport::new(ok(json!({
        "stop_reason": "end_turn",
        "content": [{"type": "text", "text": "{}"}],
    })));
    AnthropicClient::new(config(), &transport)
        .reply(&request())
        .unwrap();
    let sent = transport.sent.borrow();
    let sent = &sent[0];
    assert_eq!(sent.url, "https://api.anthropic.com/v1/messages");
    let header = |name: &str| {
        sent.headers
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.as_str())
    };
    assert_eq!(header("x-api-key"), Some("key-123"));
    assert_eq!(header("anthropic-version"), Some(API_VERSION));
    assert_eq!(header("anthropic-beta"), Some(FALLBACK_BETA));
    let body: Value = serde_json::from_str(&sent.body).unwrap();
    assert_eq!(body["model"], "claude-opus-5-5");
    assert_eq!(body["fallbacks"], "default");
    assert_eq!(body["thinking"]["type"], "adaptive");
    assert_eq!(body["output_config"]["effort"], "high");
    assert_eq!(
        body["output_config"]["format"]["schema"],
        json!({"type": "object"})
    );
    assert_eq!(body["system"][0]["text"], "be careful");
    assert_eq!(body["system"][0]["cache_control"]["type"], "ephemeral");
    assert_eq!(
        body["messages"],
        json!([{"role": "user", "content": "judge this"}])
    );
}

#[test]
fn an_answer_keeps_the_whole_content_and_replays_it_unchanged() {
    let content = json!([
        {"type": "thinking", "thinking": "", "signature": "sig"},
        {"type": "text", "text": "{\"a\":"},
        {"type": "text", "text": "1}"},
    ]);
    let reply =
        parse_reply(&json!({"stop_reason": "end_turn", "content": content}).to_string()).unwrap();
    let ModelReply::Answer(turn) = reply else {
        panic!("expected an answer");
    };
    assert_eq!(turn.text, "{\"a\":1}");

    let mut follow_up = request();
    follow_up.turns.push(Turn::Assistant(turn));
    follow_up.turns.push(Turn::User("fix it".into()));
    let body = request_body(&config(), &follow_up);
    assert_eq!(body["messages"][1]["role"], "assistant");
    assert_eq!(body["messages"][1]["content"], content);
}

#[test]
fn an_assistant_turn_without_a_record_is_sent_as_text() {
    let mut follow_up = request();
    follow_up.turns.push(Turn::Assistant(AssistantTurn {
        text: "{}".into(),
        provider_record: None,
    }));
    let body = request_body(&config(), &follow_up);
    assert_eq!(
        body["messages"][1]["content"],
        json!([{"type": "text", "text": "{}"}])
    );
}

#[test]
fn a_refusal_and_a_truncation_are_replies_not_errors() {
    let refused = parse_reply(
        &json!({
            "stop_reason": "refusal",
            "content": [],
            "stop_details": {"type": "refusal", "category": "cyber", "explanation": "no"},
        })
        .to_string(),
    );
    assert_eq!(
        refused,
        Ok(ModelReply::Refused {
            explanation: "cyber: no".into()
        })
    );
    let truncated = parse_reply(&json!({"stop_reason": "max_tokens", "content": []}).to_string());
    assert_eq!(truncated, Ok(ModelReply::Truncated));
}

#[test]
fn an_unexpected_stop_reason_is_a_permanent_error() {
    let reply = parse_reply(&json!({"stop_reason": "tool_use", "content": []}).to_string());
    assert!(matches!(reply, Err(ModelError::Permanent { .. })));
}

#[test]
fn rate_limits_and_server_errors_are_transient_with_retry_after() {
    for code in [429, 500, 529] {
        let transport = FakeTransport::new(status(code, vec![("retry-after", "7")]));
        let error = AnthropicClient::new(config(), &transport)
            .reply(&request())
            .unwrap_err();
        let ModelError::Transient {
            message,
            retry_after,
        } = error
        else {
            panic!("{code} should be transient");
        };
        assert_eq!(retry_after, Some(Duration::from_secs(7)));
        assert!(message.contains("some_error: details"), "{message}");
    }
}

#[test]
fn client_errors_are_permanent_unless_the_api_says_to_retry() {
    let transport = FakeTransport::new(status(400, vec![]));
    let error = AnthropicClient::new(config(), &transport)
        .reply(&request())
        .unwrap_err();
    assert!(matches!(error, ModelError::Permanent { .. }));

    let transport = FakeTransport::new(status(400, vec![("x-should-retry", "true")]));
    let error = AnthropicClient::new(config(), &transport)
        .reply(&request())
        .unwrap_err();
    assert!(matches!(error, ModelError::Transient { .. }));

    let transport = FakeTransport::new(status(503, vec![("x-should-retry", "false")]));
    let error = AnthropicClient::new(config(), &transport)
        .reply(&request())
        .unwrap_err();
    assert!(matches!(error, ModelError::Permanent { .. }));
}

#[test]
fn a_lost_connection_is_transient() {
    let transport = FakeTransport::new(Err(TransportError {
        message: "reset".into(),
    }));
    let error = AnthropicClient::new(config(), &transport)
        .reply(&request())
        .unwrap_err();
    assert!(matches!(error, ModelError::Transient { .. }));
}

#[test]
fn debug_output_hides_the_api_key() {
    let shown = format!("{:?}", config());
    assert!(!shown.contains("key-123"), "{shown}");
    assert!(shown.contains("claude-opus-5-5"), "{shown}");
}
