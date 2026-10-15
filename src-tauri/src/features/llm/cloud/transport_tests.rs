use super::*;
use wiremock::{
    matchers::{header, method, path},
    Mock, MockServer, ResponseTemplate,
};
async fn client(server: &MockServer) -> CloudLlm {
    let settings = LLMSettingsDto {
        provider: LLMProvider::Openai,
        model: "test-model".into(),
        ..Default::default()
    };
    let mut client = CloudLlm::new(&settings, "test-secret".into()).unwrap();
    client.endpoint = format!("{}/responses", server.uri());
    client
}
#[tokio::test]
async fn interactive_completion_can_disable_the_wall_clock_budget() {
    let server = MockServer::start().await;
    Mock::given(method("POST")).and(path("/responses"))
        .respond_with(ResponseTemplate::new(200).set_delay(Duration::from_millis(150)).set_body_json(json!({
            "status":"completed", "output":[{"type":"message","content":[{"type":"output_text","text":"Completed"}]}]
        }))).mount(&server).await;
    let client = client(&server).await;
    let request = CompletionRequest {
        time_budget: Some(Duration::from_millis(10)),
        ..Default::default()
    };
    assert!(client.complete(&request).await.is_err());
    let response = client
        .complete(&CompletionRequest {
            no_time_limit: true,
            ..request
        })
        .await
        .unwrap();
    assert_eq!(response.text, "Completed");
}

#[tokio::test]
async fn streams_text_and_requires_terminal_event() {
    let server = MockServer::start().await;
    let completed = json!({"type":"response.completed","response":{"status":"completed","output":[
        {"type":"message","content":[{"type":"output_text","text":"héllo"}]}
    ]}});
    Mock::given(method("POST")).and(path("/responses")).and(header("authorization", "Bearer test-secret"))
        .respond_with(ResponseTemplate::new(200).set_body_string(format!("data: {{\"type\":\"response.output_text.delta\",\"delta\":\"héllo\"}}\n\ndata: {completed}\n\n")))
        .mount(&server).await;
    let client = client(&server).await;
    let shown = std::sync::Mutex::new(Vec::new());
    let on_text = |text: String| {
        shown.lock().unwrap().push(text);
        Ok(())
    };
    let response = client
        .complete_with_progress(&CompletionRequest::default(), &on_text)
        .await
        .unwrap();
    assert_eq!(response.text, "héllo");
    assert_eq!(*shown.lock().unwrap(), vec!["héllo".to_string()]);
    server.reset().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_string(
            "data: {\"type\":\"response.output_text.delta\",\"delta\":\"partial\"}\n\n",
        ))
        .mount(&server)
        .await;
    assert!(client
        .complete_with_progress(&CompletionRequest::default(), &|_| Ok(()))
        .await
        .is_err());
}
#[tokio::test]
async fn typed_reasoning_completion_requests_a_stream_and_keeps_its_native_result() {
    let server = MockServer::start().await;
    let final_response = json!({"status":"completed","output":[
        {"type":"reasoning","summary":[{"type":"summary_text","text":"Checked evidence."}],"encrypted_content":"opaque"},
        {"type":"message","content":[{"type":"output_text","text":"Answer"}]}
    ],"usage":{"input_tokens":5,"output_tokens":8}});
    let body = [
        json!({"type":"response.reasoning_summary_text.delta","item_id":"r1","summary_index":0,"delta":"Checked evidence."}),
        json!({"type":"response.output_text.delta","delta":"Answer"}),
        json!({"type":"response.completed","response":final_response}),
    ].iter().map(|event| format!("data: {event}\n\n")).collect::<String>();
    Mock::given(method("POST"))
        .and(path("/responses"))
        .respond_with(ResponseTemplate::new(200).set_body_string(body))
        .mount(&server)
        .await;
    let client = client(&server).await;
    let reasoning = std::sync::Mutex::new(String::new());
    let response = client
        .complete_with_reasoning_progress(
            &CompletionRequest {
                include_reasoning: true,
                ..Default::default()
            },
            &|_| Ok(()),
            &|delta| {
                reasoning.lock().unwrap().push_str(&delta);
                Ok(())
            },
            &|_| Ok(()),
        )
        .await
        .unwrap();
    assert_eq!(*reasoning.lock().unwrap(), "Checked evidence.");
    assert_eq!(replayed(&response), final_response["output"]);
    assert_eq!(response.text, "Answer");
    let requests = server.received_requests().await.unwrap();
    let body: Value = serde_json::from_slice(&requests[0].body).unwrap();
    assert_eq!(body["stream"], true);
}
#[tokio::test]
async fn typed_tool_calls_keep_native_call_ids_and_replay_them() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "status":"completed", "output":[{"type":"function_call", "call_id":"call_7", "name":"search", "arguments":"{}"}], "usage":{"input_tokens":4,"output_tokens":2}
        }))).mount(&server).await;
    let client = client(&server).await;
    let request = CompletionRequest {
        input: vec![
            CompletionInput::Message {
                role: "system".into(),
                content: "Search first.".into(),
            },
            CompletionInput::Message {
                role: "user".into(),
                content: "find".into(),
            },
        ],
        tools: vec![crate::application::ports::ToolDefinition {
            name: "search".into(),
            description: "Search documents".into(),
            parameters: json!({"type":"object","properties":{}}),
        }],
        ..Default::default()
    };
    let response = client.complete(&request).await.unwrap();
    assert!(matches!(
        response.tool_calls.first(),
        Some(CompletionInput::ToolCall { id, .. }) if id == "call_7"
    ));
    assert_eq!(replayed(&response)[0]["call_id"], "call_7");
    let requests = server.received_requests().await.unwrap();
    let body: Value = serde_json::from_slice(&requests.first().unwrap().body).unwrap();
    assert_eq!(
        body.pointer("/input/0/role").and_then(Value::as_str),
        Some("system")
    );
    assert_eq!(
        body.pointer("/tools/0/name").and_then(Value::as_str),
        Some("search")
    );
}

#[tokio::test]
async fn authentication_failure_is_not_retried_or_echoed() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(401).set_body_string("sensitive-provider-body"))
        .expect(1)
        .mount(&server)
        .await;
    let client = client(&server).await;
    let error = client
        .complete(&CompletionRequest::default())
        .await
        .unwrap_err();
    assert!(error.to_string().contains("401"));
    assert!(!error.to_string().contains("sensitive-provider-body"));
}
#[tokio::test]
async fn dropping_a_completion_cancels_the_request_without_a_retry() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_delay(Duration::from_secs(5))
                .set_body_string("data: {\"type\":\"response.completed\"}\n\n"),
        )
        .expect(1)
        .mount(&server)
        .await;
    let client = client(&server).await;
    let abandoned = tokio::time::timeout(
        Duration::from_millis(100),
        client.complete_with_progress(&CompletionRequest::default(), &|_| Ok(())),
    )
    .await;
    assert!(abandoned.is_err());
    // No detached generation or retry task survives a dropped completion.
    server.verify().await;
}

/// What the response replays, as the provider's own JSON.
fn replayed(response: &CompletionResponse) -> Value {
    Value::Array(
        response
            .replay
            .iter()
            .map(|item| match item {
                CompletionInput::Native { value } => value.clone(),
                other => panic!("cloud replay is native, not {other:?}"),
            })
            .collect(),
    )
}
