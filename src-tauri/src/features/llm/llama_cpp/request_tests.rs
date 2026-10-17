//! The thinking switch, per-mode sampling, tool choice and server timings.
use super::*;
use crate::features::llm::scheduler::rejection::RequestSize;
use lattice_core::application::contracts::settings::ThinkingOverrideDto;
use lattice_core::application::ports::llm_port::{
    SamplingOverride, ThinkingMode, ThinkingPolicy, ToolChoice, ToolDefinition,
};

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
        let body = client
            .body(&CompletionRequest {
                input: user("Hi"),
                reasoning_effort: effort.map(str::to_owned),
                ..Default::default()
            })
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
        client
            .body(&CompletionRequest {
                input: user("Why?"),
                thinking: ThinkingMode::On,
                reasoning_effort: effort.map(str::to_owned),
                ..Default::default()
            })
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
    let body = client.body(&request).unwrap();
    assert_eq!(body["chat_template_kwargs"]["enable_thinking"], false);
    assert!(body.get("reasoning_effort").is_none());
    assert!(body.get("tool_choice").is_none(), "no choice leaves auto");

    let withdrawn = CompletionRequest {
        tool_choice: Some(ToolChoice::None),
        ..request
    };
    let body = client.body(&withdrawn).unwrap();
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
    let required = client
        .body(&CompletionRequest {
            input: user("Find it"),
            tools: vec![search_tool()],
            tool_choice: Some(ToolChoice::Required),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(required["tool_choice"], "required");
    let bare = client
        .body(&CompletionRequest {
            input: user("Hi"),
            tool_choice: Some(ToolChoice::None),
            ..Default::default()
        })
        .unwrap();
    assert!(bare.get("tool_choice").is_none());
    assert!(bare.get("tools").is_none());
}

/// The Qwen 3.5 card's two profiles: answering directly samples cooler and
/// narrower than thinking does; both keep the presence penalty that stops
/// loops and leave the multiplicative repeat penalty off.
#[test]
fn each_mode_samples_with_its_own_profile_unless_the_caller_overrides() {
    let client = client();
    let answer = client
        .body(&CompletionRequest {
            input: user("Hi"),
            ..Default::default()
        })
        .unwrap();
    assert!(
        close(sampling(&answer), (0.7, 0.8, 20, 1.5, 1.0)),
        "{answer}"
    );

    let thinking = client
        .body(&CompletionRequest {
            input: user("Why?"),
            thinking: ThinkingMode::On,
            ..Default::default()
        })
        .unwrap();
    assert!(
        close(sampling(&thinking), (1.0, 0.95, 20, 1.5, 1.0)),
        "{thinking}"
    );

    // A verdict stays greedy whichever mode it runs in, and a presence
    // penalty would push it off a label word the prompt just listed.
    for mode in [ThinkingMode::Off, ThinkingMode::On] {
        let greedy = client
            .body(&CompletionRequest {
                input: user("Supported or not?"),
                thinking: mode,
                sampling: Some(SamplingOverride::deterministic()),
                ..Default::default()
            })
            .unwrap();
        assert!(
            close(sampling(&greedy), (0.0, 1.0, 1, 0.0, 1.0)),
            "{greedy}"
        );
    }

    // An override sets only what it names.
    let partial = client
        .body(&CompletionRequest {
            input: user("Hi"),
            sampling: Some(SamplingOverride {
                temperature: Some(0.2),
                ..Default::default()
            }),
            ..Default::default()
        })
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

    // A server that reports none leaves nothing to log.
    let mut quiet = streaming::Decoder::for_completion();
    quiet
        .push(b"data: {\"choices\":[{\"index\":0,\"delta\":{\"content\":\"Hi\"},\"finish_reason\":\"stop\"}]}\n\n")
        .unwrap();
    assert_eq!(quiet.timings(), None);
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
        let body = laguna.body(request).unwrap();
        assert_eq!(body["chat_template_kwargs"]["enable_thinking"], true);
        assert!(body.get("reasoning_effort").is_none(), "{body}");
        assert!(close(sampling(&body), (1.0, 0.95, 20, 1.5, 1.0)), "{body}");
    }

    let pinned_off = local_client(ThinkingPolicy::AlwaysOn, ThinkingOverrideDto::Off);
    let body = pinned_off.body(&utility_call).unwrap();
    assert_eq!(body["chat_template_kwargs"]["enable_thinking"], false);

    let pinned_on = local_client(ThinkingPolicy::PerRequest, ThinkingOverrideDto::On);
    let body = pinned_on.body(&tool_round).unwrap();
    assert_eq!(body["chat_template_kwargs"]["enable_thinking"], true);

    let qwen = local_client(ThinkingPolicy::PerRequest, ThinkingOverrideDto::Auto);
    let body = qwen.body(&tool_round).unwrap();
    assert_eq!(body["chat_template_kwargs"]["enable_thinking"], false);
}

/// A server with an 8,192-token window whose tokenizer answers `tokens`, or
/// does not exist, sizing a request that fixes its answer at `asked` tokens
/// or, with `None`, takes the room it is given.
async fn measured(
    tokens: Option<usize>,
    content: &str,
    asked: Option<u32>,
) -> Result<(u64, RequestSize)> {
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};
    let server = MockServer::start().await;
    let reply = match tokens {
        Some(tokens) => {
            ResponseTemplate::new(200).set_body_json(json!({"tokens": vec![1; tokens]}))
        }
        None => ResponseTemplate::new(404),
    };
    Mock::given(method("POST"))
        .and(path("/tokenize"))
        .respond_with(reply)
        .mount(&server)
        .await;
    let settings = LLMSettingsDto::default();
    let config = ServerConfig {
        model: "laguna".into(),
        generation: GenerationConfig::from_settings(&settings),
        context_window: 8192,
        stall_timeout: Duration::from_secs(30),
        supports_tools: false,
        model_thinking: ThinkingPolicy::default(),
        prefill_guard: false,
    };
    let client = LlamaCppLlm::connect(&server.uri(), "token", config).unwrap();
    let (body, size) = client
        .measured_body(&CompletionRequest {
            input: user(content),
            max_output_tokens: asked,
            ..Default::default()
        })
        .await?;
    Ok((body["max_tokens"].as_u64().unwrap(), size))
}

#[tokio::test]
async fn a_prompt_the_estimate_overcounts_keeps_the_output_it_asked_for() {
    let long = "word ".repeat(8_000);
    // The server counts 2,000 tokens, so 2,000 more still fit.
    let (reserved, size) = measured(Some(2_000), &long, Some(2_000)).await.unwrap();
    assert_eq!(reserved, 2_000);
    assert!(size.counted_exactly);
}

#[tokio::test]
async fn a_fixed_answer_the_window_cannot_hold_is_refused_never_shrunk() {
    let long = "word ".repeat(8_000);
    for (tokens, counted) in [(Some(7_900), "(counted exactly)"), (None, "(estimated)")] {
        let refused = measured(tokens, &long, Some(2_000)).await.unwrap_err();
        assert!(matches!(refused, AppError::InvalidInput(_)), "{refused}");
        let told = refused.to_string();
        for expected in [
            counted,
            "2000 for the answer",
            "configured context window is 8192",
        ] {
            assert!(told.contains(expected), "{told}");
        }
    }
}

#[tokio::test]
async fn a_server_that_cannot_count_is_not_refused_on_the_reservations_margin() {
    // About 6,000 tokens at four characters a token: the margin reads 9,000,
    // past the 8,192 window; the plain estimate leaves room.
    let prose = "word ".repeat(4_800);
    let (reserved, size) = measured(None, &prose, Some(2_000)).await.unwrap();
    assert_eq!(reserved, 2_000);
    assert!(!size.counted_exactly);
    let (reserved, size) = measured(None, &prose, None).await.unwrap();
    assert_eq!(size.prompt_tokens + reserved as usize, 8_192);
    assert!(!size.counted_exactly);
}

#[tokio::test]
async fn an_open_answer_takes_the_room_the_prompt_leaves_and_never_more() {
    let long = "word ".repeat(8_000);
    // The estimate leaves nothing; the exact count leaves 2,168 of the 8,192.
    let (reserved, size) = measured(Some(6_000), &long, None).await.unwrap();
    assert_eq!(size.prompt_tokens + reserved as usize, 8_192);
    assert!(size.counted_exactly);
    // A prompt that fills the window leaves no room, and is refused.
    let refused = measured(Some(8_192), &long, None).await.unwrap_err();
    assert!(
        refused.to_string().contains("leaves no room for an answer"),
        "{refused}"
    );
}

#[tokio::test]
async fn a_prompt_the_estimate_fits_is_not_counted_again() {
    use wiremock::matchers::path;
    use wiremock::{Mock, MockServer, ResponseTemplate};
    let server = MockServer::start().await;
    Mock::given(path("/tokenize"))
        .respond_with(ResponseTemplate::new(200))
        .expect(0)
        .mount(&server)
        .await;
    let settings = LLMSettingsDto::default();
    let config = ServerConfig {
        model: "laguna".into(),
        generation: GenerationConfig::from_settings(&settings),
        context_window: 8192,
        stall_timeout: Duration::from_secs(30),
        supports_tools: false,
        model_thinking: ThinkingPolicy::default(),
        prefill_guard: false,
    };
    let client = LlamaCppLlm::connect(&server.uri(), "token", config).unwrap();
    let (body, size) = client
        .measured_body(&CompletionRequest {
            input: user("A short question."),
            max_output_tokens: Some(2_000),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(body["max_tokens"], 2_000);
    assert!(!size.counted_exactly);
}
