//! The thinking switch, per-mode sampling, tool choice and server timings.
use super::*;
use crate::features::llm::fitting::PromptMeasure;
use lattice_core::application::contracts::settings::ThinkingOverrideDto;
use lattice_core::application::ports::llm_port::{
    SamplingOverride, ServedTiming, ThinkingMode, ThinkingPolicy, ToolChoice, ToolDefinition,
};

/// The body `request` becomes, with no window to cut it to.
fn body_of(client: &LlamaCppLlm, request: &CompletionRequest) -> Result<Value> {
    client.body(&Fitted::uncut(client, request))
}

fn client() -> LlamaCppLlm {
    let settings = LLMSettingsDto {
        llama_cpp: LlamaCppSettingsDto {
            url: "http://localhost:8080".into(),
            model: "test-model".into(),
            ..Default::default()
        },
        ..Default::default()
    };
    LlamaCppLlm::new(&settings).unwrap()
}

/// A bundled server whose model pins `model_thinking`, under the user's
/// `thinking_override`.
fn local_client(
    model_thinking: ThinkingPolicy,
    thinking_override: ThinkingOverrideDto,
) -> LlamaCppLlm {
    let settings = LLMSettingsDto {
        thinking_override,
        ..Default::default()
    };
    let config = ServerConfig {
        model: "laguna".into(),
        generation: GenerationConfig::from_settings(&settings),
        context_window: 8192,
        stall_timeout: Duration::from_secs(30),
        supports_tools: true,
        model_thinking,
        prefill_guard: true,
        vision: None,
    };
    LlamaCppLlm::connect("http://localhost:8080", "token", config).unwrap()
}

fn user(text: &str) -> Vec<CompletionInput> {
    vec![CompletionInput::Message {
        role: "user".into(),
        content: text.into(),
    }]
}

fn search_tool() -> ToolDefinition {
    ToolDefinition {
        name: "search".into(),
        description: "Search".into(),
        parameters: json!({"type":"object"}),
    }
}

fn sampling(body: &Value) -> (f64, f64, i64, f64, f64) {
    (
        body["temperature"].as_f64().unwrap(),
        body["top_p"].as_f64().unwrap(),
        body["top_k"].as_i64().unwrap(),
        body["presence_penalty"].as_f64().unwrap(),
        body["repeat_penalty"].as_f64().unwrap(),
    )
}

fn close(actual: (f64, f64, i64, f64, f64), expected: (f64, f64, i64, f64, f64)) -> bool {
    (actual.0 - expected.0).abs() < 1e-6
        && (actual.1 - expected.1).abs() < 1e-6
        && actual.2 == expected.2
        && (actual.3 - expected.3).abs() < 1e-6
        && (actual.4 - expected.4).abs() < 1e-6
}

/// Templates disagree on the default and llama-server turns a missing switch
/// into "on", so every request says. An effort level such as "low" (which the
/// Qwen 3.5 templates never read) does not stand in for the switch, and no
/// level is sent with thinking off: Qwen 3.8's template rejects "none".
#[test]
fn every_request_switches_thinking_explicitly_and_off_by_default() {
    let client = client();
    for effort in [None, Some("low"), Some("none")] {
        let body = body_of(
            &client,
            &CompletionRequest {
                input: user("Hi"),
                reasoning_effort: effort.map(str::to_owned),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(
            body["chat_template_kwargs"],
            json!({"enable_thinking": false})
        );
        assert!(body.get("reasoning_effort").is_none(), "effort {effort:?}");
    }
}

#[test]
fn a_thinking_request_turns_it_on_and_passes_only_a_real_effort_through() {
    let client = client();
    let thinking = |effort: Option<&str>| {
        body_of(
            &client,
            &CompletionRequest {
                input: user("Why?"),
                thinking: ThinkingMode::On,
                reasoning_effort: effort.map(str::to_owned),
                ..Default::default()
            },
        )
        .unwrap()
    };
    let body = thinking(None);
    assert_eq!(body["chat_template_kwargs"]["enable_thinking"], true);
    assert!(body.get("reasoning_effort").is_none());
    assert_eq!(thinking(Some("high"))["reasoning_effort"], "high");
    assert!(thinking(Some("none")).get("reasoning_effort").is_none());
}

/// llama.cpp #20837: the 9B writes its call inside the thinking block and the
/// tool grammar ends generation there.
#[test]
fn a_round_that_can_call_tools_never_thinks_but_a_withdrawn_one_may() {
    let client = client();
    let request = CompletionRequest {
        input: user("Find it"),
        tools: vec![search_tool()],
        thinking: ThinkingMode::On,
        ..Default::default()
    };
    let body = body_of(&client, &request).unwrap();
    assert_eq!(body["chat_template_kwargs"]["enable_thinking"], false);
    assert!(body.get("reasoning_effort").is_none());
    assert!(body.get("tool_choice").is_none(), "no choice leaves auto");

    let withdrawn = CompletionRequest {
        tool_choice: Some(ToolChoice::None),
        ..request
    };
    let body = body_of(&client, &withdrawn).unwrap();
    assert_eq!(body["chat_template_kwargs"]["enable_thinking"], true);
    assert_eq!(body["tool_choice"], "none");
    assert_eq!(body["tools"][0]["function"]["name"], "search");
    assert_eq!(ThinkingMode::for_answer(true, &withdrawn), ThinkingMode::On);
    assert_eq!(
        ThinkingMode::for_answer(
            true,
            &CompletionRequest {
                tool_choice: Some(ToolChoice::Auto),
                ..withdrawn.clone()
            }
        ),
        ThinkingMode::Off
    );
    assert_eq!(
        ThinkingMode::for_answer(false, &withdrawn),
        ThinkingMode::Off
    );
}

#[test]
fn tool_choice_is_sent_only_alongside_tools() {
    let client = client();
    let required = body_of(
        &client,
        &CompletionRequest {
            input: user("Find it"),
            tools: vec![search_tool()],
            tool_choice: Some(ToolChoice::Required),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(required["tool_choice"], "required");
    let bare = body_of(
        &client,
        &CompletionRequest {
            input: user("Hi"),
            tool_choice: Some(ToolChoice::None),
            ..Default::default()
        },
    )
    .unwrap();
    assert!(bare.get("tool_choice").is_none());
    assert!(bare.get("tools").is_none());
}

#[test]
fn stop_sequences_are_sent_only_when_asked_for() {
    let client = client();
    let stopped = body_of(
        &client,
        &CompletionRequest {
            input: user("Read them"),
            stop: vec!["[5]".into(), "\n5.".into()],
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(stopped["stop"], json!(["[5]", "\n5."]));
    let open = body_of(
        &client,
        &CompletionRequest {
            input: user("Hi"),
            ..Default::default()
        },
    )
    .unwrap();
    assert!(open.get("stop").is_none());
}

/// The Qwen 3.5 card's two profiles: answering directly samples cooler and
/// narrower than thinking does; both keep the presence penalty that stops
/// loops and leave the multiplicative repeat penalty off.
#[test]
fn each_mode_samples_with_its_own_profile_unless_the_caller_overrides() {
    let client = client();
    let answer = body_of(
        &client,
        &CompletionRequest {
            input: user("Hi"),
            ..Default::default()
        },
    )
    .unwrap();
    assert!(
        close(sampling(&answer), (0.7, 0.8, 20, 1.5, 1.0)),
        "{answer}"
    );

    let thinking = body_of(
        &client,
        &CompletionRequest {
            input: user("Why?"),
            thinking: ThinkingMode::On,
            ..Default::default()
        },
    )
    .unwrap();
    assert!(
        close(sampling(&thinking), (1.0, 0.95, 20, 1.5, 1.0)),
        "{thinking}"
    );

    // A verdict stays greedy whichever mode it runs in, and a presence
    // penalty would push it off a label word the prompt just listed.
    for mode in [ThinkingMode::Off, ThinkingMode::On] {
        let greedy = body_of(
            &client,
            &CompletionRequest {
                input: user("Supported or not?"),
                thinking: mode,
                sampling: Some(SamplingOverride::deterministic()),
                ..Default::default()
            },
        )
        .unwrap();
        assert!(
            close(sampling(&greedy), (0.0, 1.0, 1, 0.0, 1.0)),
            "{greedy}"
        );
    }

    // An override sets only what it names.
    let partial = body_of(
        &client,
        &CompletionRequest {
            input: user("Hi"),
            sampling: Some(SamplingOverride {
                temperature: Some(0.2),
                ..Default::default()
            }),
            ..Default::default()
        },
    )
    .unwrap();
    assert!(
        close(sampling(&partial), (0.2, 0.8, 20, 1.5, 1.0)),
        "{partial}"
    );
}

/// llama-server's final frame carries `timings` alongside the finish reason.
#[test]
fn the_final_frame_reports_cached_and_evaluated_prompt_tokens() {
    let mut decoder = streaming::Decoder::for_completion();
    let frames = [
        json!({"choices":[{"index":0,"delta":{"content":"Hi"},"finish_reason":null}]}),
        json!({"choices":[{"index":0,"delta":{},"finish_reason":"stop"}],
            "timings":{"cache_n":2048,"prompt_n":12,"prompt_ms":40.5,"prompt_per_second":296.3,
                "predicted_n":7,"predicted_ms":140.0,"predicted_per_second":50.0}}),
        json!({"choices":[],"usage":{"prompt_tokens":2060,"completion_tokens":7}}),
    ];
    for frame in frames {
        decoder
            .push(format!("data: {frame}\n\n").as_bytes())
            .unwrap();
    }
    decoder.push(b"data: [DONE]\n\n").unwrap();
    assert_eq!(
        decoder.timings(),
        Some(streaming::ServerTimings {
            cached_prompt_tokens: 2048,
            evaluated_prompt_tokens: 12,
            prompt_ms: 40.5,
            generated_tokens: 7,
            generated_per_second: 50.0,
        })
    );
    let response = decoder.into_response().unwrap();
    assert_eq!(response.text, "Hi");
    assert_eq!(response.input_tokens, 2060);
    // The pace counts what the server read, not what its cache held.
    let timing = response.timing.unwrap();
    assert_eq!((timing.prompt_tokens, timing.output_tokens), (12, 7));
    assert!((timing.prompt_seconds - 0.0405).abs() < 1e-9);
    assert!((timing.output_seconds - 0.14).abs() < 1e-9);

    // A server that reports none leaves nothing to log.
    let mut quiet = streaming::Decoder::for_completion();
    quiet
        .push(b"data: {\"choices\":[{\"index\":0,\"delta\":{\"content\":\"Hi\"},\"finish_reason\":\"stop\"}]}\n\n")
        .unwrap();
    assert_eq!(quiet.timings(), None);
}

/// A whole answer carries the same `timings` object as a stream's last
/// frame; one without it is not timed.
#[test]
fn a_whole_answer_reports_its_timing_too() {
    let message = json!([{"message":{"role":"assistant","content":"Hi"},"finish_reason":"stop"}]);
    let response = parse_completion(json!({"choices":message.clone(),
        "usage":{"prompt_tokens":2060,"completion_tokens":226},
        "timings":{"cache_n":360,"prompt_n":1700,"prompt_ms":2000.0,
            "predicted_n":226,"predicted_ms":20000.0,"predicted_per_second":11.3}}))
    .unwrap();
    assert_eq!(
        response
            .timing
            .map(|timing| (timing.prompt_tokens, timing.output_tokens)),
        Some((1700, 226))
    );
    let timing: ServedTiming = response.timing.unwrap();
    assert!((timing.prompt_seconds - 2.0).abs() < 1e-9);
    assert!((timing.output_seconds - 20.0).abs() < 1e-6);
    assert_eq!(
        parse_completion(json!({"choices":message})).unwrap().timing,
        None
    );
}

/// Laguna XS 2.1 repeats one special token with thinking off, so it thinks on
/// every request, tool rounds and utility calls included. A level a caller set
/// for cloud models does not ride along; the user's pin still wins.
#[test]
fn a_model_that_needs_thinking_thinks_on_every_request_unless_the_user_pins_it_off() {
    let utility_call = CompletionRequest {
        input: user("Label this."),
        reasoning_effort: Some("low".into()),
        ..Default::default()
    };
    let tool_round = CompletionRequest {
        input: user("Find it"),
        tools: vec![search_tool()],
        ..Default::default()
    };
    let laguna = local_client(ThinkingPolicy::AlwaysOn, ThinkingOverrideDto::Auto);
    for request in [&utility_call, &tool_round] {
        let body = body_of(&laguna, request).unwrap();
        assert_eq!(body["chat_template_kwargs"]["enable_thinking"], true);
        assert!(body.get("reasoning_effort").is_none(), "{body}");
        assert!(close(sampling(&body), (1.0, 0.95, 20, 1.5, 1.0)), "{body}");
    }

    let pinned_off = local_client(ThinkingPolicy::AlwaysOn, ThinkingOverrideDto::Off);
    let body = body_of(&pinned_off, &utility_call).unwrap();
    assert_eq!(body["chat_template_kwargs"]["enable_thinking"], false);

    let pinned_on = local_client(ThinkingPolicy::PerRequest, ThinkingOverrideDto::On);
    let body = body_of(&pinned_on, &tool_round).unwrap();
    assert_eq!(body["chat_template_kwargs"]["enable_thinking"], true);

    let qwen = local_client(ThinkingPolicy::PerRequest, ThinkingOverrideDto::Auto);
    let body = body_of(&qwen, &tool_round).unwrap();
    assert_eq!(body["chat_template_kwargs"]["enable_thinking"], false);
}

/// One 400 KB picture from the user, costing `tokens`, before `text`.
fn picture_request(role: &str, text: &str, tokens: u32) -> CompletionRequest {
    use lattice_core::application::ports::llm_port::{ContentPart, ImagePart};
    CompletionRequest {
        input: vec![CompletionInput::Multimodal {
            role: role.into(),
            parts: vec![
                ContentPart::Image(ImagePart {
                    mime: "image/jpeg".into(),
                    data_base64: "A".repeat(400_000),
                    width: 896,
                    height: 1120,
                    tokens,
                    alt: "photo.jpg".into(),
                }),
                ContentPart::Text { text: text.into() },
            ],
        }],
        ..Default::default()
    }
}

fn seeing(client: LlamaCppLlm) -> LlamaCppLlm {
    use lattice_core::domain::models::vision::VisionPreset;
    client.with_vision(Some(VisionPreset::Qwen25Vl.geometry()))
}

#[test]
fn a_picture_is_sent_as_an_image_url_before_the_text() {
    let body = body_of(
        &seeing(client()),
        &picture_request("user", "What is this?", 1_282),
    )
    .unwrap();
    let content = &body["messages"][0]["content"];
    assert_eq!(content[0]["type"], "image_url");
    let url = content[0]["image_url"]["url"].as_str().unwrap();
    assert!(url.starts_with("data:image/jpeg;base64,AAAA"), "{url:.40}");
    assert_eq!(content[1], json!({"type":"text","text":"What is this?"}));
}

#[test]
fn a_model_nobody_declared_able_to_see_refuses_pictures() {
    let refused = body_of(&client(), &picture_request("user", "What is this?", 1_282)).unwrap_err();
    assert!(matches!(refused, AppError::InvalidConfig(_)), "{refused}");
    assert!(
        refused.to_string().contains("is not set up to see images"),
        "{refused}"
    );
}

#[test]
fn pictures_outside_a_user_message_are_refused() {
    let refused = body_of(
        &seeing(client()),
        &picture_request("assistant", "Here", 1_282),
    )
    .unwrap_err();
    assert!(matches!(refused, AppError::InvalidInput(_)), "{refused}");
}

#[test]
fn a_pictures_bytes_never_count_as_prompt_text() {
    let request = picture_request("user", "What is this?", 1_282);
    // The prefill allowance adds the picture's declared tokens, not its bytes.
    let allowance = retry::prefill_allowance(&request.input);
    assert_eq!(
        allowance,
        retry::FIRST_FRAME_FLOOR
            + Duration::from_secs((1_282 / retry::PREFILL_FLOOR_TOKENS_PER_SEC) as u64)
    );
}

#[test]
fn the_ceiling_sent_is_the_one_the_shared_rule_settles_on() {
    let client = client();
    let capped = CompletionRequest {
        input: user("hi"),
        max_output_tokens: Some(512),
        ..Default::default()
    };
    assert_eq!(body_of(&client, &capped).unwrap()["max_tokens"], 512);
    let open = CompletionRequest {
        input: user("hi"),
        ..Default::default()
    };
    let configured = u32::try_from(client.config.generation.max_tokens).unwrap();
    assert_eq!(client.output_limits(), OutputLimits::known(configured));
    assert_eq!(body_of(&client, &open).unwrap()["max_tokens"], configured);
    // The room the scheduler measured cuts it.
    let cut = Fitted::with_room(
        &client,
        &open,
        PromptMeasure {
            tokens: 8_000,
            counted_exactly: true,
        },
        100,
    );
    assert_eq!(client.body(&cut).unwrap()["max_tokens"], 100);
    let size = cut.size();
    assert_eq!(
        (size.prompt_tokens, size.counted_exactly, size.output_tokens),
        (8_000, true, 100)
    );
}
