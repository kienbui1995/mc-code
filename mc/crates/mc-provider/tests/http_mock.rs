use mc_provider::{
    CompletionRequest, ContentBlock, GenericProvider, InputMessage, MessageRole, ProviderEvent,
};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn make_request(model: &str) -> CompletionRequest {
    CompletionRequest {
        model: model.into(),
        max_tokens: 100,
        system_prompt: Some("test".into()),
        messages: vec![InputMessage {
            role: MessageRole::User,
            content: vec![ContentBlock::Text {
                text: "hello".into(),
            }],
        }],
        tools: vec![],
        tool_choice: None,
        thinking_budget: None,
        response_format: None,
    }
}

async fn collect_events(stream: mc_provider::ProviderStream) -> Vec<ProviderEvent> {
    use futures_core::Stream;
    use std::pin::Pin;
    let mut stream = stream;
    let mut events = Vec::new();
    loop {
        let next = std::future::poll_fn(|cx| Pin::as_mut(&mut stream).poll_next(cx)).await;
        match next {
            Some(Ok(e)) => events.push(e),
            Some(Err(_)) => break,
            None => break,
        }
    }
    events
}

fn sse(body: &str) -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_raw(body.to_string(), "text/event-stream")
}

#[tokio::test]
async fn streams_text_deltas() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(sse(
            "data: {\"choices\":[{\"delta\":{\"content\":\"Hello\"}}]}\n\n\
             data: {\"choices\":[{\"delta\":{\"content\":\" world\"}}]}\n\n\
             data: {\"choices\":[{}],\"usage\":{\"prompt_tokens\":10,\"completion_tokens\":5}}\n\n\
             data: [DONE]\n\n",
        ))
        .mount(&server)
        .await;

    let provider = GenericProvider::new(server.uri(), Some("k".into()));
    let events = collect_events(provider.stream(&make_request("gpt-4"))).await;
    let texts: Vec<_> = events
        .iter()
        .filter_map(|e| match e {
            ProviderEvent::TextDelta(t) => Some(t.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(texts, vec!["Hello", " world"]);
}

#[tokio::test]
async fn streams_tool_call() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(sse(
            "data: {\"choices\":[{\"delta\":{\"role\":\"assistant\",\"tool_calls\":[{\"index\":0,\"id\":\"c1\",\"type\":\"function\",\"function\":{\"name\":\"bash\",\"arguments\":\"\"}}]}}]}\n\n\
             data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"arguments\":\"{\\\"cmd\\\":\\\"ls\\\"}\"}}]}}]}\n\n\
             data: {\"choices\":[{}],\"usage\":{\"prompt_tokens\":10,\"completion_tokens\":5}}\n\n\
             data: [DONE]\n\n",
        ))
        .mount(&server)
        .await;

    let provider = GenericProvider::new(server.uri(), Some("k".into()));
    let events = collect_events(provider.stream(&make_request("gpt-4"))).await;
    assert!(events
        .iter()
        .any(|e| matches!(e, ProviderEvent::ToolUse { name, .. } if name == "bash")));
}

#[tokio::test]
async fn reports_usage() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(sse(
            "data: {\"choices\":[{\"delta\":{\"content\":\"hi\"}}]}\n\n\
             data: {\"choices\":[{}],\"usage\":{\"prompt_tokens\":42,\"completion_tokens\":7}}\n\n\
             data: [DONE]\n\n",
        ))
        .mount(&server)
        .await;

    let provider = GenericProvider::new(server.uri(), Some("k".into()));
    let events = collect_events(provider.stream(&make_request("gpt-4"))).await;
    let usage = events.iter().find_map(|e| match e {
        ProviderEvent::Usage(u) => Some(u),
        _ => None,
    });
    assert!(usage.is_some());
    let u = usage.unwrap();
    assert_eq!(u.input_tokens, 42);
    assert_eq!(u.output_tokens, 7);
}

#[tokio::test]
async fn handles_empty_stream() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(sse("data: [DONE]\n\n"))
        .mount(&server)
        .await;

    let provider = GenericProvider::new(server.uri(), Some("k".into()));
    let events = collect_events(provider.stream(&make_request("gpt-4"))).await;
    // Should not panic, may have MessageStop
    assert!(
        events.is_empty()
            || events
                .iter()
                .any(|e| matches!(e, ProviderEvent::MessageStop))
    );
}

#[tokio::test]
async fn json_mode_sent_in_body() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .and(wiremock::matchers::body_partial_json(serde_json::json!({
            "response_format": {"type": "json_object"}
        })))
        .respond_with(sse(
            "data: {\"choices\":[{\"delta\":{\"content\":\"{\\\"ok\\\":true}\"}}]}\n\n\
             data: {\"choices\":[{}],\"usage\":{\"prompt_tokens\":1,\"completion_tokens\":1}}\n\n\
             data: [DONE]\n\n",
        ))
        .mount(&server)
        .await;

    let provider = GenericProvider::new(server.uri(), Some("k".into()));
    let mut req = make_request("gpt-4");
    req.response_format = Some(mc_provider::ResponseFormat::Json);
    let events = collect_events(provider.stream(&req)).await;
    assert!(events
        .iter()
        .any(|e| matches!(e, ProviderEvent::TextDelta(t) if t.contains("ok"))));
}
