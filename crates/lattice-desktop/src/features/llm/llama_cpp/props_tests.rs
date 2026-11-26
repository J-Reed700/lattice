use super::*;
use lattice_core::application::contracts::settings::ThinkingOverrideDto;
use wiremock::{
    matchers::{method, path},
    Mock, MockServer, ResponseTemplate,
};

fn remote_settings(
    url: String,
    model: &str,
    thinking_override: ThinkingOverrideDto,
) -> LLMSettingsDto {
    LLMSettingsDto {
        thinking_override,
        llama_cpp: LlamaCppSettingsDto {
            url,
            model: model.into(),
            ..Default::default()
        },
        ..Default::default()
    }
}

#[tokio::test]
async fn remote_server_detects_thinking_from_props_chat_template_not_model_name() {
    for template in [
        "{% if enable_thinking is defined and enable_thinking %}<think>{% endif %}",
        "{{ bos_token }}<think>\n{{ reasoning }}\n</think>",
        "Answer after </think>.",
    ] {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/props"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "total_slots": 2,
                "default_generation_settings": {"n_ctx": 32_768},
                "chat_template": template,
            })))
            .expect(1)
            .mount(&server)
            .await;

        // Even an opaque/non-thinking model name is recognized from `/props`.
        let llm = LlamaCppLlm::new(
            &remote_settings(
                server.uri(),
                "custom-renamed-model",
                ThinkingOverrideDto::Auto,
            ),
            None,
        )
        .unwrap()
        .schedule(&SchedulerRegistry::default())
        .await;
        assert!(
            llm.supports_native_thinking(),
            "template {template:?} should enable native thinking"
        );
    }
}

#[tokio::test]
async fn remote_server_with_non_thinking_props_template_or_missing_template_falls_back_to_false() {
    // Plain template on a model named "Qwen3.5-9B" must trust `/props`, not the name.
    let plain_server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/props"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "total_slots": 1,
            "chat_template": "{{ bos_token }}{% for m in messages %}{{ m['content'] }}{% endfor %}",
        })))
        .mount(&plain_server)
        .await;
    let plain = LlamaCppLlm::new(
        &remote_settings(plain_server.uri(), "Qwen3.5-9B", ThinkingOverrideDto::Auto),
        None,
    )
    .unwrap()
    .schedule(&SchedulerRegistry::default())
    .await;
    assert!(!plain.supports_native_thinking());

    // `/props` omitting `chat_template` gracefully falls back to false without guessing by name.
    let omitted_server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/props"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "total_slots": 2,
            "default_generation_settings": {"n_ctx": 16_384},
        })))
        .mount(&omitted_server)
        .await;
    let omitted = LlamaCppLlm::new(
        &remote_settings(
            omitted_server.uri(),
            "Qwen3.5-9B",
            ThinkingOverrideDto::Auto,
        ),
        None,
    )
    .unwrap()
    .schedule(&SchedulerRegistry::default())
    .await;
    assert!(!omitted.supports_native_thinking());

    // Non-llama-server (404 on `/props`) also gracefully falls back to false.
    let missing_server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/props"))
        .respond_with(ResponseTemplate::new(404))
        .mount(&missing_server)
        .await;
    let missing = LlamaCppLlm::new(
        &remote_settings(
            missing_server.uri(),
            "DeepSeek-R1-Distill",
            ThinkingOverrideDto::Auto,
        ),
        None,
    )
    .unwrap()
    .schedule(&SchedulerRegistry::default())
    .await;
    assert!(!missing.supports_native_thinking());
}

#[tokio::test]
async fn shared_registry_reuses_props_thinking_across_roles_and_respects_overrides() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/props"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "total_slots": 2,
            "chat_template": "{% if enable_thinking %}<think>{% endif %}",
        })))
        .expect(1)
        .mount(&server)
        .await;

    let registry = SchedulerRegistry::default();
    let first = LlamaCppLlm::new(
        &remote_settings(server.uri(), "custom-a", ThinkingOverrideDto::Auto),
        None,
    )
    .unwrap()
    .schedule(&registry)
    .await;
    let second = LlamaCppLlm::new(
        &remote_settings(server.uri(), "custom-b", ThinkingOverrideDto::Auto),
        None,
    )
    .unwrap()
    .schedule(&registry)
    .await;
    assert!(first.supports_native_thinking());
    assert!(second.supports_native_thinking());

    let pinned_off = LlamaCppLlm::new(
        &remote_settings(server.uri(), "custom-c", ThinkingOverrideDto::Off),
        None,
    )
    .unwrap()
    .schedule(&registry)
    .await;
    assert!(!pinned_off.supports_native_thinking());
}
