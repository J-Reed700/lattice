use super::*;
use wiremock::{
    matchers::{body_partial_json, header, method, path},
    Mock, MockServer, ResponseTemplate,
};

fn settings(url: String) -> LLMSettingsDto {
    LLMSettingsDto {
        llama_cpp: LlamaCppSettingsDto {
            url,
            model: "test-model".into(),
            auth_header_name: "Authorization".into(),
            auth_header_value: "Basic test-credential".into(),
        },
        ..Default::default()
    }
}
fn reply() -> Value {
    json!({"choices":[{"message":{"role":"assistant","content":"Hello"},"finish_reason":"stop"}]})
}

fn stream_reply(mut message: Value, reason: &str) -> String {
    if let Some(calls) = message.get_mut("tool_calls").and_then(Value::as_array_mut) {
        for (index, call) in calls.iter_mut().enumerate() {
            call.as_object_mut()
                .unwrap()
                .insert("index".into(), json!(index));
        }
    }
    format!(
        "data: {}\n\ndata: [DONE]\n\n",
        json!({"choices":[{"delta":message,"finish_reason":reason}]})
    )
}

#[tokio::test]
async fn first_response_and_followup_work_with_a_single_system_template() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(|request: &wiremock::Request| {
            let body: Value = serde_json::from_slice(&request.body).unwrap();
            let messages = body["messages"].as_array().unwrap();
            if messages
                .iter()
                .enumerate()
                .any(|(index, m)| index > 0 && m["role"] == "system")
            {
                return ResponseTemplate::new(400).set_body_json(json!({"error":{
                    "message":"System message must be at the beginning."}}));
            }
            assert_eq!(
                messages[0],
                json!({"role":"system","content":"Conversation override\n\nCite the PDF."})
            );
            assert!(!body.to_string().contains("Global default"));
            ResponseTemplate::new(200)
                .set_body_string(stream_reply(json!({"content":"Verified answer"}), "stop"))
        })
        .expect(2)
        .mount(&server)
        .await;
    let mut config = settings(server.uri());
    config.prompts.system_prompt = "Global default".into();
    let client = LlamaCppLlm::new(&config).unwrap();
    let mut context = vec![
        "System: Conversation override".into(),
        "System: Cite the PDF.".into(),
    ];
    let mut stream = client
        .generate_streaming("First question", &context, None)
        .await
        .unwrap();
    let mut answer = String::new();
    while let Some(chunk) = stream.next().await {
        answer.push_str(&chunk.unwrap());
    }
    assert_eq!(answer, "Verified answer");
    context.extend([
        "User: First question".into(),
        format!("Assistant: {answer}"),
        "System: Cite the PDF.".into(),
    ]);
    assert_eq!(
        client.generate("Follow-up", &context, None).await.unwrap(),
        "Verified answer"
    );
    let requests = server.received_requests().await.unwrap();
    let body: Value = serde_json::from_slice(&requests[1].body).unwrap();
    assert_eq!(
        body["messages"],
        json!([
            {"role":"system","content":"Conversation override\n\nCite the PDF."},
            {"role":"user","content":"First question"},
            {"role":"assistant","content":"Verified answer"},
            {"role":"user","content":"Follow-up"},
        ])
    );
}

#[test]
fn typed_system_messages_coalesce_without_altering_native_tools_or_images() {
    let client = LlamaCppLlm::new(&settings("http://localhost:8080".into())).unwrap();
    let assistant = json!({"role":"assistant","content":null,"reasoning_content":"reasoning", "tool_calls":[
        {"id":"call_1","type":"function","function":{"name":"search","arguments":"{}"}}
    ]});
    let request = CompletionRequest {
        input: vec![
            CompletionInput::Message {
                role: "system".into(),
                content: "First".into(),
            },
            CompletionInput::Message {
                role: "user".into(),
                content: "Question".into(),
            },
            CompletionInput::Native {
                value: assistant.clone(),
            },
            CompletionInput::Native {
                value: json!({"role":"system","content":"Second"}),
            },
            CompletionInput::ToolResult {
                id: "call_1".into(),
                output: "Result".into(),
            },
            CompletionInput::Message {
                role: "system".into(),
                content: "First".into(),
            },
        ],
        ..Default::default()
    };
    let body = client.body(&request, true).unwrap();
    assert_eq!(
        body["messages"],
        json!([
            {"role":"system","content":"First\n\nSecond"},
            {"role":"user","content":"Question"}, assistant,
            {"role":"tool","tool_call_id":"call_1","content":"Result"}
        ])
    );
    let image_request = client.legacy_request(
        "Describe",
        &["System: Image instructions".into()],
        Some(vec!["image-data".into()]),
    );
    let body = client.body(&image_request, true).unwrap();
    assert_eq!(body["messages"].as_array().unwrap().len(), 2);
    assert_eq!(body["messages"][0]["content"], "Image instructions");
    assert_eq!(body["messages"][1]["content"][0]["text"], "Describe");
    assert_eq!(
        body["messages"][1]["content"][1]["image_url"]["url"],
        "data:image/png;base64,image-data"
    );
}

#[tokio::test]
async fn template_rejections_explain_the_cause_without_echoing_private_content() {
    let server = MockServer::start().await;
    Mock::given(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(400).set_body_json(json!({"error":{
            "message":"Unable to generate parser for this template. Private prompt, Basic test-credential. System message must be at the beginning."
        }})))
        .mount(&server).await;
    let error = LlamaCppLlm::new(&settings(server.uri()))
        .unwrap()
        .generate("Private prompt", &[], None)
        .await
        .unwrap_err();
    assert!(matches!(error, AppError::InvalidState(_)));
    let message = error.to_string();
    assert!(message.contains("single system message at the beginning"));
    for secret in ["Private prompt", "test-credential", "check your connection"] {
        assert!(!message.contains(secret));
    }
}

/// Opt-in acceptance check through the real adapter and configured server. The
/// fixture supplies context/prompt; settings and credentials are never printed.
#[tokio::test]
#[ignore = "requires LATTICE_LLAMACPP_SETTINGS and LATTICE_LLAMACPP_CHAT_FIXTURE"]
async fn live_first_response_and_followup() {
    let settings_path = std::env::var("LATTICE_LLAMACPP_SETTINGS").unwrap();
    let fixture_path = std::env::var("LATTICE_LLAMACPP_CHAT_FIXTURE").unwrap();
    let settings: Value = serde_json::from_slice(&std::fs::read(settings_path).unwrap()).unwrap();
    let config: LLMSettingsDto =
        serde_json::from_value(settings["settings"]["llm"].clone()).unwrap();
    let fixture: Value = serde_json::from_slice(&std::fs::read(fixture_path).unwrap()).unwrap();
    let prompt = fixture["prompt"].as_str().unwrap();
    let mut context: Vec<String> = serde_json::from_value(fixture["context"].clone()).unwrap();
    let client = LlamaCppLlm::new(&config).unwrap();
    let start = std::time::Instant::now();
    let mut stream = client
        .generate_streaming(prompt, &context, None)
        .await
        .unwrap();
    let mut answer = String::new();
    while let Some(chunk) = stream.next().await {
        answer.push_str(&chunk.unwrap());
    }
    assert!(!answer.trim().is_empty(), "first response was empty");
    println!(
        "Live first response completed: {} characters in {:?}",
        answer.chars().count(),
        start.elapsed()
    );
    context.extend([format!("User: {prompt}"), format!("Assistant: {answer}")]);
    context.push("System: Keep this follow-up to one sentence.".into());
    let start = std::time::Instant::now();
    let followup = client
        .generate(
            "What source did you use in your previous answer?",
            &context,
            None,
        )
        .await
        .unwrap();
    assert!(!followup.trim().is_empty(), "follow-up was empty");
    println!(
        "Live follow-up completed: {} characters in {:?}",
        followup.chars().count(),
        start.elapsed()
    );
}

#[tokio::test]
async fn connection_test_checks_models_and_chat_with_the_same_auth() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/models"))
        .and(header("authorization", "Basic test-credential"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({"data":[{"id":"test-model"}]})),
        )
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .and(header("authorization", "Basic test-credential"))
        .and(body_partial_json(
            json!({"model":"test-model","stream":false}),
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(reply()))
        .expect(1)
        .mount(&server)
        .await;
    let models = LlamaCppLlm::test_connection(&settings(format!("{}/v1/", server.uri())).llama_cpp)
        .await
        .unwrap();
    assert_eq!(models, ["test-model"]);
}

#[tokio::test]
async fn model_listing_success_does_not_hide_a_missing_chat_endpoint() {
    let server = MockServer::start().await;
    Mock::given(path("/v1/models"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({"data":[{"id":"test-model"}]})),
        )
        .mount(&server)
        .await;
    let error = LlamaCppLlm::test_connection(&settings(server.uri()).llama_cpp)
        .await
        .unwrap_err()
        .to_string();
    assert!(error.contains("404"));
    assert!(error.contains("/v1/chat/completions"));
}

#[tokio::test]
async fn tools_and_native_assistant_messages_survive_followup() {
    let server = MockServer::start().await;
    let message = json!({"role":"assistant","content":"","reasoning_content":"retained reasoning",
        "tool_calls":[{"id":"call_1","type":"function","function":{"name":"search","arguments":"{\"q\":\"rust\"}"}}]});
    Mock::given(method("POST")).and(path("/v1/chat/completions")).and(header("authorization","Basic test-credential"))
        .and(body_partial_json(json!({"stream":true,"messages":[{"role":"user","content":"Find rust"}],"tools":[{"type":"function","function":{"name":"search","description":"Search","parameters":{"type":"object"}}}]})))
        .respond_with(ResponseTemplate::new(200).set_body_string(stream_reply(message.clone(), "tool_calls"))).expect(1).mount(&server).await;
    let client = LlamaCppLlm::new(&settings(server.uri())).unwrap();
    let response = client
        .complete(&CompletionRequest {
            input: vec![CompletionInput::Message {
                role: "user".into(),
                content: "Find rust".into(),
            }],
            tools: vec![crate::application::ports::ToolDefinition {
                name: "search".into(),
                description: "Search".into(),
                parameters: json!({"type":"object"}),
            }],
            ..Default::default()
        })
        .await
        .unwrap();
    assert!(
        matches!(response.tool_calls.first(), Some(CompletionInput::ToolCall { id, name, .. }) if id == "call_1" && name == "search")
    );
    let followup = CompletionRequest {
        input: vec![
            CompletionInput::Native {
                value: response.provider_output,
            },
            CompletionInput::ToolResult {
                id: "call_1".into(),
                output: "Found Rust docs".into(),
            },
        ],
        ..Default::default()
    };
    Mock::given(path("/v1/chat/completions")).and(body_partial_json(json!({"messages":[message,{"role":"tool","tool_call_id":"call_1","content":"Found Rust docs"}]})))
        .respond_with(ResponseTemplate::new(200).set_body_string(stream_reply(json!({"role":"assistant","content":"Hello"}), "stop"))).expect(1).mount(&server).await;
    assert_eq!(client.complete(&followup).await.unwrap().text, "Hello");
}

#[tokio::test]
async fn streaming_uses_chat_completions_with_auth_and_emits_content() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .and(header("authorization", "Basic test-credential"))
        .and(body_partial_json(
            json!({"stream":true,"model":"test-model"}),
        ))
        .respond_with(ResponseTemplate::new(200).set_body_string(
            "data: {\"choices\":[{\"delta\":{\"content\":\"Hello\"},\"finish_reason\":\"stop\"}]}\n\ndata: [DONE]\n\n",
        ))
        .expect(1)
        .mount(&server)
        .await;
    let client = LlamaCppLlm::new(&settings(server.uri())).unwrap();
    let mut stream = client.generate_streaming("Hi", &[], None).await.unwrap();
    assert_eq!(stream.next().await.unwrap().unwrap(), "Hello");
    assert!(stream.next().await.is_none());
}

#[tokio::test]
async fn errors_do_not_expose_credentials_or_response_bodies() {
    let server = MockServer::start().await;
    Mock::given(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(401).set_body_string("private response-body"))
        .mount(&server)
        .await;
    let error = LlamaCppLlm::new(&settings(server.uri()))
        .unwrap()
        .generate("Private prompt", &[], None)
        .await
        .unwrap_err()
        .to_string();
    assert!(error.contains("401"));
    for secret in ["test-credential", "private response-body", "Private prompt"] {
        assert!(!error.contains(secret));
    }
}

#[test]
fn sse_handles_split_utf8_and_rejects_failed_or_truncated_streams() {
    let event =
        "data: {\"choices\":[{\"delta\":{\"content\":\"你好🙂\"},\"finish_reason\":\"stop\"}]}\r\n\r\ndata: [DONE]\r\n\r\n";
    let mut decoder = streaming::Decoder::default();
    let mut text = Vec::new();
    for byte in event.bytes() {
        text.extend(decoder.push(&[byte]).unwrap());
    }
    decoder.finish().unwrap();
    assert_eq!(text.concat(), "你好🙂");
    assert!(streaming::Decoder::default().finish().is_err());
    for event in [
        r#"{"error":{"message":"private"}}"#,
        r#"{"choices":[{"delta":{},"finish_reason":"length"}]}"#,
    ] {
        assert!(streaming::Decoder::default()
            .push(format!("data: {event}\n\n").as_bytes())
            .is_err());
    }
}

#[test]
fn connections_reject_unsafe_urls_and_partial_auth() {
    for (url, valid) in [
        ("https://example.com", true),
        ("https://example.com/v1/", true),
        ("file:///tmp/test", false),
        ("https://user:pass@example.com", false),
        ("https://example.com?token=secret", false),
    ] {
        assert_eq!(
            validate_connection(&settings(url.into()).llama_cpp).is_ok(),
            valid
        );
    }
    let mut connection = settings("https://example.com".into()).llama_cpp;
    connection.auth_header_value.clear();
    assert!(validate_connection(&connection).is_err());
}

#[tokio::test]
async fn ollama_test_does_not_accept_a_llama_cpp_model_list() {
    let server = MockServer::start().await;
    Mock::given(path("/v1/models"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({"data":[{"id":"test-model"}]})),
        )
        .expect(0)
        .mount(&server)
        .await;
    let error = crate::features::settings::plugin::test_ollama_connection(
        crate::features::settings::plugin::TestOllamaConnectionRequest {
            ollama_url: server.uri(),
            auth_header_name: String::new(),
            auth_header_value: String::new(),
        },
    )
    .await
    .unwrap_err();
    assert!(error
        .details
        .as_deref()
        .is_some_and(|details| details.contains("llama.cpp")));
}

#[tokio::test]
async fn ollama_test_keeps_native_model_discovery() {
    let server = MockServer::start().await;
    Mock::given(path("/api/tags"))
        .and(header("authorization", "Basic test-credential"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({"models":[{"name":"llama3.2"}]})),
        )
        .expect(1)
        .mount(&server)
        .await;
    let response = crate::features::settings::plugin::test_ollama_connection(
        crate::features::settings::plugin::TestOllamaConnectionRequest {
            ollama_url: server.uri(),
            auth_header_name: "Authorization".into(),
            auth_header_value: "Basic test-credential".into(),
        },
    )
    .await
    .unwrap();
    assert_eq!(response.endpoint, "/api/tags");
    assert_eq!(response.models, ["llama3.2"]);
}

#[test]
fn streamed_tool_arguments_reasoning_and_usage_survive_fragmentation() {
    let events = [
        json!({"choices":[{"delta":{"reasoning_content":"Look ","tool_calls":[{"index":1,"id":"second","function":{"name":"search","arguments":"{\"q\":\""}}]}}]}),
        json!({"choices":[{"delta":{"reasoning_content":"here","tool_calls":[{"index":0,"id":"first","function":{"name":"lookup","arguments":"{}"}},{"index":1,"function":{"arguments":"专利\"}"}}]}}]}),
        json!({"choices":[{"delta":{},"finish_reason":"tool_calls"}]}),
        json!({"choices":[],"usage":{"prompt_tokens":20,"completion_tokens":30}}),
    ];
    let mut decoder = streaming::Decoder::for_completion();
    for event in events {
        for byte in format!("data: {event}\r\n\r\n").bytes() {
            decoder.push(&[byte]).unwrap();
        }
    }
    decoder.push(b"data: [DONE]\n\n").unwrap();
    let response = decoder.into_response().unwrap();
    assert_eq!(response.finish_reason, "tool_calls");
    assert_eq!(response.input_tokens, 20);
    assert_eq!(response.output_tokens, 30);
    assert_eq!(response.provider_output["reasoning_content"], "Look here");
    assert!(matches!(&response.tool_calls[0], CompletionInput::ToolCall {id, ..} if id == "first"));
    assert!(
        matches!(&response.tool_calls[1], CompletionInput::ToolCall {id, arguments, ..} if id == "second" && *arguments == json!({"q":"专利"}))
    );
}

#[test]
fn incomplete_or_invalid_streamed_tool_calls_fail() {
    for arguments in ["{", "not json"] {
        let mut decoder = streaming::Decoder::for_completion();
        decoder.push(stream_reply(json!({"tool_calls":[{"id":"call", "function":{"name":"search", "arguments":arguments}}]}), "tool_calls").as_bytes()).unwrap();
        assert!(decoder.into_response().is_err());
    }
    let mut decoder = streaming::Decoder::for_completion();
    decoder
        .push(b"data: {\"choices\":[{\"delta\":{\"content\":\"partial\"}}]}\n\n")
        .unwrap();
    assert!(decoder.into_response().is_err());
}

async fn read_request_body(socket: &mut tokio::net::TcpStream) -> Value {
    use tokio::io::AsyncReadExt;
    let mut request = Vec::new();
    let mut buffer = [0u8; 4096];
    loop {
        let count = socket.read(&mut buffer).await.unwrap();
        assert!(count > 0);
        request.extend_from_slice(&buffer[..count]);
        if let Some(end) = request.windows(4).position(|window| window == b"\r\n\r\n") {
            let headers = String::from_utf8_lossy(&request[..end]);
            let length: usize = headers
                .lines()
                .find_map(|line| {
                    let (name, value) = line.split_once(':')?;
                    name.eq_ignore_ascii_case("content-length")
                        .then(|| value.trim().parse().unwrap())
                })
                .unwrap();
            if request.len() >= end + 4 + length {
                return serde_json::from_slice(&request[end + 4..end + 4 + length]).unwrap();
            }
        }
    }
}

#[tokio::test]
async fn active_completion_can_outlast_the_read_timeout() {
    use tokio::io::AsyncWriteExt;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let body = read_request_body(&mut socket).await;
        assert_eq!(body["stream"], true);
        assert_eq!(body["stream_options"]["include_usage"], true);
        assert_eq!(body["return_progress"], true);
        let chunks = [
            "data: {\"choices\":[{\"delta\":{\"reasoning_content\":\"Thinking\"}}]}\n\n",
            "data: {\"choices\":[{\"delta\":{\"content\":\"Hello\"},\"finish_reason\":\"stop\"}]}\n\n",
            "data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}]}\n\ndata: [DONE]\n\n",
        ];
        let size: usize = chunks.iter().map(|chunk| chunk.len()).sum();
        socket.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {size}\r\nConnection: close\r\n\r\n").as_bytes()).await.unwrap();
        for chunk in chunks {
            tokio::time::sleep(Duration::from_millis(650)).await;
            socket.write_all(chunk.as_bytes()).await.unwrap();
        }
    });
    let mut config = settings(format!("http://{address}"));
    config.timeout_seconds = 1;
    let client = LlamaCppLlm::new(&config).unwrap();
    let response = client
        .complete(&CompletionRequest::default())
        .await
        .unwrap();
    assert_eq!(response.text, "Hello");
    server.await.unwrap();
}

const PROMPT_PROGRESS_EVENT: &str = "data: {\"choices\":[{\"finish_reason\":null,\"index\":0,\"delta\":{\"role\":\"assistant\",\"content\":null}}],\"prompt_progress\":{\"total\":10,\"cache\":0,\"processed\":0,\"time_ms\":1}}\n\n";

#[tokio::test]
async fn stream_that_goes_silent_after_starting_is_retried() {
    use tokio::io::AsyncWriteExt;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    tokio::spawn(async move {
        // First attempt: an answer starts, then silence with the body unfinished.
        let (mut stalled, _) = listener.accept().await.unwrap();
        read_request_body(&mut stalled).await;
        let started = format!(
            "{PROMPT_PROGRESS_EVENT}data: {{\"choices\":[{{\"delta\":{{\"content\":\"Partial\"}}}}]}}\n\n"
        );
        stalled.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\n\r\n{started}", started.len() + 1024).as_bytes()).await.unwrap();
        let (mut healthy, _) = listener.accept().await.unwrap();
        read_request_body(&mut healthy).await;
        let body = format!(
            "{PROMPT_PROGRESS_EVENT}{}",
            stream_reply(json!({"content":"Recovered"}), "stop")
        );
        healthy.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).as_bytes()).await.unwrap();
        drop(stalled);
    });
    let mut config = settings(format!("http://{address}"));
    config.timeout_seconds = 1;
    let client = LlamaCppLlm::new(&config).unwrap();
    let retries = std::sync::atomic::AtomicUsize::new(0);
    let response = client
        .complete_with_retry_progress(&CompletionRequest::default(), &|_| Ok(()), &|_| {
            retries.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Ok(())
        })
        .await
        .unwrap();
    assert_eq!(response.text, "Recovered");
    assert_eq!(retries.load(std::sync::atomic::Ordering::SeqCst), 1);
}

/// Prompt processing can outlast the stall timeout on a large prompt; the events
/// that report it must not make the request less patient than silence would.
#[tokio::test]
async fn prompt_progress_alone_does_not_arm_the_stall_timer() {
    use tokio::io::AsyncWriteExt;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        read_request_body(&mut socket).await;
        let answer = stream_reply(json!({"content":"Processed"}), "stop");
        let size = PROMPT_PROGRESS_EVENT.len() + answer.len();
        socket.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {size}\r\nConnection: close\r\n\r\n{PROMPT_PROGRESS_EVENT}").as_bytes()).await.unwrap();
        tokio::time::sleep(Duration::from_millis(1500)).await;
        socket.write_all(answer.as_bytes()).await.unwrap();
    });
    let mut config = settings(format!("http://{address}"));
    config.timeout_seconds = 1;
    let client = LlamaCppLlm::new(&config).unwrap();
    let response = client
        .complete(&CompletionRequest::default())
        .await
        .unwrap();
    assert_eq!(response.text, "Processed");
    server.await.unwrap();
}

/// A second full budget would let the retry finish; a shared one must not.
#[tokio::test]
async fn a_retry_shares_the_budget_instead_of_restarting_it() {
    let server = MockServer::start().await;
    let attempts = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(move |_: &wiremock::Request| {
            let delay = Duration::from_millis(400);
            if attempts.fetch_add(1, std::sync::atomic::Ordering::SeqCst) == 0 {
                ResponseTemplate::new(503)
                    .insert_header("Retry-After", "0")
                    .set_delay(delay)
            } else {
                sse_response(json!({"content":"Too late"})).set_delay(delay)
            }
        })
        .mount(&server)
        .await;
    let client = LlamaCppLlm::new(&settings(server.uri())).unwrap();
    let error = client
        .complete(&CompletionRequest {
            time_budget: Some(Duration::from_millis(600)),
            ..Default::default()
        })
        .await
        .unwrap_err();
    assert!(error.to_string().contains("time budget"), "{error}");
}

#[tokio::test]
async fn queued_request_may_wait_longer_than_the_stall_timeout() {
    let server = sequence_server(vec![
        sse_response(json!({"content":"Served"})).set_delay(Duration::from_millis(1500))
    ])
    .await;
    let mut config = settings(server.uri());
    config.timeout_seconds = 1;
    let client = LlamaCppLlm::new(&config).unwrap();
    let response = client
        .complete(&CompletionRequest::default())
        .await
        .unwrap();
    assert_eq!(response.text, "Served");
    assert_eq!(server.received_requests().await.unwrap().len(), 1);
}

#[tokio::test]
async fn time_budget_bounds_the_whole_generation() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(sse_response(json!({"content":"Late"})).set_delay(Duration::from_secs(5)))
        .mount(&server)
        .await;
    let client = LlamaCppLlm::new(&settings(server.uri())).unwrap();
    let error = client
        .complete(&CompletionRequest {
            time_budget: Some(Duration::from_millis(300)),
            ..Default::default()
        })
        .await
        .unwrap_err();
    assert!(error.to_string().contains("time budget"), "{error}");
}

#[test]
fn prompt_progress_events_are_not_answer_text() {
    let mut decoder = streaming::Decoder::for_completion();
    assert!(decoder
        .push(PROMPT_PROGRESS_EVENT.as_bytes())
        .unwrap()
        .is_empty());
    decoder
        .push(stream_reply(json!({"content":"Answer"}), "stop").as_bytes())
        .unwrap();
    assert_eq!(decoder.into_response().unwrap().text, "Answer");
}

#[tokio::test]
async fn typed_completion_delivers_answer_progress_and_preserves_tool_response() {
    let server = MockServer::start().await;
    Mock::given(method("POST")).and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_string(
            "data: {\"choices\":[{\"delta\":{\"reasoning_content\":\"private reasoning\"}}]}\n\ndata: {\"choices\":[{\"delta\":{\"content\":\"First \"}}]}\n\ndata: {\"choices\":[{\"delta\":{\"content\":\"answer\"},\"finish_reason\":\"stop\"}]}\n\ndata: [DONE]\n\n"))
        .mount(&server).await;
    let llm = LlamaCppLlm::new(&settings(server.uri())).unwrap();
    let deltas = std::sync::Mutex::new(Vec::new());
    let on_text = |text: String| {
        deltas.lock().unwrap().push(text);
        Ok(())
    };
    let response = llm
        .complete_with_progress(
            &CompletionRequest {
                input: vec![CompletionInput::Message {
                    role: "user".into(),
                    content: "A question".into(),
                }],
                ..Default::default()
            },
            &on_text,
        )
        .await
        .unwrap();
    assert_eq!(*deltas.lock().unwrap(), vec!["First ", "answer"]);
    assert_eq!(response.text, "First answer");
    assert_eq!(response.finish_reason, "stop");
}

async fn sequence_server(responses: Vec<ResponseTemplate>) -> MockServer {
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };
    let server = MockServer::start().await;
    let count = responses.len();
    let index = Arc::new(AtomicUsize::new(0));
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(move |_: &wiremock::Request| {
            responses[index.fetch_add(1, Ordering::SeqCst).min(count - 1)].clone()
        })
        .expect(count as u64)
        .mount(&server)
        .await;
    server
}
fn sse_response(message: Value) -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_string(stream_reply(message, "stop"))
}

#[tokio::test]
async fn reasoning_only_empty_and_transient_failures_retry_identical_request() {
    let server = sequence_server(vec![
        sse_response(json!({"reasoning_content":"private thought"})),
        sse_response(json!({"content":""})),
        ResponseTemplate::new(429).insert_header("Retry-After", "0"),
        sse_response(json!({"content":"Recovered answer"})),
    ])
    .await;
    let client = LlamaCppLlm::new(&settings(server.uri())).unwrap();
    let response = client
        .complete(&CompletionRequest::default())
        .await
        .unwrap();
    assert_eq!(response.text, "Recovered answer");
    let requests = server.received_requests().await.unwrap();
    assert!(requests.windows(2).all(|pair| pair[0].body == pair[1].body));
}

#[tokio::test]
async fn failed_partial_draft_is_reset_before_recovery_and_tools_are_returned_once() {
    let truncated = ResponseTemplate::new(200).set_body_string(
        "data: {\"choices\":[{\"delta\":{\"content\":\"Discard this\",\"tool_calls\":[{\"index\":0,\"id\":\"partial\",\"function\":{\"name\":\"search\",\"arguments\":\"{\"}}]}}]}\n\n"
    );
    let server = sequence_server(vec![truncated, sse_response(json!({
        "content":"Recovered", "tool_calls":[{"id":"valid", "type":"function", "function":{"name":"search", "arguments":"{}"}}]
    }))]).await;
    let client = LlamaCppLlm::new(&settings(server.uri())).unwrap();
    let events = std::sync::Mutex::new(Vec::new());
    let response = client
        .complete_with_retry_progress(
            &CompletionRequest::default(),
            &|text| {
                events.lock().unwrap().push(text);
                Ok(())
            },
            &|attempt| {
                events.lock().unwrap().push(format!("reset {attempt}"));
                Ok(())
            },
        )
        .await
        .unwrap();
    assert_eq!(
        *events.lock().unwrap(),
        vec!["Discard this", "reset 2", "Recovered"]
    );
    assert_eq!(response.tool_calls.len(), 1);
    assert!(
        matches!(&response.tool_calls[0], CompletionInput::ToolCall { id, .. } if id == "valid")
    );
}

#[tokio::test]
async fn empty_response_exhaustion_returns_error_after_five_attempts() {
    let server = sequence_server(vec![sse_response(json!({"reasoning":"hidden"})); 5]).await;
    let client = LlamaCppLlm::new(&settings(server.uri())).unwrap();
    assert!(client
        .complete(&CompletionRequest::default())
        .await
        .is_err());
}

#[tokio::test]
async fn authentication_invalid_tools_and_output_limit_are_not_retried() {
    for response in [
        ResponseTemplate::new(401),
        sse_response(
            json!({"tool_calls":[{"id":"x","type":"function","function":{"name":"search","arguments":"[1]"}}]}),
        ),
        ResponseTemplate::new(200)
            .set_body_string(stream_reply(json!({"content":"unfinished"}), "length")),
    ] {
        let server = sequence_server(vec![response]).await;
        let client = LlamaCppLlm::new(&settings(server.uri())).unwrap();
        assert!(client
            .complete(&CompletionRequest::default())
            .await
            .is_err());
    }
}

#[tokio::test]
async fn dropping_generation_during_backoff_prevents_another_request() {
    let server = sequence_server(vec![
        ResponseTemplate::new(503).insert_header("Retry-After", "30")
    ])
    .await;
    let client = LlamaCppLlm::new(&settings(server.uri())).unwrap();
    let reset = std::sync::atomic::AtomicBool::new(false);
    let request = CompletionRequest::default();
    let on_retry = |_| {
        reset.store(true, std::sync::atomic::Ordering::SeqCst);
        Ok(())
    };
    let result = tokio::time::timeout(
        Duration::from_millis(150),
        client.complete_with_retry_progress(&request, &|_| Ok(()), &on_retry),
    )
    .await;
    assert!(result.is_err());
    assert!(reset.load(std::sync::atomic::Ordering::SeqCst));
    assert_eq!(server.received_requests().await.unwrap().len(), 1);
}

#[tokio::test]
async fn callback_failure_does_not_retry() {
    let server = sequence_server(vec![sse_response(json!({"content":"Answer"}))]).await;
    let client = LlamaCppLlm::new(&settings(server.uri())).unwrap();
    assert!(client
        .complete_with_retry_progress(
            &CompletionRequest::default(),
            &|_| Err(AppError::InvalidState("cancelled".into())),
            &|_| Ok(())
        )
        .await
        .is_err());
}

#[test]
fn done_marker_without_finish_reason_and_duplicate_tool_ids_are_rejected() {
    let mut decoder = streaming::Decoder::for_completion();
    decoder.push(b"data: [DONE]\n\n").unwrap();
    assert!(decoder.into_response().is_err());
    let call = json!({"id":"same","type":"function","function":{"name":"search","arguments":"{}"}});
    assert!(parse_completion(json!({"choices":[{"message":{"tool_calls":[call.clone(), call]},"finish_reason":"tool_calls"}]})).is_err());
}

#[tokio::test]
async fn text_only_progress_does_not_repeat_a_partial_answer() {
    let server = sequence_server(vec![ResponseTemplate::new(200)
        .set_body_string("data: {\"choices\":[{\"delta\":{\"content\":\"Partial\"}}]}\n\n")])
    .await;
    let client = LlamaCppLlm::new(&settings(server.uri())).unwrap();
    let text = std::sync::Mutex::new(String::new());
    let result = client
        .complete_with_progress(&CompletionRequest::default(), &|chunk| {
            text.lock().unwrap().push_str(&chunk);
            Ok(())
        })
        .await;
    assert!(result.is_err());
    assert_eq!(*text.lock().unwrap(), "Partial");
}
