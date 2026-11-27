use super::*;
use crate::features::llm::scheduler::{BackendCapacity, InferenceScheduler, ScheduledLlm};
use lattice_core::application::contracts::settings::ThinkingOverrideDto;

fn client_for(
    model: &str,
    supports_thinking: bool,
    model_thinking: ThinkingPolicy,
    thinking_override: ThinkingOverrideDto,
) -> Result<LlamaCppLlm> {
    let settings = LLMSettingsDto {
        thinking_override,
        ..Default::default()
    };
    let config = ServerConfig {
        model: model.into(),
        generation: GenerationConfig::from_settings(&settings),
        context_window: 32_768,
        stall_timeout: Duration::from_secs(30),
        supports_tools: true,
        supports_thinking,
        model_thinking,
        prefill_guard: true,
        vision: None,
    };
    LlamaCppLlm::connect("http://127.0.0.1:8080", "token", config)
}

fn client_with(
    model_thinking: ThinkingPolicy,
    thinking_override: ThinkingOverrideDto,
) -> Result<LlamaCppLlm> {
    client_for("qwen3.5", true, model_thinking, thinking_override)
}

fn remote_client(model: &str, thinking_override: ThinkingOverrideDto) -> Result<LlamaCppLlm> {
    let settings = LLMSettingsDto {
        thinking_override,
        llama_cpp: LlamaCppSettingsDto {
            url: "http://127.0.0.1:8080".into(),
            model: model.into(),
            ..Default::default()
        },
        ..Default::default()
    };
    LlamaCppLlm::new(&settings, None)
}

#[test]
fn supports_native_thinking_respects_model_policy_and_user_override() -> Result<()> {
    let per_request = client_with(ThinkingPolicy::PerRequest, ThinkingOverrideDto::Auto)?;
    assert!(per_request.supports_native_thinking());

    let always_on = client_with(ThinkingPolicy::AlwaysOn, ThinkingOverrideDto::Auto)?;
    assert!(always_on.supports_native_thinking());

    let user_off = client_with(ThinkingPolicy::PerRequest, ThinkingOverrideDto::Off)?;
    assert!(!user_off.supports_native_thinking());

    let pinned_off_over_always_on =
        client_with(ThinkingPolicy::AlwaysOn, ThinkingOverrideDto::Off)?;
    assert!(!pinned_off_over_always_on.supports_native_thinking());

    let model_off = client_with(ThinkingPolicy::AlwaysOff, ThinkingOverrideDto::Auto)?;
    assert!(!model_off.supports_native_thinking());

    let user_on_over_model_off = client_with(ThinkingPolicy::AlwaysOff, ThinkingOverrideDto::On)?;
    assert!(user_on_over_model_off.supports_native_thinking());

    let non_thinking = client_for(
        "Meta-Llama-3.1-8B-Instruct",
        false,
        ThinkingPolicy::PerRequest,
        ThinkingOverrideDto::Auto,
    )?;
    assert!(!non_thinking.supports_native_thinking());

    let non_thinking_user_on = client_for(
        "Meta-Llama-3.1-8B-Instruct",
        false,
        ThinkingPolicy::PerRequest,
        ThinkingOverrideDto::On,
    )?;
    assert!(non_thinking_user_on.supports_native_thinking());
    Ok(())
}

#[test]
fn supports_native_thinking_distinguishes_thinking_and_non_thinking_models() -> Result<()> {
    // Remote clients must never guess thinking support from model-name substrings;
    // capability is learned from `/props` `chat_template` in `schedule()`.
    for model in [
        "Qwen3.6-35B-A3B-UD-Q4_K_XL",
        "Qwen3.5-9B-Q4_K_M",
        "Laguna-XS-2.1-APEX-I-Compact",
        "DeepSeek-R1-Distill-Qwen-7B",
        "QwQ-32B",
        "Meta-Llama-3.1-8B-Instruct",
        "Mistral-7B-Instruct-v0.3",
        "gemma-4-12b-it",
    ] {
        let auto = remote_client(model, ThinkingOverrideDto::Auto)?;
        assert!(
            !auto.supports_native_thinking(),
            "{model} must not guess native thinking from model name before /props"
        );

        let on = remote_client(model, ThinkingOverrideDto::On)?;
        assert!(
            on.supports_native_thinking(),
            "{model} with AlwaysOn override must think natively"
        );

        let off = remote_client(model, ThinkingOverrideDto::Off)?;
        assert!(
            !off.supports_native_thinking(),
            "{model} with AlwaysOff override must not think natively"
        );
    }

    let qwen36 = client_for(
        "Qwen3.6-35B-A3B",
        true,
        ThinkingPolicy::PerRequest,
        ThinkingOverrideDto::Auto,
    )?;
    assert!(qwen36.supports_native_thinking());

    let laguna = client_for(
        "Laguna-XS-2.1",
        true,
        ThinkingPolicy::AlwaysOn,
        ThinkingOverrideDto::Auto,
    )?;
    assert!(laguna.supports_native_thinking());

    let laguna_off = client_for(
        "Laguna-XS-2.1",
        true,
        ThinkingPolicy::AlwaysOn,
        ThinkingOverrideDto::Off,
    )?;
    assert!(!laguna_off.supports_native_thinking());

    let mistral = client_for(
        "Mistral-7B-Instruct-v0.3",
        false,
        ThinkingPolicy::PerRequest,
        ThinkingOverrideDto::Auto,
    )?;
    assert!(!mistral.supports_native_thinking());
    Ok(())
}

#[test]
fn scheduled_llama_cpp_exposes_native_thinking_for_per_request_models() -> Result<()> {
    let scheduler = Arc::new(InferenceScheduler::new(
        "test",
        BackendCapacity::llama_server(2, 32_768),
    ));

    let qwen = ScheduledLlm::new(
        Arc::new(client_with(
            ThinkingPolicy::PerRequest,
            ThinkingOverrideDto::Auto,
        )?),
        Arc::clone(&scheduler),
    );
    assert!(!qwen.thinks_before_answering());
    assert!(qwen.supports_native_thinking());

    let qwen_off = ScheduledLlm::new(
        Arc::new(client_with(
            ThinkingPolicy::PerRequest,
            ThinkingOverrideDto::Off,
        )?),
        Arc::clone(&scheduler),
    );
    assert!(!qwen_off.thinks_before_answering());
    assert!(!qwen_off.supports_native_thinking());

    let laguna = ScheduledLlm::new(
        Arc::new(client_with(
            ThinkingPolicy::AlwaysOn,
            ThinkingOverrideDto::Auto,
        )?),
        Arc::clone(&scheduler),
    );
    assert!(laguna.thinks_before_answering());
    assert!(laguna.supports_native_thinking());

    let llama = ScheduledLlm::new(
        Arc::new(client_for(
            "Meta-Llama-3.1-8B-Instruct",
            false,
            ThinkingPolicy::PerRequest,
            ThinkingOverrideDto::Auto,
        )?),
        Arc::clone(&scheduler),
    );
    assert!(!llama.thinks_before_answering());
    assert!(!llama.supports_native_thinking());

    let mistral = ScheduledLlm::new(
        Arc::new(client_for(
            "Mistral-7B-Instruct-v0.3",
            false,
            ThinkingPolicy::PerRequest,
            ThinkingOverrideDto::Auto,
        )?),
        scheduler,
    );
    assert!(!mistral.thinks_before_answering());
    assert!(!mistral.supports_native_thinking());
    Ok(())
}
