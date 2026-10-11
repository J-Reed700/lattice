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

/// The assistant message a response replays.
fn assistant(response: &CompletionResponse) -> Value {
    match response.replay.as_slice() {
        [CompletionInput::Native { value }] => value.clone(),
        other => panic!("a llama-server reply replays as one message, not {other:?}"),
    }
}

/// Plain text through the typed path, as string-only callers make it.
async fn text(client: &LlamaCppLlm, prompt: &str, context: &[String]) -> Result<String> {
    crate::application::services::completion_input::complete_text(
        client,
        prompt,
        context,
        Default::default(),
    )
    .await
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
    let answer = text(&client, "First question", &context).await.unwrap();
    assert_eq!(answer, "Verified answer");
    context.extend([
        "User: First question".into(),
        format!("Assistant: {answer}"),
        "System: Cite the PDF.".into(),
    ]);
    assert_eq!(
        text(&client, "Follow-up", &context).await.unwrap(),
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
    let body = client.body(&request).unwrap();
    assert_eq!(
        body["messages"],
        json!([
            {"role":"system","content":"First\n\nSecond"},
            {"role":"user","content":"Question"}, assistant,
            {"role":"tool","tool_call_id":"call_1","content":"Result"}
        ])
    );
    let image = json!({"role":"user","content":[
        {"type":"text","text":"Describe"},
        {"type":"image_url","image_url":{"url":"data:image/png;base64,image-data"}}
    ]});
    let image_request = CompletionRequest {
        input: vec![
            CompletionInput::Message {
                role: "system".into(),
                content: "Image instructions".into(),
            },
            CompletionInput::Native {
                value: image.clone(),
            },
        ],
        ..Default::default()
    };
    let body = client.body(&image_request).unwrap();
    assert_eq!(
        body["messages"],
        json!([{"role":"system","content":"Image instructions"}, image])
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
    let error = text(
        &LlamaCppLlm::new(&settings(server.uri())).unwrap(),
        "Private prompt",
        &[],
    )
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
    let answer = text(&client, prompt, &context).await.unwrap();
    assert!(!answer.trim().is_empty(), "first response was empty");
    println!(
        "Live first response completed: {} characters in {:?}",
        answer.chars().count(),
        start.elapsed()
    );
    context.extend([format!("User: {prompt}"), format!("Assistant: {answer}")]);
    context.push("System: Keep this follow-up to one sentence.".into());
    let start = std::time::Instant::now();
    let followup = text(
        &client,
        "What source did you use in your previous answer?",
        &context,
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
        .and(body_partial_json(json!({"stream":true,"messages":[{"role":"user","content":"Find rust"}],"tools":[{"type":"function","function":{"name":"search","description":"Search","parameters":{"type":"object"}}}],"parallel_tool_calls":true})))
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
    let mut input = response.replay;
    input.push(CompletionInput::ToolResult {
        id: "call_1".into(),
        output: "Found Rust docs".into(),
    });
    let followup = CompletionRequest {
        input,
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
    let shown = std::sync::Mutex::new(Vec::new());
    let on_text = |text: String| {
        shown.lock().unwrap().push(text);
        Ok(())
    };
    let response = client
        .complete_with_progress(&CompletionRequest::default(), &on_text)
        .await
        .unwrap();
    assert_eq!(response.text, "Hello");
    assert_eq!(*shown.lock().unwrap(), vec!["Hello".to_string()]);
}

#[tokio::test]
async fn errors_do_not_expose_credentials_or_response_bodies() {
    let server = MockServer::start().await;
    Mock::given(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(401).set_body_string("private response-body"))
        .mount(&server)
        .await;
    let error = text(
        &LlamaCppLlm::new(&settings(server.uri())).unwrap(),
        "Private prompt",
        &[],
    )
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
        r#"{"choices":[{"delta":{},"finish_reason":"content_filter"}]}"#,
    ] {
        assert!(streaming::Decoder::default()
            .push(format!("data: {event}\n\n").as_bytes())
            .is_err());
    }
}

/// Running out of answer room keeps what was written; the caller sees why it
/// stopped and decides what a cut-short answer is worth.
#[test]
fn sse_length_stop_keeps_the_text_and_reports_the_reason() {
    let mut decoder = streaming::Decoder::for_completion();
    let text = decoder
        .push(b"data: {\"choices\":[{\"delta\":{\"content\":\"half an answer\"},\"finish_reason\":\"length\"}]}\n\ndata: [DONE]\n\n")
        .unwrap();
    assert_eq!(text.concat(), "half an answer");
    let response = decoder.into_response().unwrap();
    assert_eq!(response.text, "half an answer");
    assert_eq!(response.finish_reason, "length");
}

#[test]
fn sse_error_names_the_servers_error_type_and_never_its_message() {
    let failure = |event: &str| {
        streaming::Decoder::default()
            .push(format!("data: {event}\n\n").as_bytes())
            .unwrap_err()
            .to_string()
    };

    let named =
        failure(r#"{"error":{"code":400,"type":"exceed_context_size_error","message":"private"}}"#);
    assert!(named.contains("(400 exceed_context_size_error)"), "{named}");
    assert!(!named.contains("private"));

    // A type that is not an identifier is free text, and is dropped like one.
    let unnamed = failure(r#"{"error":{"type":"The prompt said: private"}}"#);
    assert!(!unnamed.contains("private"));
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
    assert_eq!(assistant(&response)["reasoning_content"], "Look here");
    assert_eq!(response.reasoning.as_deref(), Some("Look here"));
    assert!(matches!(&response.tool_calls[0], CompletionInput::ToolCall {id, ..} if id == "first"));
    assert!(
        matches!(&response.tool_calls[1], CompletionInput::ToolCall {id, arguments, ..} if id == "second" && *arguments == json!({"q":"专利"}))
    );
}

#[test]
fn a_malformed_streamed_tool_call_comes_back_as_an_invalid_call_not_an_error() {
    for (arguments, finish) in [
        ("{", "tool_calls"),
        ("not json", "tool_calls"),
        ("{\"q\":\"ru", "length"),
    ] {
        let mut decoder = streaming::Decoder::for_completion();
        decoder.push(stream_reply(json!({"content":"Let me look.","tool_calls":[{"id":"call", "function":{"name":"search", "arguments":arguments}}]}), finish).as_bytes()).unwrap();
        let response = decoder.into_response().unwrap();
        assert_eq!(response.text, "Let me look.");
        let [CompletionInput::ToolCall {
            id,
            name,
            arguments: marker,
        }] = response.tool_calls.as_slice()
        else {
            panic!("expected one call, got {:?}", response.tool_calls);
        };
        assert_eq!((id.as_str(), name.as_str()), ("call", "search"));
        assert_eq!(marker["raw"], arguments);
        let problem = invalid_tool_call_problem(marker).unwrap();
        assert!(problem.contains(arguments), "{problem}");
        if finish == "length" {
            assert!(problem.contains("cut off"), "{problem}");
        }
        // The replayed assistant message carries arguments the template can parse.
        let message = assistant(&response);
        let replayed = message["tool_calls"][0]["function"]["arguments"]
            .as_str()
            .unwrap();
        assert!(serde_json::from_str::<Value>(replayed).is_ok());
    }
    let mut decoder = streaming::Decoder::for_completion();
    decoder
        .push(b"data: {\"choices\":[{\"delta\":{\"content\":\"partial\"}}]}\n\n")
        .unwrap();
    assert!(decoder.into_response().is_err());
}

#[test]
fn a_call_with_no_id_or_no_name_is_given_one_and_kept() {
    let response = parse_completion(json!({"choices":[{"message":{"tool_calls":[
        {"type":"function","function":{"name":"search","arguments":"{\"q\":\"x\"}"}},
        {"id":"b","type":"function","function":{"arguments":"{}"}}
    ]},"finish_reason":"tool_calls"}]}))
    .unwrap();
    let ids: Vec<_> = response
        .tool_calls
        .iter()
        .map(|call| match call {
            CompletionInput::ToolCall { id, arguments, .. } => {
                (id.clone(), invalid_tool_call_problem(arguments).is_some())
            }
            other => panic!("unexpected {other:?}"),
        })
        .collect();
    // The id-less call is valid and runnable; the nameless one is reported.
    assert_eq!(
        ids,
        [
            ("call_invalid_0".to_string(), false),
            ("b".to_string(), true)
        ]
    );
    assert_eq!(
        assistant(&response)["tool_calls"][0]["id"],
        "call_invalid_0"
    );
}

#[test]
fn ordinary_arguments_are_never_mistaken_for_the_invalid_marker() {
    assert!(invalid_tool_call_problem(&json!({"invalid": true, "query": "x"})).is_none());
    assert!(invalid_tool_call_problem(&json!({"q": "x"})).is_none());
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

#[tokio::test]
async fn typed_completion_delivers_reasoning_on_its_own_progress_channel() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_string(
            "data: {\"choices\":[{\"delta\":{\"reasoning_content\":\"Compare \"}}]}\n\ndata: {\"choices\":[{\"delta\":{\"reasoning_content\":\"sources\"}}]}\n\ndata: {\"choices\":[{\"delta\":{\"content\":\"Answer\"},\"finish_reason\":\"stop\"}]}\n\ndata: [DONE]\n\n",
        ))
        .mount(&server)
        .await;
    let llm = LlamaCppLlm::new(&settings(server.uri())).unwrap();
    let reasoning = std::sync::Mutex::new(String::new());
    let response = llm
        .complete_with_reasoning_progress(
            &CompletionRequest::default(),
            &|_| Ok(()),
            &|delta| {
                reasoning.lock().unwrap().push_str(&delta);
                Ok(())
            },
            &|_| Ok(()),
        )
        .await
        .unwrap();

    assert_eq!(*reasoning.lock().unwrap(), "Compare sources");
    assert_eq!(response.reasoning.as_deref(), Some("Compare sources"));
    assert_eq!(response.text, "Answer");
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
async fn response_status_preserves_outages_for_durable_callers() {
    let statuses = [408, 429, 400, 401, 503];
    let server = sequence_server(
        statuses
            .iter()
            .map(|status| ResponseTemplate::new(*status))
            .collect(),
    )
    .await;
    for status in statuses {
        let response = reqwest::Client::new()
            .post(format!("{}/v1/chat/completions", server.uri()))
            .send()
            .await
            .unwrap();
        let error = check_status(response).await.unwrap_err();
        match status {
            408 => assert!(matches!(error, AppError::Network(_))),
            429 => assert!(matches!(error, AppError::RateLimitExceeded(_))),
            503 => assert!(matches!(error, AppError::ServiceNotAvailable(_))),
            _ => assert!(matches!(error, AppError::InvalidState(_))),
        }
    }
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
async fn transient_sse_errors_retry_but_context_errors_do_not() {
    let server = sequence_server(vec![
        ResponseTemplate::new(200).set_body_string(
            "data: {\"error\":{\"code\":500,\"type\":\"server_error\",\"message\":\"private\"}}\n\n",
        ),
        sse_response(json!({"content":"Recovered"})),
    ])
    .await;
    let client = LlamaCppLlm::new(&settings(server.uri())).unwrap();
    let attempts = std::sync::Mutex::new(Vec::new());
    let response = client
        .complete_with_retry_progress(&CompletionRequest::default(), &|_| Ok(()), &|attempt| {
            attempts.lock().unwrap().push(attempt);
            Ok(())
        })
        .await
        .unwrap();
    assert_eq!(response.text, "Recovered");
    assert_eq!(*attempts.lock().unwrap(), vec![2]);
    let requests = server.received_requests().await.unwrap();
    let mut original: Value = serde_json::from_slice(&requests[0].body).unwrap();
    let recovered: Value = serde_json::from_slice(&requests[1].body).unwrap();
    assert!(original.get("cache_prompt").is_none());
    original["cache_prompt"] = json!(false);
    assert_eq!(
        original, recovered,
        "Only server prompt-cache reuse changes"
    );

    let server = sequence_server(vec![ResponseTemplate::new(200).set_body_string(
        "data: {\"error\":{\"code\":400,\"type\":\"exceed_context_size_error\",\"message\":\"private\"}}\n\n",
    )])
    .await;
    let error = LlamaCppLlm::new(&settings(server.uri()))
        .unwrap()
        .complete(&CompletionRequest::default())
        .await
        .unwrap_err();
    assert!(error.to_string().contains("exceed_context_size_error"));
    assert!(!error.to_string().contains("private"));
    assert!(matches!(error, AppError::InvalidState(_)));
}

#[test]
fn streamed_server_errors_preserve_their_failure_category_without_private_text() {
    for (code, expected) in [
        (500, "service"),
        (503, "service"),
        (429, "rate"),
        (408, "network"),
        (400, "invalid"),
    ] {
        let mut decoder = streaming::Decoder::for_completion();
        let event = format!(
            "data: {}\n\n",
            json!({"error":{"code":code,"type":"server_error","message":"private prompt and credentials"}})
        );
        let error = decoder.push(event.as_bytes()).unwrap_err();
        assert!(!error.to_string().contains("private"));
        let actual = match error {
            AppError::ServiceNotAvailable(_) => "service",
            AppError::RateLimitExceeded(_) => "rate",
            AppError::Network(_) => "network",
            AppError::InvalidState(_) => "invalid",
            _ => panic!("Unexpected category"),
        };
        assert_eq!(actual, expected);
    }
}

#[tokio::test]
async fn http_server_failure_retries_with_a_fresh_prompt_cache_and_same_input() {
    let server = sequence_server(vec![
        ResponseTemplate::new(500).insert_header("Retry-After", "0"),
        sse_response(json!({"content":"Recovered"})),
    ])
    .await;
    let client = LlamaCppLlm::new(&settings(server.uri())).unwrap();
    let response = client
        .complete(&CompletionRequest {
            no_time_limit: true,
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(response.text, "Recovered");
    let requests = server.received_requests().await.unwrap();
    let mut original: Value = serde_json::from_slice(&requests[0].body).unwrap();
    original["cache_prompt"] = json!(false);
    assert_eq!(
        original,
        serde_json::from_slice::<Value>(&requests[1].body).unwrap()
    );
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
async fn authentication_and_invalid_tools_are_not_retried() {
    let server = sequence_server(vec![ResponseTemplate::new(401)]).await;
    let client = LlamaCppLlm::new(&settings(server.uri())).unwrap();
    assert!(client
        .complete(&CompletionRequest::default())
        .await
        .is_err());

    // A malformed call is the model's to fix next round, not a request to
    // repeat: it comes back once, marked, with no retry.
    let server = sequence_server(vec![sse_response(
        json!({"tool_calls":[{"id":"x","type":"function","function":{"name":"search","arguments":"[1]"}}]}),
    )])
    .await;
    let client = LlamaCppLlm::new(&settings(server.uri())).unwrap();
    let response = client
        .complete(&CompletionRequest::default())
        .await
        .unwrap();
    assert!(matches!(&response.tool_calls[..],
        [CompletionInput::ToolCall { arguments, .. }] if invalid_tool_call_problem(arguments).is_some()));
    assert_eq!(server.received_requests().await.unwrap().len(), 1);
}

#[tokio::test]
async fn an_output_limit_stop_returns_the_text_once_without_retrying() {
    let server = sequence_server(vec![ResponseTemplate::new(200)
        .set_body_string(stream_reply(json!({"content":"unfinished"}), "length"))])
    .await;
    let client = LlamaCppLlm::new(&settings(server.uri())).unwrap();
    let response = client
        .complete(&CompletionRequest::default())
        .await
        .unwrap();
    assert_eq!(response.text, "unfinished");
    assert_eq!(response.finish_reason, "length");
    assert_eq!(server.received_requests().await.unwrap().len(), 1);

    // Spent it all thinking: nothing to keep, and a retry would do the same.
    let server = sequence_server(vec![ResponseTemplate::new(200)
        .set_body_string(stream_reply(json!({"reasoning_content":"hmm"}), "length"))])
    .await;
    let client = LlamaCppLlm::new(&settings(server.uri())).unwrap();
    assert!(client
        .complete(&CompletionRequest::default())
        .await
        .is_err());
    assert_eq!(server.received_requests().await.unwrap().len(), 1);
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
fn done_marker_without_finish_reason_is_rejected_and_duplicate_tool_ids_are_split() {
    let mut decoder = streaming::Decoder::for_completion();
    decoder.push(b"data: [DONE]\n\n").unwrap();
    assert!(decoder.into_response().is_err());
    // Two calls sharing an id would have their results confused; the second
    // is given its own, and the replayed message says so too.
    let call = json!({"id":"same","type":"function","function":{"name":"search","arguments":"{}"}});
    let response = parse_completion(json!({"choices":[{"message":{"tool_calls":[call.clone(), call]},"finish_reason":"tool_calls"}]})).unwrap();
    let ids: Vec<_> = assistant(&response)["tool_calls"]
        .as_array()
        .unwrap()
        .iter()
        .map(|call| call["id"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(ids, ["same", "call_invalid_1"]);
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

#[test]
fn a_logprobs_request_reads_the_first_tokens_alternatives_back() {
    let client = LlamaCppLlm::new(&settings("http://127.0.0.1:1".into())).unwrap();
    let request = CompletionRequest {
        input: vec![CompletionInput::Message {
            role: "user".into(),
            content: "Answer in one word.".into(),
        }],
        want_logprobs: true,
        ..Default::default()
    };
    let body = client.body(&request).unwrap();
    assert_eq!(body["logprobs"], true);
    assert!(body["top_logprobs"].as_u64().unwrap() > 1);
    let plain = client
        .body(&CompletionRequest {
            want_logprobs: false,
            ..request.clone()
        })
        .unwrap();
    assert!(plain.get("logprobs").is_none());

    // Streamed: the first chunk carries the first token's alternatives; later
    // chunks' logprobs must not replace them.
    let events = [
        json!({"choices":[{"delta":{"content":"supported"},"logprobs":{"content":[{"token":"supported","logprob":-0.2,"top_logprobs":[{"token":"supported","logprob":-0.2},{"token":"uns","logprob":-1.9}]}]}}]}),
        json!({"choices":[{"delta":{"content":"."},"logprobs":{"content":[{"token":".","logprob":-0.01,"top_logprobs":[]}]}}]}),
        json!({"choices":[{"delta":{},"finish_reason":"stop"}]}),
    ];
    let mut decoder = streaming::Decoder::for_completion();
    for event in events {
        decoder
            .push(format!("data: {event}\n\n").as_bytes())
            .unwrap();
    }
    decoder.push(b"data: [DONE]\n\n").unwrap();
    let response = decoder.into_response().unwrap();
    assert_eq!(response.text, "supported.");
    let alternatives = response.first_token_logprobs.unwrap();
    assert_eq!(alternatives[0], ("supported".to_string(), -0.2));
    assert_eq!(alternatives[1], ("uns".to_string(), -1.9));

    // Not asked for: nothing reported.
    let parsed = parse_completion(reply()).unwrap();
    assert!(parsed.first_token_logprobs.is_none());
}

#[test]
fn calls_made_in_one_round_replay_as_one_assistant_message() {
    let call = |id: &str| CompletionInput::ToolCall {
        id: id.into(),
        name: "search".into(),
        arguments: json!({"q": id}),
    };
    let result = |id: &str| CompletionInput::ToolResult {
        id: id.into(),
        output: "found".into(),
    };
    let messages = chat_messages(&[
        CompletionInput::Message {
            role: "user".into(),
            content: "q".into(),
        },
        call("a"),
        call("b"),
        result("a"),
        result("b"),
        call("c"),
    ])
    .unwrap();
    let roles: Vec<_> = messages
        .iter()
        .map(|m| m["role"].as_str().unwrap())
        .collect();
    assert_eq!(roles, ["user", "assistant", "tool", "tool", "assistant"]);
    assert_eq!(messages[1]["tool_calls"].as_array().unwrap().len(), 2);
    assert_eq!(messages[4]["tool_calls"].as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn interactive_completion_is_not_cut_off_by_a_request_time_budget() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(
            sse_response(json!({"content":"Completed"})).set_delay(Duration::from_millis(150)),
        )
        .expect(1)
        .mount(&server)
        .await;
    let client = LlamaCppLlm::new(&settings(server.uri())).unwrap();
    let result = client
        .complete(&CompletionRequest {
            time_budget: Some(Duration::from_millis(10)),
            no_time_limit: true,
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(result.text, "Completed");
    assert!(
        !serde_json::from_slice::<Value>(&server.received_requests().await.unwrap()[0].body)
            .unwrap()
            .to_string()
            .contains("no_time_limit")
    );
}

#[test]
fn internal_requests_do_not_disable_model_reasoning() {
    let client = LlamaCppLlm::new(&settings("http://localhost:8080".into())).unwrap();
    for effort in [None, Some("none")] {
        let request = CompletionRequest {
            reasoning_effort: effort.map(str::to_owned),
            ..Default::default()
        };
        let body = client.body(&request).unwrap();
        assert!(body.get("reasoning_effort").is_none());
        assert!(body.get("chat_template_kwargs").is_none());
    }
}

#[test]
fn reasoning_probabilities_are_never_treated_as_verdict_confidence() {
    let probability = |word: &str| json!({"content":[{"token":word,"logprob":-0.1,"top_logprobs":[{"token":word,"logprob":-0.1}]}]});
    let mut decoder = streaming::Decoder::for_completion();
    for frame in [
        json!({"choices":[{"delta":{"reasoning_content":"contradicted"},"logprobs":probability("contradicted")}]}),
        json!({"choices":[{"delta":{"content":"supported"},"logprobs":probability("supported"),"finish_reason":"stop"}]}),
    ] {
        decoder
            .push(format!("data: {frame}\n\n").as_bytes())
            .unwrap();
    }
    decoder.push(b"data: [DONE]\n\n").unwrap();
    let response = decoder.into_response().unwrap();
    assert_eq!(response.text, "supported");
    assert_eq!(response.first_token_logprobs.unwrap()[0].0, "supported");
    let combined = parse_completion(json!({"choices":[{"message":{"role":"assistant","content":"supported","reasoning_content":"contradicted"},"logprobs":probability("contradicted"),"finish_reason":"stop"}]})).unwrap();
    assert!(combined.first_token_logprobs.is_none());
}

#[test]
fn a_scheduled_request_carries_its_slot_and_reuses_the_cached_prefix() {
    let client = LlamaCppLlm::new(&settings("http://localhost:8080".into())).unwrap();
    let request = CompletionRequest {
        input: vec![CompletionInput::Message {
            role: "user".into(),
            content: "Hello".into(),
        }],
        assigned_slot: Some(1),
        ..Default::default()
    };
    let body = client.body(&request).unwrap();
    assert_eq!(body["id_slot"], 1);
    assert_eq!(body["cache_prompt"], true);

    let unscheduled = client
        .body(&CompletionRequest {
            assigned_slot: None,
            ..request
        })
        .unwrap();
    assert!(unscheduled.get("id_slot").is_none());
}

#[tokio::test]
async fn a_remote_server_is_sized_from_its_props_and_counts_tokens_exactly() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/props"))
        .respond_with(ResponseTemplate::new(200).set_body_json(
            json!({"total_slots": 3, "default_generation_settings": {"n_ctx": 32768}}),
        ))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/tokenize"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"tokens": [1, 2, 3, 4]})))
        .mount(&server)
        .await;

    let llm = LlamaCppLlm::new(&settings(server.uri()))
        .unwrap()
        .schedule()
        .await;
    assert!(llm.counts_tokens_exactly());
    assert_eq!(llm.count_tokens_exact("four tokens here").await.unwrap(), 4);
}

/// The config both a bundled and a remote server can be described by.
fn server_config(prefill_guard: bool) -> ServerConfig {
    let settings = settings("http://localhost:8080".into());
    ServerConfig {
        model: "test-model".into(),
        generation: GenerationConfig {
            temperature: settings.temperature,
            top_p: settings.top_p,
            top_k: settings.top_k,
            max_tokens: settings.max_tokens as usize,
            repeat_penalty: settings.repeat_penalty,
        },
        context_window: settings.context_window as usize,
        stall_timeout: Duration::from_secs(30),
        supports_tools: true,
        prefill_guard,
    }
}

/// The bundled sidecar and a server the user runs differ in where the server
/// is, how it authenticates and who owns its process — never in what they ask
/// of it.
#[tokio::test]
async fn the_bundled_sidecar_and_a_remote_server_send_identical_requests() {
    let remote_server = MockServer::start().await;
    let sidecar_server = MockServer::start().await;
    for (server, auth) in [
        (&remote_server, "Basic test-credential"),
        (&sidecar_server, "Bearer sidecar-token"),
    ] {
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .and(header("authorization", auth))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_string(stream_reply(json!({"content":"Answer"}), "stop")),
            )
            .expect(1)
            .mount(server)
            .await;
    }
    let remote = LlamaCppLlm::new(&settings(remote_server.uri())).unwrap();
    let sidecar =
        LlamaCppLlm::connect(&sidecar_server.uri(), "sidecar-token", server_config(true)).unwrap();
    let request = CompletionRequest {
        input: vec![
            CompletionInput::Message {
                role: "system".into(),
                content: "Answer from the passages.".into(),
            },
            CompletionInput::Message {
                role: "user".into(),
                content: "Which passage says so? 世界".into(),
            },
            CompletionInput::ToolCall {
                id: "call_1".into(),
                name: "search".into(),
                arguments: json!({"q": "rust"}),
            },
            CompletionInput::ToolResult {
                id: "call_1".into(),
                output: "Found Rust docs".into(),
            },
        ],
        tools: vec![crate::application::ports::ToolDefinition {
            name: "search".into(),
            description: "Search".into(),
            parameters: json!({"type":"object"}),
        }],
        json_schema: Some(json!({"type":"object"})),
        reasoning_effort: Some("low".into()),
        sampling: Some(crate::application::ports::llm_port::SamplingOverride::deterministic()),
        max_output_tokens: Some(512),
        want_logprobs: true,
        assigned_slot: Some(2),
        ..Default::default()
    };
    assert_eq!(remote.complete(&request).await.unwrap().text, "Answer");
    assert_eq!(sidecar.complete(&request).await.unwrap().text, "Answer");
    let sent = |requests: Vec<wiremock::Request>| -> Value {
        serde_json::from_slice(&requests[0].body).unwrap()
    };
    let remote_body = sent(remote_server.received_requests().await.unwrap());
    let sidecar_body = sent(sidecar_server.received_requests().await.unwrap());
    assert_eq!(remote_body, sidecar_body);
    assert_eq!(remote_body["id_slot"], 2);
    assert_eq!(
        remote_body["chat_template_kwargs"]["reasoning_effort"],
        "low"
    );
    assert_eq!(remote_body["max_tokens"], 512);
}

#[test]
fn a_model_without_a_tool_template_refuses_tool_traffic() {
    let config = ServerConfig {
        supports_tools: false,
        ..server_config(true)
    };
    let client = LlamaCppLlm::connect("http://127.0.0.1:9", "token", config).unwrap();
    let plain = CompletionRequest {
        input: vec![CompletionInput::Message {
            role: "user".into(),
            content: "Hi".into(),
        }],
        ..Default::default()
    };
    assert!(client.body(&plain).is_ok());
    let replayed_result = CompletionRequest {
        input: vec![CompletionInput::ToolResult {
            id: "call_1".into(),
            output: "Result".into(),
        }],
        ..Default::default()
    };
    assert!(matches!(
        client.body(&replayed_result),
        Err(AppError::InvalidConfig(_))
    ));
    assert!(!client.supports_tool_calling());
}

#[test]
fn ownership_names_the_provider_and_keeps_a_remote_server_alive() {
    let remote = LlamaCppLlm::new(&settings("http://localhost:8080".into())).unwrap();
    assert_eq!(remote.provider_name(), "llamacpp");
    assert!(remote.is_alive());
    assert!(!remote.config.prefill_guard);
}

const PROGRESS_FRAME: &str = "data: {\"choices\":[{\"finish_reason\":null,\"index\":0,\"delta\":{\"role\":\"assistant\",\"content\":null}}],\"prompt_progress\":{\"total\":4000,\"cache\":0,\"processed\":2048,\"time_ms\":1}}\n\n";

/// Frames delivered at virtual times, for driving the stream clocks on a
/// paused runtime.
fn scripted(
    frames: Vec<(u64, String)>,
) -> impl futures::Stream<Item = std::result::Result<Vec<u8>, std::io::Error>> + Unpin {
    Box::pin(futures::stream::unfold(
        frames.into_iter(),
        |mut frames| async move {
            let (delay, frame) = frames.next()?;
            tokio::time::sleep(Duration::from_secs(delay)).await;
            Some((Ok(frame.into_bytes()), frames))
        },
    ))
}

fn io_error(_: std::io::Error) -> AppError {
    AppError::Network("fixture stream failed".into())
}

async fn read(
    frames: Vec<(u64, String)>,
    first_frame: Option<Duration>,
) -> std::result::Result<CompletionResponse, retry::Failure> {
    retry::read_stream(
        scripted(frames),
        retry::Patience {
            first_frame,
            stall: Duration::from_secs(30),
        },
        io_error,
        &|_| Ok(()),
        None,
        &mut false,
    )
    .await
}

/// Ninety seconds of prompt processing, reported every 45, then the answer.
fn slow_prefill() -> Vec<(u64, String)> {
    let answer = "data: {\"choices\":[{\"index\":0,\"delta\":{\"content\":\"Answer\"},\"finish_reason\":null}]}\n\n";
    let finish = "data: {\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"stop\"}]}\n\ndata: [DONE]\n\n";
    vec![
        (45, PROGRESS_FRAME.to_string()),
        (45, PROGRESS_FRAME.to_string()),
        (10, answer.to_string()),
        (1, finish.to_string()),
    ]
}

/// A flat first-token wait failed this turn; progress frames are liveness.
#[tokio::test(start_paused = true)]
async fn a_bundled_servers_prompt_progress_keeps_its_first_frame_wait_alive() {
    let allowance = retry::prefill_allowance(&[json!({"role":"user","content":"hi"})]);
    assert_eq!(
        allowance,
        retry::FIRST_FRAME_FLOOR,
        "a short prompt gets the floor"
    );
    let response = read(slow_prefill(), Some(allowance)).await.ok().unwrap();
    assert_eq!(response.text, "Answer");
}

/// Every request a bundled server sees went through its scheduler, so a
/// server that says nothing past the allowance has hung: the attempt fails
/// for good rather than queueing a retry behind it.
#[tokio::test(start_paused = true)]
async fn a_silent_bundled_server_fails_without_a_retry_but_a_remote_one_is_waited_for() {
    let answer = "data: {\"choices\":[{\"index\":0,\"delta\":{\"content\":\"Late\"},\"finish_reason\":\"stop\"}]}\n\n";
    let frames = vec![(70, answer.to_string())];
    let bundled = read(frames.clone(), Some(Duration::from_secs(60))).await;
    assert!(matches!(
        bundled,
        Err(retry::Failure::Permanent(AppError::ServiceNotAvailable(_)))
    ));
    let remote = read(frames, None).await.ok().unwrap();
    assert_eq!(remote.text, "Late");
}

/// Once the model is generating, silence is a stall, and a stall is retried.
#[tokio::test(start_paused = true)]
async fn silence_after_generation_starts_is_still_a_stall() {
    let answer = "data: {\"choices\":[{\"index\":0,\"delta\":{\"content\":\"A\"},\"finish_reason\":null}]}\n\n";
    let frames = vec![(1, answer.to_string()), (45, answer.to_string())];
    let result = read(frames, Some(Duration::from_secs(60))).await;
    assert!(matches!(result, Err(retry::Failure::Retry(..))));
}

/// A long prompt with no progress frames gets one batch at the floor rate.
#[test]
fn a_long_prompt_gets_a_batch_of_prefill_on_top_of_the_floor() {
    let long = json!({"role":"user","content":"x".repeat(40_000)});
    assert_eq!(
        retry::prefill_allowance(&[long]),
        retry::FIRST_FRAME_FLOOR
            + Duration::from_secs(
                (retry::PREFILL_BATCH_TOKENS / retry::PREFILL_FLOOR_TOKENS_PER_SEC) as u64
            )
    );
}

#[test]
fn a_full_window_keeps_the_output_floor_but_a_smaller_cap_is_kept() {
    let client = LlamaCppLlm::connect(
        "http://127.0.0.1:9",
        "token",
        ServerConfig {
            context_window: 8_192,
            ..server_config(true)
        },
    )
    .unwrap();
    let prompt = |chars: usize| vec![json!({"role":"user","content":"x".repeat(chars)})];
    // A prompt that leaves the window nearly full still gets the floor.
    assert_eq!(client.output_room_for(&prompt(30_000), 4_096), 1_024);
    // A small prompt gets what it asked for, up to the room left.
    assert_eq!(client.output_room_for(&prompt(100), 4_096), 4_096);
    // A cap below the floor is the caller's decision and is kept.
    assert_eq!(client.output_room_for(&prompt(100), 512), 512);
    assert_eq!(client.output_room_for(&prompt(30_000), 512), 512);
}
