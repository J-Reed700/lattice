//! Tests for request limits, priorities, token estimates, thinking policy and logprobs.

#![cfg(test)]

use super::*;

#[tokio::test(start_paused = true)]
async fn interactive_completions_can_opt_out_without_changing_default_limits() {
    let request = CompletionRequest {
        time_budget: Some(Duration::from_secs(1)),
        ..Default::default()
    };
    assert!(request
        .within_time_budget(tokio::time::sleep(Duration::from_secs(2)))
        .await
        .is_err());
    let request = CompletionRequest {
        no_time_limit: true,
        ..request
    };
    request
        .within_time_budget(tokio::time::sleep(Duration::from_secs(3600)))
        .await
        .unwrap();
    assert_eq!(
        CompletionRequest::default().wall_clock_budget(),
        Some(DEFAULT_COMPLETION_TIME_BUDGET)
    );
}

#[test]
fn a_request_output_cap_can_tighten_the_providers_limit_but_never_raise_it() {
    let provider_limit = 4096;
    let uncapped = CompletionRequest::default();
    assert_eq!(
        uncapped.effective_max_output_tokens(provider_limit),
        provider_limit,
        "no request cap leaves the configured limit alone"
    );

    let tighter = CompletionRequest {
        max_output_tokens: Some(512),
        ..Default::default()
    };
    assert_eq!(tighter.effective_max_output_tokens(provider_limit), 512);

    // A budget that reserved more room than the user configured must not
    // silently grant it: the configured ceiling is the user's decision.
    let looser = CompletionRequest {
        max_output_tokens: Some(100_000),
        ..Default::default()
    };
    assert_eq!(
        looser.effective_max_output_tokens(provider_limit),
        provider_limit
    );

    // Zero is meaningless as a generation cap and would produce an empty
    // response rather than an error, so it is treated as "unset".
    let zero = CompletionRequest {
        max_output_tokens: Some(0),
        ..Default::default()
    };
    assert_eq!(
        zero.effective_max_output_tokens(provider_limit),
        provider_limit
    );
}

#[test]
fn interactive_work_outranks_verification_maintenance_and_background() {
    use InferencePriority::*;
    assert!(Interactive > Verification);
    assert!(Verification > Maintenance);
    assert!(Maintenance > Background);
    assert_eq!(CompletionRequest::default().priority, Background);
}

#[test]
fn token_estimates_round_up_and_character_budgets_err_short() {
    assert_eq!(tokens_for_chars(0, 4.0), 0);
    assert_eq!(tokens_for_chars(1, 4.0), 1);
    assert_eq!(tokens_for_chars(9, 3.0), 3);
    assert_eq!(
        tokens_for_chars(10, 0.0),
        3,
        "a broken ratio falls back to the default"
    );
    assert_eq!(estimate_tokens("héllo wörld!"), 3, "characters, not bytes");
    assert_eq!(chars_within_tokens(1_000, 4.0), 3_000);
    assert_eq!(chars_within_tokens(1_000, 3.0), 2_250);
}

#[test]
fn a_pinned_policy_overrides_the_request_and_per_request_holds_tool_rounds_off() {
    let tool = ToolDefinition {
        name: "search".into(),
        description: "Search".into(),
        parameters: serde_json::json!({"type":"object"}),
    };
    let plain = CompletionRequest::default();
    let thinking = CompletionRequest {
        thinking: ThinkingMode::On,
        ..Default::default()
    };
    let tool_round = CompletionRequest {
        tools: vec![tool],
        ..thinking.clone()
    };
    let per_request = ThinkingPolicy::PerRequest;
    assert_eq!(per_request.mode_for(&plain), ThinkingMode::Off);
    assert_eq!(per_request.mode_for(&thinking), ThinkingMode::On);
    assert_eq!(per_request.mode_for(&tool_round), ThinkingMode::Off);
    assert_eq!(ThinkingPolicy::AlwaysOn.mode_for(&plain), ThinkingMode::On);
    assert_eq!(
        ThinkingPolicy::AlwaysOn.mode_for(&tool_round),
        ThinkingMode::On
    );
    assert_eq!(
        ThinkingPolicy::AlwaysOff.mode_for(&thinking),
        ThinkingMode::Off
    );
    assert_eq!(
        ThinkingPolicy::PerRequest.or(ThinkingPolicy::AlwaysOn),
        ThinkingPolicy::AlwaysOn
    );
    assert_eq!(
        ThinkingPolicy::AlwaysOff.or(ThinkingPolicy::AlwaysOn),
        ThinkingPolicy::AlwaysOff,
        "the user's pin beats the model's"
    );
}

#[test]
fn first_token_alternatives_are_read_from_the_openai_logprobs_shape() {
    let logprobs = serde_json::json!({"content":[
        {"token":"supported","logprob":-0.1,"top_logprobs":[
            {"token":"uns","logprob":-2.5},
            {"token":"supported","logprob":-0.1}
        ]},
        {"token":"!","logprob":-3.0}
    ]});
    let alternatives = first_token_logprobs(&logprobs).expect("first token present");
    assert_eq!(alternatives[0], ("supported".to_string(), -0.1));
    assert_eq!(alternatives[1], ("uns".to_string(), -2.5));

    // No alternatives offered: the chosen token alone still counts.
    let bare = serde_json::json!({"content":[{"token":"contr","logprob":-0.3}]});
    assert_eq!(
        first_token_logprobs(&bare),
        Some(vec![("contr".to_string(), -0.3)])
    );
    assert_eq!(
        first_token_logprobs(&serde_json::json!({"content":[]})),
        None
    );
    assert_eq!(first_token_logprobs(&serde_json::Value::Null), None);
}

fn picture_message(role: &str) -> CompletionInput {
    CompletionInput::Multimodal {
        role: role.into(),
        parts: vec![
            ContentPart::Image(ImagePart {
                mime: "image/png".into(),
                data_base64: "iVBORw0KGgo=".into(),
                width: 640,
                height: 480,
                tokens: 402,
                alt: "plot.png".into(),
            }),
            ContentPart::Text {
                text: "Read the axis".into(),
            },
        ],
    }
}

#[test]
fn a_multimodal_message_round_trips_with_tagged_parts() {
    let json = serde_json::to_value(picture_message("user")).unwrap();
    assert_eq!(json["type"], "multimodal");
    assert_eq!(json["parts"][0]["type"], "image");
    assert_eq!(json["parts"][0]["tokens"], 402);
    assert_eq!(
        json["parts"][1],
        serde_json::json!({"type":"text","text":"Read the axis"})
    );
    let back: CompletionInput = serde_json::from_value(json).unwrap();
    assert_eq!(back.image_tokens(), 402);
}

#[test]
fn pictures_count_by_declared_tokens_and_fall_back_to_captions() {
    let request = CompletionRequest {
        input: vec![
            CompletionInput::Message {
                role: "system".into(),
                content: "Be brief".into(),
            },
            picture_message("user"),
        ],
        ..Default::default()
    };
    assert!(request.has_images());
    assert_eq!((request.image_tokens(), request.image_count()), (402, 1));
    assert_eq!(request.user_text(), "Read the axis");
    assert!(!request.input[0].is_user() && request.input[1].is_user());
    assert!(matches!(
        request.input[1].without_images(),
        CompletionInput::Message { content, .. } if content == "Read the axis\n[Picture: plot.png (640×480)]"
    ));
    assert!(matches!(
        request.input[0].without_images(),
        CompletionInput::Message { content, .. } if content == "Be brief"
    ));
}

#[test]
fn a_model_sees_nothing_unless_declared() {
    struct Plain;
    #[async_trait]
    impl LLMPort for Plain {
        async fn complete(&self, _: &CompletionRequest) -> Result<CompletionResponse> {
            Ok(CompletionResponse::default())
        }
        fn model_name(&self) -> &str {
            "plain"
        }
        fn max_context_tokens(&self) -> usize {
            1
        }
        async fn is_ready(&self) -> Result<bool> {
            Ok(true)
        }
    }
    assert_eq!(Plain.vision(), None);
}
