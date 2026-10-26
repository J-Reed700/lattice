//! Tests for typed completions, streaming, tool calls and timeouts against scripted Ollama servers.

use super::*;
use std::sync::Arc;

#[tokio::test]
async fn typed_completion_sends_schema_reasoning_sampling_and_output_budget() {
    use lattice_core::application::ports::llm_port::{
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
        "options":{"temperature":0.0,"top_p":1.0,"top_k":1,"presence_penalty":0.0,"num_ctx":131072,"num_predict":2048},
        "format":schema,
        "think":false
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
    assert!((repeat_penalty - 1.0).abs() < 1e-6);
    sent["options"]
        .as_object_mut()
        .unwrap()
        .remove("repeat_penalty");
    assert_eq!(sent, expected);
}

#[tokio::test]
async fn typed_completion_replays_native_tool_call_and_result_on_next_round() {
    use lattice_core::application::ports::llm_port::{
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
    use lattice_core::application::ports::llm_port::{CompletionInput, CompletionRequest, LLMPort};
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
    use lattice_core::application::ports::llm_port::{
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
    // A tool round never thinks, and says so: Qwen 3.5 thinks by default.
    assert_eq!(sent["think"], false);
    assert_eq!(
        sent["tools"][0]["function"]["name"],
        "search_saved_knowledge"
    );
}

#[tokio::test]
async fn typed_stream_handles_sparse_tool_indices_without_allocating_gaps() {
    use lattice_core::application::ports::llm_port::{CompletionInput, CompletionRequest, LLMPort};
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
    use lattice_core::application::ports::llm_port::CompletionRequest;
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
        .complete(&lattice_core::application::ports::llm_port::CompletionRequest::default())
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
        .complete(&lattice_core::application::ports::llm_port::CompletionRequest::default())
        .await
        .unwrap_err();
    assert!(error.to_string().contains("sent nothing"), "{error}");
    server.abort();
}

#[test]
fn a_thinking_answer_turns_think_on_with_the_thinking_profile() {
    use lattice_core::application::ports::llm_port::ToolDefinition;
    let client = OllamaClient::with_model_and_timeouts(
        "http://localhost:11434",
        "qwen3.5",
        Duration::from_secs(1),
        Duration::from_secs(1),
    )
    .unwrap();
    let mut request = CompletionRequest {
        thinking: ThinkingMode::On,
        ..Default::default()
    };
    let sent = serde_json::to_value(client.chat_request(&request).unwrap()).unwrap();
    assert_eq!(sent["think"], true);
    assert_eq!(sent["options"]["temperature"], 1.0);
    assert_eq!(sent["options"]["top_k"], 20);
    request.reasoning_effort = Some("high".into());
    let sent = serde_json::to_value(client.chat_request(&request).unwrap()).unwrap();
    assert_eq!(sent["think"], "high");
    request.tools = vec![ToolDefinition {
        name: "search".into(),
        description: "Search".into(),
        parameters: serde_json::json!({"type":"object"}),
    }];
    let sent = serde_json::to_value(client.chat_request(&request).unwrap()).unwrap();
    assert_eq!(
        sent["think"], false,
        "a round that can call tools never thinks"
    );
}

#[test]
fn stop_sequences_go_in_the_options_only_when_asked_for() {
    let client = OllamaClient::with_model_and_timeouts(
        "http://localhost:11434",
        "qwen3.5",
        Duration::from_secs(1),
        Duration::from_secs(1),
    )
    .unwrap();
    let mut request = CompletionRequest {
        stop: vec!["[5]".into(), "\n5.".into()],
        ..Default::default()
    };
    let sent = serde_json::to_value(client.chat_request(&request).unwrap()).unwrap();
    assert_eq!(sent["options"]["stop"], serde_json::json!(["[5]", "\n5."]));
    request.stop.clear();
    let sent = serde_json::to_value(client.chat_request(&request).unwrap()).unwrap();
    assert!(sent["options"].get("stop").is_none());
}

fn picture_request() -> CompletionRequest {
    use lattice_core::application::ports::llm_port::ImagePart;
    CompletionRequest {
        input: vec![CompletionInput::Multimodal {
            role: "user".into(),
            parts: vec![
                ContentPart::Image(ImagePart {
                    mime: "image/png".into(),
                    data_base64: "iVBORw0KGgo=".into(),
                    width: 448,
                    height: 448,
                    tokens: 258,
                    alt: "chart.png".into(),
                }),
                ContentPart::Text {
                    text: "What does it show?".into(),
                },
            ],
        }],
        ..Default::default()
    }
}

fn llava() -> OllamaClient {
    OllamaClient::with_model_and_timeouts(
        "http://localhost:11434",
        "llava:7b",
        Duration::from_secs(1),
        Duration::from_secs(1),
    )
    .unwrap()
}

#[test]
fn pictures_go_in_the_messages_images_list_beside_its_text() {
    use lattice_core::domain::models::vision::VisionPreset;
    let client = llava().with_vision(Some(VisionPreset::GemmaSiglip.geometry()));
    let sent = serde_json::to_value(client.chat_request(&picture_request()).unwrap()).unwrap();
    assert_eq!(
        sent["messages"][0],
        serde_json::json!({
            "role": "user",
            "content": "What does it show?",
            "images": ["iVBORw0KGgo="]
        })
    );
}

#[test]
fn a_model_nobody_declared_able_to_see_refuses_pictures() {
    let refused = llava().chat_request(&picture_request()).unwrap_err();
    assert!(matches!(refused, AppError::InvalidConfig(_)), "{refused}");
    assert!(refused
        .to_string()
        .contains("llava:7b is not set up to see images"));
}

/// Ollama's final chunk times the prompt and the answer in nanoseconds;
/// counts without durations time nothing.
#[test]
fn the_final_chunk_times_the_prompt_and_the_answer() {
    let chunk = |extra: serde_json::Value| {
        let mut line = serde_json::json!({
            "model":"utility","created_at":"2026-09-28T00:00:00Z",
            "message":{"role":"assistant","content":"Hi"},"done":true,"done_reason":"stop",
            "prompt_eval_count":1700,"eval_count":226
        });
        line.as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        let mut answer = Answer::default();
        answer
            .push_line(line.to_string().as_bytes(), &|_| Ok(()), &|_| Ok(()))
            .unwrap();
        answer.into_response().unwrap().timing
    };
    let timing = chunk(serde_json::json!({
        "prompt_eval_duration": 2_000_000_000u64, "eval_duration": 20_000_000_000u64
    }))
    .unwrap();
    assert_eq!((timing.prompt_tokens, timing.output_tokens), (1700, 226));
    assert!((timing.prompt_seconds - 2.0).abs() < 1e-9);
    assert!((timing.output_seconds - 20.0).abs() < 1e-9);
    assert_eq!(chunk(serde_json::json!({})), None);
}

/// Ollama runs at its server default (often 4,096 tokens) unless a request
/// names a window, and trims a longer prompt without saying so. The window
/// asked for, and reported to the prompt builders, is the configured one held
/// to the model's trained length and to a Modelfile's pinned `num_ctx`.
#[tokio::test]
async fn the_window_is_asked_for_and_held_to_what_the_server_reports() {
    use wiremock::{
        matchers::{method, path},
        Mock, MockServer, ResponseTemplate,
    };

    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/show"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "parameters": "stop \"<|im_end|>\"
        num_ctx 16384",
            "model_info": {"general.architecture": "qwen3", "qwen3.context_length": 40960}
        })))
        .mount(&server)
        .await;
    let client = OllamaClient::with_model(server.uri(), "qwen3").unwrap();

    assert_eq!(client.resolve_context_window(Some(131_072)).await, 16_384);
    assert_eq!(client.resolve_context_window(Some(8_192)).await, 8_192);
    let client = client.with_context_window(16_384);
    assert_eq!(LLMPort::max_context_tokens(&client), 16_384);
    let body = client
        .chat_request(&lattice_core::application::ports::llm_port::CompletionRequest::default())
        .unwrap();
    assert_eq!(
        serde_json::to_value(body).unwrap()["options"]["num_ctx"],
        16_384
    );
}

#[test]
fn show_limits_read_the_trained_length_and_a_pinned_window() {
    let limits = ShowLimits::from_show(&serde_json::json!({
        "parameters": "num_ctx 8192",
        "model_info": {"llama.context_length": 131072}
    }));
    assert_eq!(limits.trained, Some(131_072));
    assert_eq!(limits.pinned, Some(8_192));
    assert_eq!(
        ShowLimits::from_show(&serde_json::json!({})),
        ShowLimits::default()
    );
}

#[tokio::test]
async fn a_server_that_cannot_say_leaves_the_configured_window() {
    let client = OllamaClient::with_model_and_timeouts(
        "http://127.0.0.1:9",
        "qwen3",
        Duration::from_millis(200),
        Duration::from_secs(1),
    )
    .unwrap();
    assert_eq!(client.resolve_context_window(Some(32_768)).await, 32_768);
}
