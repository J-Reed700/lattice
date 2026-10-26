use super::*;
#[test]
fn typed_tool_calls_keep_ids_and_usage() {
    let result = parse_completion(LLMProvider::Openai, json!({"status":"completed","output":[{"type":"function_call","call_id":"call_123","name":"search","arguments":"{\"q\":\"rust\"}"}],"usage":{"input_tokens":12,"output_tokens":3}})).unwrap();
    assert!(
        matches!(&result.tool_calls[0], CompletionInput::ToolCall { id, .. } if id == "call_123")
    );
    assert_eq!(result.input_tokens, 12);
}

#[test]
fn openai_reasoning_summary_is_kept_separate_from_answer_text() {
    let result = parse_completion(
        LLMProvider::Openai,
        json!({
            "status":"completed",
            "output":[
                {"type":"reasoning","summary":[{"type":"summary_text","text":"Compared the available evidence."}]},
                {"type":"message","content":[{"type":"output_text","text":"The answer."}]}
            ]
        }),
    )
    .unwrap();
    assert_eq!(result.text, "The answer.");
    assert_eq!(
        result.reasoning.as_deref(),
        Some("Compared the available evidence.")
    );
}
#[test]
fn provider_wire_formats_preserve_tool_results() {
    let mut settings = LLMSettingsDto::default();
    let request = CompletionRequest {
        input: vec![CompletionInput::ToolResult {
            id: "abc".into(),
            output: "found".into(),
        }],
        ..Default::default()
    };
    settings.provider = LLMProvider::Openai;
    let openai = CloudLlm::new(&settings, "test".into()).unwrap();
    assert_eq!(
        openai.body(&request, false).unwrap()["input"][0]["call_id"],
        "abc"
    );
    settings.provider = LLMProvider::Anthropic;
    let anthropic = CloudLlm::new(&settings, "test".into()).unwrap();
    assert_eq!(
        anthropic.body(&request, false).unwrap()["messages"][0]["content"][0]["tool_use_id"],
        "abc"
    );
    settings.provider = LLMProvider::Google;
    let google = CloudLlm::new(&settings, "test".into()).unwrap();
    assert_eq!(
        google.body(&request, false).unwrap()["messages"][0]["tool_call_id"],
        "abc"
    );
}

/// A turn is replayed by appending the response's own replay items: the
/// next request then answers calls the provider recognises, signed
/// thinking included, with no knowledge of which provider it is.
#[test]
fn each_provider_replays_its_own_turn_ahead_of_the_tool_results() {
    let continue_with = |llm: &CloudLlm, response: CompletionResponse| {
        let mut input = vec![CompletionInput::Message {
            role: "user".into(),
            content: "find".into(),
        }];
        input.extend(response.replay);
        input.push(CompletionInput::ToolResult {
            id: "call_7".into(),
            output: "found".into(),
        });
        llm.body(
            &CompletionRequest {
                input,
                ..Default::default()
            },
            false,
        )
        .unwrap()
    };

    let mut settings = LLMSettingsDto {
        provider: LLMProvider::Openai,
        ..Default::default()
    };
    let openai = CloudLlm::new(&settings, "test".into()).unwrap();
    let reply = parse_completion(
        LLMProvider::Openai,
        json!({"status":"completed","output":[
            {"type":"reasoning","encrypted_content":"opaque","summary":[]},
            {"type":"function_call","call_id":"call_7","name":"search","arguments":"{}"}
        ]}),
    )
    .unwrap();
    let body = continue_with(&openai, reply);
    assert_eq!(body["input"][1]["encrypted_content"], "opaque");
    assert_eq!(body["input"][2]["call_id"], "call_7");
    assert_eq!(body["input"][3]["type"], "function_call_output");

    settings.provider = LLMProvider::Anthropic;
    let anthropic = CloudLlm::new(&settings, "test".into()).unwrap();
    let reply = parse_completion(
        LLMProvider::Anthropic,
        json!({"stop_reason":"tool_use","content":[
            {"type":"thinking","thinking":"Search first.","signature":"signed"},
            {"type":"tool_use","id":"call_7","name":"search","input":{}}
        ]}),
    )
    .unwrap();
    let body = continue_with(&anthropic, reply);
    assert_eq!(body["messages"][1]["role"], "assistant");
    assert_eq!(body["messages"][1]["content"][0]["signature"], "signed");
    assert_eq!(body["messages"][1]["content"][1]["id"], "call_7");
    assert_eq!(body["messages"][2]["content"][0]["tool_use_id"], "call_7");

    settings.provider = LLMProvider::Google;
    let google = CloudLlm::new(&settings, "test".into()).unwrap();
    let reply = parse_completion(
        LLMProvider::Google,
        json!({
            "choices": [{
                "message": {
                    "role": "assistant",
                    "content": null,
                    "tool_calls": [{
                        "id": "call_7",
                        "type": "function",
                        "function": {
                            "name": "search",
                            "arguments": "{}"
                        }
                    }]
                },
                "finish_reason": "tool_calls"
            }],
            "usage": {"prompt_tokens": 10, "completion_tokens": 5}
        }),
    )
    .unwrap();
    let body = continue_with(&google, reply);
    assert_eq!(body["messages"][1]["role"], "assistant");
    assert_eq!(body["messages"][1]["tool_calls"][0]["id"], "call_7");
    assert_eq!(body["messages"][2]["role"], "tool");
    assert_eq!(body["messages"][2]["tool_call_id"], "call_7");
}

fn anthropic_body(effort: &str) -> Value {
    let settings = LLMSettingsDto {
        provider: LLMProvider::Anthropic,
        ..Default::default()
    };
    let anthropic = CloudLlm::new(&settings, "test".into()).unwrap();
    anthropic
        .body(
            &CompletionRequest {
                reasoning_effort: Some(effort.into()),
                ..Default::default()
            },
            false,
        )
        .unwrap()
}

/// A `thinking` block is what switches extended thinking on, so asking for
/// no reasoning must not send one.
#[test]
fn anthropic_omits_the_thinking_block_when_no_reasoning_is_asked_for() {
    let body = anthropic_body("none");
    assert!(body.get("thinking").is_none(), "{body}");
    assert_eq!(body["output_config"]["effort"], "low");
}

/// Requests that share a prompt prefix (a cache key) ask Claude to cache it;
/// a one-off request does not pay for a cache write it will never read.
#[test]
fn anthropic_caches_only_requests_that_share_a_prefix() {
    let settings = LLMSettingsDto {
        provider: LLMProvider::Anthropic,
        ..Default::default()
    };
    let anthropic = CloudLlm::new(&settings, "test".into()).unwrap();
    let body = |cache_key: Option<&str>| {
        anthropic
            .body(
                &CompletionRequest {
                    cache_key: cache_key.map(str::to_owned),
                    ..Default::default()
                },
                false,
            )
            .unwrap()
    };
    assert_eq!(
        body(Some("conversation-1"))["cache_control"]["type"],
        "ephemeral"
    );
    assert!(body(None).get("cache_control").is_none());
}

/// Current Claude models 400 on any sampling knob, so a caller's determinism
/// override must not reach the request.
#[test]
fn anthropic_never_sends_sampling_knobs() {
    let settings = LLMSettingsDto {
        provider: LLMProvider::Anthropic,
        ..Default::default()
    };
    let anthropic = CloudLlm::new(&settings, "test".into()).unwrap();
    let body = anthropic
        .body(
            &CompletionRequest {
                sampling: Some(
                    lattice_core::application::ports::llm_port::SamplingOverride {
                        temperature: Some(0.0),
                        top_p: Some(1.0),
                        top_k: Some(1),
                        ..Default::default()
                    },
                ),
                ..Default::default()
            },
            false,
        )
        .unwrap();
    for knob in ["temperature", "top_p", "top_k"] {
        assert!(body.get(knob).is_none(), "{knob} sent: {body}");
    }
}

/// Claude shows no thinking unless asked, so a caller that displays reasoning
/// asks for summaries.
#[test]
fn anthropic_asks_for_thinking_summaries_only_when_reasoning_is_shown() {
    let settings = LLMSettingsDto {
        provider: LLMProvider::Anthropic,
        ..Default::default()
    };
    let anthropic = CloudLlm::new(&settings, "test".into()).unwrap();
    let body = |include_reasoning| {
        anthropic
            .body(
                &CompletionRequest {
                    thinking: ThinkingMode::On,
                    include_reasoning,
                    ..Default::default()
                },
                false,
            )
            .unwrap()
    };
    assert_eq!(body(true)["thinking"]["display"], "summarized");
    assert!(body(false)["thinking"].get("display").is_none());
}

#[test]
fn anthropic_still_enables_thinking_for_a_real_effort() {
    let body = anthropic_body("medium");
    assert_eq!(body["thinking"]["type"], "adaptive");
    assert_eq!(body["output_config"]["effort"], "medium");
}

#[test]
fn anthropic_thinks_when_asked_and_maps_the_tool_choice() {
    let settings = LLMSettingsDto {
        provider: LLMProvider::Anthropic,
        ..Default::default()
    };
    let anthropic = CloudLlm::new(&settings, "test".into()).unwrap();
    let tool = lattice_core::application::ports::ToolDefinition {
        name: "search".into(),
        description: "Search".into(),
        parameters: json!({"type":"object"}),
    };
    let body = |thinking, choice| {
        anthropic
            .body(
                &CompletionRequest {
                    tools: vec![tool.clone()],
                    thinking,
                    tool_choice: Some(choice),
                    ..Default::default()
                },
                false,
            )
            .unwrap()
    };
    let plain = body(ThinkingMode::Off, ToolChoice::Required);
    assert!(plain.get("thinking").is_none());
    assert_eq!(
        plain["tool_choice"]["type"], "auto",
        "the newest models refuse a forced call even without thinking"
    );
    assert_eq!(
        body(ThinkingMode::Off, ToolChoice::None)["tool_choice"]["type"],
        "none"
    );
    let thinking = body(ThinkingMode::On, ToolChoice::Required);
    assert_eq!(thinking["thinking"]["type"], "adaptive");
    assert_eq!(
        thinking["tool_choice"]["type"], "auto",
        "a forced call is refused while thinking"
    );
}

#[test]
fn openai_sends_the_tool_choice_with_tools() {
    let settings = LLMSettingsDto {
        provider: LLMProvider::Openai,
        model: "gpt-5.1".into(),
        ..Default::default()
    };
    let openai = CloudLlm::new(&settings, "test".into()).unwrap();
    let request = CompletionRequest {
        tools: vec![lattice_core::application::ports::ToolDefinition {
            name: "search".into(),
            description: "Search".into(),
            parameters: json!({"type":"object"}),
        }],
        tool_choice: Some(ToolChoice::None),
        ..Default::default()
    };
    assert_eq!(openai.body(&request, false).unwrap()["tool_choice"], "none");
    let bare = CompletionRequest {
        tools: Vec::new(),
        ..request
    };
    assert!(openai
        .body(&bare, false)
        .unwrap()
        .get("tool_choice")
        .is_none());
}

fn openai_body(model: &str, effort: Option<&str>) -> Value {
    let settings = LLMSettingsDto {
        provider: LLMProvider::Openai,
        model: model.into(),
        ..Default::default()
    };
    let openai = CloudLlm::new(&settings, "test".into()).unwrap();
    openai
        .body(
            &CompletionRequest {
                reasoning_effort: effort.map(str::to_owned),
                ..Default::default()
            },
            false,
        )
        .unwrap()
}

/// A `reasoning` block on a model without reasoning controls is a 400, and
/// the retrieval callers discard errors — so the field must never be sent
/// speculatively.
#[test]
fn openai_sends_no_reasoning_block_to_a_model_that_has_no_reasoning() {
    let body = openai_body("gpt-4o-mini", Some("none"));
    assert!(body.get("reasoning").is_none(), "{body}");
}

#[test]
fn openai_clamps_no_reasoning_to_each_family_floor() {
    assert_eq!(
        openai_body("gpt-5.1", Some("none"))["reasoning"]["effort"],
        "minimal"
    );
    assert_eq!(
        openai_body("o3", Some("none"))["reasoning"]["effort"],
        "low"
    );
}

#[test]
fn openai_forwards_a_real_effort_unchanged() {
    assert_eq!(
        openai_body("gpt-5.1", Some("high"))["reasoning"]["effort"],
        "high"
    );
    assert!(openai_body("gpt-5.1", None).get("reasoning").is_none());
}

#[test]
fn openai_requests_a_displayable_summary_only_for_reasoning_models() {
    let body = |model: &str| {
        let settings = LLMSettingsDto {
            provider: LLMProvider::Openai,
            model: model.into(),
            ..Default::default()
        };
        CloudLlm::new(&settings, "test".into())
            .unwrap()
            .body(
                &CompletionRequest {
                    include_reasoning: true,
                    ..Default::default()
                },
                false,
            )
            .unwrap()
    };
    assert_eq!(body("gpt-5.1")["reasoning"]["summary"], "auto");
    assert_eq!(body("gpt-6-sol")["reasoning"]["summary"], "auto");
    assert!(body("gpt-4o-mini").get("reasoning").is_none());
}

#[test]
fn google_completion_parses_content_reasoning_and_tool_calls() {
    let result = parse_completion(
        LLMProvider::Google,
        json!({
            "choices": [{
                "message": {
                    "role": "assistant",
                    "content": "The answer.",
                    "reasoning_content": "Thought step.",
                    "tool_calls": [{
                        "id": "call_123",
                        "type": "function",
                        "function": {
                            "name": "search",
                            "arguments": "{\"q\":\"rust\"}"
                        }
                    }]
                },
                "finish_reason": "stop"
            }],
            "usage": {
                "prompt_tokens": 15,
                "completion_tokens": 8
            }
        }),
    )
    .unwrap();
    assert_eq!(result.text, "The answer.");
    assert_eq!(result.reasoning.as_deref(), Some("Thought step."));
    assert_eq!(result.finish_reason, "tool_calls");
    assert_eq!(result.input_tokens, 15);
    assert_eq!(result.output_tokens, 8);
    assert!(
        matches!(&result.tool_calls[0], CompletionInput::ToolCall { id, name, arguments } if id == "call_123" && name == "search" && arguments["q"] == "rust")
    );
}

#[test]
fn google_sends_the_tool_choice_with_tools() {
    let settings = LLMSettingsDto {
        provider: LLMProvider::Google,
        model: "gemini-2.5-flash".into(),
        ..Default::default()
    };
    let google = CloudLlm::new(&settings, "test".into()).unwrap();
    let request = CompletionRequest {
        tools: vec![lattice_core::application::ports::ToolDefinition {
            name: "search".into(),
            description: "Search".into(),
            parameters: json!({"type":"object"}),
        }],
        tool_choice: Some(ToolChoice::None),
        ..Default::default()
    };
    assert_eq!(google.body(&request, false).unwrap()["tool_choice"], "none");
    let bare = CompletionRequest {
        tools: Vec::new(),
        ..request
    };
    assert!(google
        .body(&bare, false)
        .unwrap()
        .get("tool_choice")
        .is_none());
}

#[test]
fn each_provider_that_takes_stop_sequences_is_sent_them() {
    let request = CompletionRequest {
        input: vec![CompletionInput::Message {
            role: "user".into(),
            content: "Read them.".into(),
        }],
        stop: vec!["[5]".into(), "\n5.".into()],
        ..Default::default()
    };
    let bare = CompletionRequest {
        stop: Vec::new(),
        ..request.clone()
    };
    for (provider, field) in [
        (LLMProvider::Anthropic, "stop_sequences"),
        (LLMProvider::Google, "stop"),
    ] {
        let settings = LLMSettingsDto {
            provider,
            model: "model".into(),
            ..Default::default()
        };
        let cloud = CloudLlm::new(&settings, "test".into()).unwrap();
        assert_eq!(
            cloud.body(&request, false).unwrap()[field],
            json!(["[5]", "\n5."]),
            "{provider:?}"
        );
        assert!(
            cloud.body(&bare, false).unwrap().get(field).is_none(),
            "{provider:?}"
        );
    }
    let settings = LLMSettingsDto {
        provider: LLMProvider::Openai,
        model: "gpt-5.1".into(),
        ..Default::default()
    };
    let openai = CloudLlm::new(&settings, "test".into()).unwrap();
    let body = openai.body(&request, false).unwrap();
    assert!(
        body.get("stop").is_none() && body.get("stop_sequences").is_none(),
        "the Responses API takes no stop sequences"
    );
}

fn picture_request() -> CompletionRequest {
    use lattice_core::application::ports::llm_port::{ContentPart, ImagePart};
    CompletionRequest {
        input: vec![CompletionInput::Multimodal {
            role: "user".into(),
            parts: vec![
                ContentPart::Image(ImagePart {
                    mime: "image/png".into(),
                    data_base64: "iVBORw0KGgo=".into(),
                    width: 512,
                    height: 512,
                    tokens: 255,
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

fn seeing(provider: LLMProvider) -> CloudLlm {
    use lattice_core::domain::models::vision::VisionPreset;
    let settings = LLMSettingsDto {
        provider,
        model: "vision-model".into(),
        ..Default::default()
    };
    CloudLlm::new(&settings, "test".into())
        .unwrap()
        .with_vision(Some(VisionPreset::OpenAi.geometry()))
}

#[test]
fn each_provider_receives_a_picture_in_its_own_format_before_the_text() {
    let request = picture_request();
    let openai = seeing(LLMProvider::Openai).body(&request, false).unwrap();
    assert_eq!(
        openai["input"][0],
        json!({"role":"user","content":[
            {"type":"input_image","image_url":"data:image/png;base64,iVBORw0KGgo=","detail":"auto"},
            {"type":"input_text","text":"What does it show?"}
        ]})
    );
    let anthropic = seeing(LLMProvider::Anthropic)
        .body(&request, false)
        .unwrap();
    assert_eq!(
        anthropic["messages"][0],
        json!({"role":"user","content":[
            {"type":"image","source":{"type":"base64","media_type":"image/png","data":"iVBORw0KGgo="}},
            {"type":"text","text":"What does it show?"}
        ]})
    );
    let google = seeing(LLMProvider::Google).body(&request, false).unwrap();
    assert_eq!(
        google["messages"][0],
        json!({"role":"user","content":[
            {"type":"image_url","image_url":{"url":"data:image/png;base64,iVBORw0KGgo="}},
            {"type":"text","text":"What does it show?"}
        ]})
    );
}

#[test]
fn a_cloud_model_nobody_declared_able_to_see_refuses_pictures() {
    let settings = LLMSettingsDto {
        provider: LLMProvider::Openai,
        model: "gpt-3.5-turbo".into(),
        ..Default::default()
    };
    let refused = CloudLlm::new(&settings, "test".into())
        .unwrap()
        .body(&picture_request(), false)
        .unwrap_err();
    assert!(matches!(refused, AppError::InvalidConfig(_)), "{refused}");
    assert!(refused
        .to_string()
        .contains("gpt-3.5-turbo is not set up to see images"));
}

/// An OpenAI-compatible llama-server's `timings` object is taken as the
/// server reports it; a whole answer without one has no edge between
/// reading and writing and is not timed.
#[test]
fn a_reported_timings_object_times_a_cloud_answer() {
    let response = parse_completion(
        LLMProvider::Google,
        json!({"choices":[{"message":{"role":"assistant","content":"Hi"},"finish_reason":"stop"}],
            "usage":{"prompt_tokens":2060,"completion_tokens":226},
            "timings":{"cache_n":360,"prompt_n":1700,"prompt_ms":2000.0,"predicted_n":226,"predicted_per_second":11.3}}),
    )
    .unwrap();
    let timing = response.timing.unwrap();
    assert_eq!((timing.prompt_tokens, timing.output_tokens), (1700, 226));
    assert!((timing.prompt_seconds - 2.0).abs() < 1e-9);
    let untimed = parse_completion(
        LLMProvider::Openai,
        json!({"status":"completed","output":[],"usage":{"input_tokens":12,"output_tokens":3}}),
    )
    .unwrap();
    assert_eq!(untimed.timing, None);
}

fn stream_of(events: &[Value]) -> Vec<u8> {
    events
        .iter()
        .map(|event| format!("data: {event}\r\n\r\n"))
        .collect::<String>()
        .into_bytes()
}

/// A streamed answer is timed by the app's own clock on its own request:
/// until the first delta the backend was reading, after it writing. The
/// prompt tokens it read leave out those its usage says were cached.
#[tokio::test]
async fn a_streamed_answer_is_timed_from_its_first_delta() {
    let bytes = stream_of(&[
        json!({"type":"response.output_text.delta","delta":"Hello"}),
        json!({"type":"response.completed","response":{"status":"completed",
            "usage":{"input_tokens":2000,"input_tokens_details":{"cached_tokens":300},"output_tokens":226},
            "output":[{"type":"message","content":[{"type":"output_text","text":"Hello"}]}]}}),
    ]);
    let sent = Instant::now().checked_sub(Duration::from_secs(2)).unwrap();
    let response = streaming::drain(
        LLMProvider::Openai,
        futures::stream::iter([Ok::<_, String>(bytes)]),
        Duration::from_secs(1),
        sent,
        &|_| Ok(()),
        &|_| Ok(()),
    )
    .await
    .unwrap();
    let timing = response.timing.unwrap();
    assert_eq!((timing.prompt_tokens, timing.output_tokens), (1700, 226));
    assert!(timing.prompt_seconds >= 2.0, "{timing:?}");
    assert!(timing.output_seconds < timing.prompt_seconds, "{timing:?}");
}

/// A chat-completions stream whose last chunk carries the server's own
/// timings is timed by them, not by the app's clock.
#[tokio::test]
async fn a_streams_own_timings_win_over_the_clock() {
    let bytes = stream_of(&[
        json!({"choices":[{"index":0,"delta":{"content":"Answer"}}],"usage":null}),
        json!({"choices":[{"index":0,"delta":{},"finish_reason":"stop"}],
            "usage":{"prompt_tokens":2060,"completion_tokens":226},
            "timings":{"prompt_n":1700,"prompt_ms":2000.0,"predicted_n":226,"predicted_per_second":11.3}}),
    ]);
    let sent = Instant::now().checked_sub(Duration::from_secs(10)).unwrap();
    let response = streaming::drain(
        LLMProvider::Google,
        futures::stream::iter([Ok::<_, String>(bytes)]),
        Duration::from_secs(1),
        sent,
        &|_| Ok(()),
        &|_| Ok(()),
    )
    .await
    .unwrap();
    let timing = response.timing.unwrap();
    assert_eq!(timing.prompt_tokens, 1700);
    assert!((timing.prompt_seconds - 2.0).abs() < 1e-9, "{timing:?}");
}

/// A provider refuses a prompt and output ceiling that together exceed the
/// model's window rather than writing less, so each is asked for no more than
/// the window has left; a ceiling that fits is sent as asked.
#[test]
fn each_provider_is_asked_for_no_more_than_the_window_has_left() {
    for provider in [
        LLMProvider::Openai,
        LLMProvider::Anthropic,
        LLMProvider::Google,
    ] {
        let settings = LLMSettingsDto {
            provider,
            model: "model".into(),
            max_tokens: 4_096,
            context_window: 3_000,
            ..Default::default()
        };
        let cloud = CloudLlm::new(&settings, "test".into()).unwrap();
        let ceiling = |request: &CompletionRequest| {
            let body = cloud.body(request, false).unwrap();
            match provider {
                LLMProvider::Openai => body["max_output_tokens"].clone(),
                _ => body["max_tokens"].clone(),
            }
        };
        let long = CompletionRequest {
            input: vec![CompletionInput::Message {
                role: "user".into(),
                content: "word ".repeat(1_600),
            }],
            ..Default::default()
        };
        let room = 3_000 - cloud.prompt_tokens(&long);
        assert!(room < 4_096, "{provider:?}: {room}");
        assert_eq!(ceiling(&long), json!(room), "{provider:?}");

        let short = CompletionRequest {
            input: vec![CompletionInput::Message {
                role: "user".into(),
                content: "When did the bridge open?".into(),
            }],
            max_output_tokens: Some(512),
            ..Default::default()
        };
        assert_eq!(ceiling(&short), json!(512), "{provider:?}");
    }
}
