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
    assert_eq!(plain["tool_choice"]["type"], "any");
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
