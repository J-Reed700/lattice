//! Tests for typed completions, streaming, tool calls and timeouts against scripted Ollama servers.

use super::*;
use std::sync::Arc;

#[tokio::test]
async fn typed_completion_sends_schema_reasoning_sampling_and_output_budget() {
    use crate::application::ports::llm_port::{
        CompletionInput, CompletionRequest, LLMPort, SamplingOverride,
    };
    use wiremock::{
        matchers::{method, path},
        Mock, MockServer, ResponseTemplate,
    };

    let server = MockServer::start().await;
    let schema = serde_json::json!({
        "type": "object",
        "properties": {"ok": {"type": "boolean"}},
        "required": ["ok"],
        "additionalProperties": false
    });
    let expected = serde_json::json!({
        "model":"utility",
        "messages":[
            {"role":"system","content":"Return JSON."},
            {"role":"user","content":"Check this batch."}
        ],
        "stream":true,
        "options":{"temperature":0.0,"top_p":1.0,"top_k":1,"num_predict":2048},
        "format":schema
    });
    Mock::given(method("POST"))
        .and(path("/api/chat"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_delay(Duration::from_millis(100))
                .set_body_json(serde_json::json!({
                    "model": "utility",
                    "created_at": "2026-09-28T00:00:00Z",
                    "message": {"role": "assistant", "content": "{\"ok\":true}"},
                    "done": true,
                    "done_reason": "stop",
                    "prompt_eval_count": 30,
                    "eval_count": 4
                })),
        )
        .mount(&server)
        .await;

    let client = OllamaClient::with_model_and_timeouts(
        server.uri(),
        "utility",
        Duration::from_millis(50),
        Duration::from_secs(1),
    )
    .unwrap();
    let request = CompletionRequest {
        input: vec![
            CompletionInput::Message {
                role: "system".into(),
                content: "Return JSON.".into(),
            },
            CompletionInput::Message {
                role: "user".into(),
                content: "Check this batch.".into(),
            },
        ],
        json_schema: Some(schema),
        reasoning_effort: Some("none".into()),
        sampling: Some(SamplingOverride::deterministic()),
        max_output_tokens: Some(2048),
        time_budget: Some(Duration::from_secs(2)),
        ..Default::default()
    };
    let response = LLMPort::complete(&client, &request).await.unwrap();
    assert_eq!(response.text, "{\"ok\":true}");
    assert_eq!(response.input_tokens, 30);
    assert_eq!(response.output_tokens, 4);
    assert_eq!(response.finish_reason, "stop");
    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests[0].method.as_str(), "POST");
    assert_eq!(requests[0].url.path(), "/api/chat");
    let mut sent: serde_json::Value = serde_json::from_slice(&requests[0].body).unwrap();
    let repeat_penalty = sent["options"]["repeat_penalty"]
        .as_f64()
        .expect("repeat penalty is numeric");
    assert!((repeat_penalty - 1.1).abs() < 1e-6);
    sent["options"]
        .as_object_mut()
        .unwrap()
        .remove("repeat_penalty");
    assert_eq!(sent, expected);
}

#[tokio::test]
async fn typed_completion_replays_native_tool_call_and_result_on_next_round() {
    use crate::application::ports::llm_port::{
        CompletionInput, CompletionRequest, LLMPort, ToolDefinition,
    };
    use wiremock::{
        matchers::{method, path},
        Mock, MockServer, ResponseTemplate,
    };

    let server = MockServer::start().await;
    let tool = ToolDefinition {
        name: "search_saved_knowledge".into(),
        description: "Search saved notes".into(),
        parameters: serde_json::json!({"type":"object","properties":{}}),
    };
    let first = serde_json::json!({
        "model":"utility",
        "created_at":"2026-09-28T00:00:00Z",
        "message":{"role":"assistant","content":"","tool_calls":[{
            "function":{"name":"search_saved_knowledge","arguments":{"query":"trip"}}
        }]},
        "done":true,
        "done_reason":"stop"
    });
    let second = serde_json::json!({
        "model":"utility",
        "created_at":"2026-09-28T00:00:01Z",
        "message":{"role":"assistant","content":"Friday."},
        "done":true,
        "done_reason":"stop"
    });
    let response_index = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let response_index_for_mock = response_index.clone();
    Mock::given(method("POST"))
        .and(path("/api/chat"))
        .respond_with(move |_request: &wiremock::Request| {
            let response =
                if response_index_for_mock.fetch_add(1, std::sync::atomic::Ordering::SeqCst) == 0 {
                    first.clone()
                } else {
                    second.clone()
                };
            ResponseTemplate::new(200).set_body_json(response)
        })
        .mount(&server)
        .await;

    let client = OllamaClient::with_model_and_timeouts(
        server.uri(),
        "utility",
        Duration::from_secs(1),
        Duration::from_secs(1),
    )
    .unwrap();
    let request = CompletionRequest {
        input: vec![CompletionInput::Message {
            role: "user".into(),
            content: "When is the trip?".into(),
        }],
        tools: vec![tool],
        max_output_tokens: Some(1024),
        ..Default::default()
    };
    let first_response = LLMPort::complete(&client, &request).await.unwrap();
    let call = first_response.tool_calls.first().unwrap();
    let (id, name) = match call {
        CompletionInput::ToolCall { id, name, .. } => (id.clone(), name.clone()),
        other => panic!("unexpected typed tool call: {other:?}"),
    };
    assert_eq!(name, "search_saved_knowledge");

    let mut next_request = request;
    next_request.input.extend(first_response.replay.clone());
    next_request.input.push(CompletionInput::ToolResult {
        id,
        output: "The trip is on Friday.".into(),
    });
    let second_response = LLMPort::complete(&client, &next_request).await.unwrap();
    assert_eq!(second_response.text, "Friday.");
    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 2);
    let first_request: serde_json::Value = serde_json::from_slice(&requests[0].body).unwrap();
    assert_eq!(
        first_request["tools"][0]["function"]["name"],
        "search_saved_knowledge"
    );
    let second_request: serde_json::Value = serde_json::from_slice(&requests[1].body).unwrap();
    let messages = second_request["messages"].as_array().unwrap();
    assert_eq!(messages.len(), 3);
    assert_eq!(messages[1]["role"], "assistant");
    assert_eq!(
        messages[1]["tool_calls"][0]["function"]["name"],
        "search_saved_knowledge"
    );
    assert_eq!(messages[2]["role"], "tool");
    assert_eq!(messages[2]["tool_name"], "search_saved_knowledge");
    assert_eq!(messages[2]["content"], "The trip is on Friday.");
}

#[tokio::test]
async fn typed_completion_cancels_http_work_when_request_budget_expires() {
    use crate::application::ports::llm_port::{CompletionInput, CompletionRequest, LLMPort};
    use wiremock::{
        matchers::{method, path},
        Mock, MockServer, ResponseTemplate,
    };

    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/chat"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_delay(Duration::from_millis(200))
                .set_body_json(serde_json::json!({
                    "model":"utility",
                    "created_at":"2026-09-28T00:00:00Z",
                    "message":{"role":"assistant","content":"late"},
                    "done":true
                })),
        )
        .mount(&server)
        .await;
    let client = OllamaClient::with_model_and_timeouts(
        server.uri(),
        "utility",
        Duration::from_secs(1),
        Duration::from_secs(1),
    )
    .unwrap();
    let request = CompletionRequest {
        input: vec![CompletionInput::Message {
            role: "user".into(),
            content: "Please answer quickly.".into(),
        }],
        time_budget: Some(Duration::from_millis(20)),
        ..Default::default()
    };

    let error = LLMPort::complete(&client, &request).await.unwrap_err();
    assert!(error.to_string().contains("time budget"));
    let request = CompletionRequest {
        no_time_limit: true,
        ..request
    };
    assert_eq!(
        LLMPort::complete(&client, &request).await.unwrap().text,
        "late"
    );
    assert_eq!(
        LLMPort::complete_with_progress(&client, &request, &|_| Ok(()))
            .await
            .unwrap()
            .text,
        "late"
    );
}

#[tokio::test]
async fn typed_completion_streams_content_and_keeps_terminal_tool_calls() {
    use crate::application::ports::llm_port::{
        CompletionInput, CompletionRequest, LLMPort, ToolDefinition,
    };
    use wiremock::{
        matchers::{method, path},
        Mock, MockServer, ResponseTemplate,
    };

    let server = MockServer::start().await;
    let chunks = [
        serde_json::json!({
            "model":"utility","created_at":"2026-09-28T00:00:00Z",
            "message":{"role":"assistant","content":"Hi ","thinking":"Compare "},"done":false
        }),
        serde_json::json!({
            "model":"utility","created_at":"2026-09-28T00:00:00Z",
            "message":{"role":"assistant","content":"世界","thinking":"sources","tool_calls":[{
                "function":{"name":"search_saved_knowledge","arguments":{"query":"trip"},"index":0}
            }]},"done":true,"done_reason":"stop","prompt_eval_count":20,"eval_count":5
        }),
    ];
    let body = chunks
        .iter()
        .map(serde_json::Value::to_string)
        .collect::<Vec<_>>()
        .join("\n");
    Mock::given(method("POST"))
        .and(path("/api/chat"))
        .respond_with(ResponseTemplate::new(200).set_body_string(format!("{body}\n")))
        .mount(&server)
        .await;
    let client = OllamaClient::with_model_and_timeouts(
        server.uri(),
        "utility",
        Duration::from_secs(1),
        Duration::from_secs(1),
    )
    .unwrap();
    let request = CompletionRequest {
        input: vec![CompletionInput::Message {
            role: "user".into(),
            content: "When is the trip?".into(),
        }],
        include_reasoning: true,
        tools: vec![ToolDefinition {
            name: "search_saved_knowledge".into(),
            description: "Search saved notes".into(),
            parameters: serde_json::json!({"type":"object","properties":{}}),
        }],
        ..Default::default()
    };
    let received = Arc::new(std::sync::Mutex::new(Vec::new()));
    let received_by_callback = received.clone();
    let on_text = move |text: String| {
        received_by_callback.lock().unwrap().push(text);
        Ok(())
    };
    let received_reasoning = Arc::new(std::sync::Mutex::new(Vec::new()));
    let reasoning_by_callback = received_reasoning.clone();
    let on_reasoning = move |text: String| {
        reasoning_by_callback.lock().unwrap().push(text);
        Ok(())
    };
    let response = LLMPort::complete_with_reasoning_progress(
        &client,
        &request,
        &on_text,
        &on_reasoning,
        &|_| Ok(()),
    )
    .await
    .unwrap();

    assert_eq!(response.text, "Hi 世界");
    assert_eq!(response.reasoning.as_deref(), Some("Compare sources"));
    assert_eq!(*received.lock().unwrap(), vec!["Hi ", "世界"]);
    assert_eq!(
        *received_reasoning.lock().unwrap(),
        vec!["Compare ", "sources"]
    );
    assert_eq!(response.tool_calls.len(), 1);
    assert!(matches!(
        response.tool_calls.first(),
        Some(CompletionInput::ToolCall { name, .. }) if name == "search_saved_knowledge"
    ));
    assert_eq!(response.output_tokens, 5);
    assert_eq!(response.finish_reason, "stop");
    let sent: serde_json::Value =
        serde_json::from_slice(&server.received_requests().await.unwrap()[0].body).unwrap();
    assert_eq!(sent["stream"], true);
    assert!(sent.get("think").is_none());
    assert_eq!(
        sent["tools"][0]["function"]["name"],
        "search_saved_knowledge"
    );
}

#[tokio::test]
async fn typed_stream_handles_sparse_tool_indices_without_allocating_gaps() {
    use crate::application::ports::llm_port::{CompletionInput, CompletionRequest, LLMPort};
    use wiremock::{
        matchers::{method, path},
        Mock, MockServer, ResponseTemplate,
    };

    // Exercise both newline-delimited chunks and the final unterminated line.
    for suffix in ["\n", ""] {
        let server = MockServer::start().await;
        let body = serde_json::json!({
            "model": "utility", "created_at": "2026-09-30T00:00:00Z",
            "message": {"role": "assistant", "content": "", "tool_calls": [
                {"function": {"name": "later", "arguments": {}, "index": usize::MAX}},
                {"function": {"name": "replaced", "arguments": {}, "index": 0}},
                {"function": {"name": "first", "arguments": {}, "index": 0}}
            ]}, "done": true
        });
        Mock::given(method("POST"))
            .and(path("/api/chat"))
            .respond_with(ResponseTemplate::new(200).set_body_string(format!("  {body}  {suffix}")))
            .mount(&server)
            .await;
        let client = OllamaClient::with_model_and_timeouts(
            server.uri(),
            "utility",
            Duration::from_secs(1),
            Duration::from_secs(1),
        )
        .unwrap();
        let response =
            LLMPort::complete_with_progress(&client, &CompletionRequest::default(), &|_| Ok(()))
                .await
                .unwrap();
        let names: Vec<_> = response
            .tool_calls
            .iter()
            .filter_map(|call| match call {
                CompletionInput::ToolCall { name, .. } => Some(name.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(names, ["first", "later"]);
    }
}

#[test]
fn the_base_url_loses_its_trailing_slash() {
    let client = OllamaClient::with_model("http://localhost:11434/", "mistral").unwrap();
    assert_eq!(client.base_url, "http://localhost:11434");
    assert_eq!(client.model_name(), "mistral");
    assert!(OllamaClient::with_model("http://localhost:11434", " ").is_err());
}

#[tokio::test]
async fn health_check_requires_an_ollama_route_not_just_a_listening_port() {
    use wiremock::{
        matchers::{method, path},
        Mock, MockServer, ResponseTemplate,
    };

    // A llama.cpp server answers "/" with its web UI but has no /api/tags.
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/"))
        .respond_with(ResponseTemplate::new(200).set_body_string("<html>llama.cpp</html>"))
        .mount(&server)
        .await;
    let client = OllamaClient::with_model(server.uri(), "any-model").unwrap();
    assert!(!client.health_check().await);

    Mock::given(method("GET"))
        .and(path("/api/tags"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"models":[]})))
        .mount(&server)
        .await;
    assert!(client.health_check().await);
}

#[tokio::test]
async fn a_failed_status_says_what_it_means_without_echoing_the_body() {
    use crate::application::ports::llm_port::CompletionRequest;
    use wiremock::{
        matchers::{method, path},
        Mock, MockServer, ResponseTemplate,
    };

    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/chat"))
        .respond_with(
            ResponseTemplate::new(404).set_body_json(
                serde_json::json!({"error":"model 'private prompt text' not found"}),
            ),
        )
        .mount(&server)
        .await;
    let client = OllamaClient::with_model(server.uri(), "absent").unwrap();
    let error = client
        .complete(&CompletionRequest::default())
        .await
        .unwrap_err();
    assert!(matches!(error, AppError::InvalidConfig(_)), "{error:?}");
    assert!(error.to_string().contains("pull it first"), "{error}");
    assert!(!error.to_string().contains("private prompt text"));
}

/// Scripted chunks written to a raw socket, each after its delay.
async fn scripted_server(
    chunks: Vec<(Duration, Vec<u8>)>,
) -> (String, tokio::task::JoinHandle<()>) {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        // Drain the request headers; the body is irrelevant to these tests.
        let mut buffer = [0u8; 8192];
        let read = socket.read(&mut buffer).await.unwrap();
        assert!(read > 0, "client sent no request");
        let size: usize = chunks.iter().map(|(_, chunk)| chunk.len()).sum();
        socket.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: application/x-ndjson\r\nContent-Length: {size}\r\n\r\n").as_bytes()).await.unwrap();
        for (delay, chunk) in chunks {
            tokio::time::sleep(delay).await;
            if socket.write_all(&chunk).await.is_err() {
                return;
            }
        }
    });
    (format!("http://{address}"), server)
}

/// A client-wide `timeout` covers the response body, so it would truncate a
/// generation that is still producing bytes. A character split between two
/// network chunks must also come out whole.
#[tokio::test]
async fn a_streamed_body_outlives_the_probe_timeout_and_keeps_split_characters() {
    let first = "{\"model\":\"m\",\"created_at\":\"\",\"message\":{\"role\":\"assistant\",\"content\":\"Slow 世\"},\"done\":false}\n{\"model\":\"m\",\"created_at\":\"\",\"message\":{\"role\":\"assistant\",\"content\":\"界\"},\"done\":true}\n";
    let split = first.find('界').unwrap() + 1;
    let bytes = first.as_bytes();
    let delay = Duration::from_millis(250);
    let (url, server) = scripted_server(vec![
        (delay, bytes[..split].to_vec()),
        (delay, bytes[split..].to_vec()),
    ])
    .await;

    let client = OllamaClient::with_model_and_timeouts(
        url,
        "any-model",
        Duration::from_millis(150),
        Duration::from_secs(5),
    )
    .unwrap();
    let response = client
        .complete(&crate::application::ports::llm_port::CompletionRequest::default())
        .await
        .unwrap();
    assert_eq!(response.text, "Slow 世界");
    server.await.unwrap();
}

#[tokio::test]
async fn a_stream_that_goes_silent_is_a_stall_not_a_wait() {
    let chunk = b"{\"model\":\"m\",\"created_at\":\"\",\"message\":{\"role\":\"assistant\",\"content\":\"Half\"},\"done\":false}\n".to_vec();
    let (url, server) = scripted_server(vec![
        (Duration::ZERO, chunk),
        (Duration::from_secs(5), b"never".to_vec()),
    ])
    .await;
    let client = OllamaClient::with_model_and_timeouts(
        url,
        "any-model",
        Duration::from_secs(5),
        Duration::from_millis(200),
    )
    .unwrap();
    let error = client
        .complete(&crate::application::ports::llm_port::CompletionRequest::default())
        .await
        .unwrap_err();
    assert!(error.to_string().contains("sent nothing"), "{error}");
    server.abort();
}
