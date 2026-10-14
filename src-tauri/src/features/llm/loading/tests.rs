use super::*;

#[test]
fn generation_settings_are_preserved() {
    let settings = LLMSettingsDto {
        temperature: 0.25,
        top_p: 0.8,
        top_k: 17,
        max_tokens: 731,
        repeat_penalty: 1.2,
        ..Default::default()
    };
    let config = ModelLoader::generation_config_from_settings(&settings);
    assert_eq!(config.temperature, settings.temperature);
    assert_eq!(config.top_p, settings.top_p);
    assert_eq!(config.top_k, settings.top_k);
    assert_eq!(config.max_tokens, settings.max_tokens as usize);
    assert_eq!(config.repeat_penalty, settings.repeat_penalty);
    assert_eq!(config.presence_penalty, settings.presence_penalty);
    assert_eq!(
        config.thinking,
        crate::features::llm::engine::SamplingProfile::from(settings.thinking_sampling)
    );
}

#[test]
fn a_file_that_failed_to_start_is_retried_only_after_it_changes() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("model.gguf");
    std::fs::write(&path, b"broken").expect("write");

    let unloadable = UnloadableArtifacts::default();
    assert_eq!(unloadable.known_failure(&path), None);
    unloadable.remember(&path, "unsupported architecture".into());
    assert_eq!(
        unloadable.known_failure(&path).as_deref(),
        Some("unsupported architecture")
    );

    // A repaired or re-downloaded file is different bytes, so the memo
    // must not keep a working model from ever being tried again.
    std::fs::write(&path, b"repaired and longer").expect("rewrite");
    assert_eq!(unloadable.known_failure(&path), None);
}

#[tokio::test]
async fn a_cached_utility_failure_is_an_error_not_a_successful_no_op() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("test-model.gguf");
    std::fs::write(&path, b"model fixture").unwrap();
    let active = crate::domain::DownloadedModel::new(
        "test-id".into(),
        "Test utility".into(),
        "test-model".into(),
        crate::domain::models::downloaded::ModelLocation::LocalFile { path: path.clone() },
        13,
        "GGUF".into(),
        None,
    )
    .unwrap();
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .connect_lazy("sqlite::memory:")
        .unwrap();
    let runtime = LlmRuntime::default();
    let loader = ModelLoader::new(
        Arc::new(DownloadedModelRepository::new(pool)),
        None,
        runtime.clone(),
    );
    runtime.unloadable.remember(
        &path,
        "This engine does not support the architecture".into(),
    );

    for _ in 0..2 {
        let result = loader
            .load_utility_local(
                &active,
                crate::features::llm::engine::GenerationConfig::default(),
                LocalServerOptions::default(),
            )
            .await;
        assert!(matches!(result, Err(AppError::ModelLoadFailed(message))
            if message == "This engine does not support the architecture"));
    }
    std::fs::remove_file(&path).unwrap();
    let result = loader
        .load_utility_local(
            &active,
            crate::features::llm::engine::GenerationConfig::default(),
            LocalServerOptions::default(),
        )
        .await;
    assert!(matches!(result, Err(AppError::ModelLoadFailed(message))
        if message.contains("missing from disk")));
}

#[test]
fn unusable_sidecar_is_not_reported_as_a_model_failure() {
    let mapped = unusable_sidecar(&LLMError::SidecarBinaryUnusable("broken".into()));
    assert!(matches!(mapped, Some(AppError::ServiceNotAvailable(m)) if m == "broken"));
    assert!(unusable_sidecar(&LLMError::GenerationFailed("bad gguf".into())).is_none());
    assert!(unusable_sidecar(&LLMError::Timeout).is_none());
}

#[tokio::test]
async fn auto_and_explicit_llamacpp_use_their_connection_despite_stale_ollama_and_local_selections()
{
    use crate::application::{
        contracts::settings::{LLMProvider, LlamaCppSettingsDto},
        ports::llm_port::{CompletionInput, CompletionRequest},
    };
    use wiremock::{
        matchers::{header, method, path},
        Mock, MockServer, ResponseTemplate,
    };

    for provider in [LLMProvider::Auto, LLMProvider::Llamacpp] {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .and(header("authorization", "Basic test-credential"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-type", "text/event-stream")
                    .set_body_string(concat!(
                "data: {\"choices\":[{\"delta\":{\"content\":\"OK\"},\"finish_reason\":null}]}\n\n",
                "data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}]}\n\n",
                "data: [DONE]\n\n"
            )),
            )
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/v1/models"))
            .and(header("authorization", "Basic test-credential"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!({"data":[{"id":"test-model"}]})),
            )
            .expect(if provider == LLMProvider::Auto { 1 } else { 0 })
            .mount(&server)
            .await;
        // The server's scheduler is sized from its `/props`, once, when the
        // connection is first loaded.
        Mock::given(method("GET"))
            .and(path("/props"))
            .and(header("authorization", "Basic test-credential"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(serde_json::json!({"total_slots": 1})),
            )
            .expect(1)
            .mount(&server)
            .await;
        // An uninitialized DB must never be queried for an explicit remote provider.
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .connect_lazy("sqlite::memory:")
            .unwrap();
        let loader = ModelLoader::new(
            Arc::new(DownloadedModelRepository::new(pool)),
            None,
            LlmRuntime::default(),
        );
        let settings = LLMSettingsDto {
            provider,
            ollama_url: server.uri(),
            model: "stale-ollama-model".into(),
            llama_cpp: LlamaCppSettingsDto {
                url: server.uri(),
                model: "test-model".into(),
                auth_header_name: "Authorization".into(),
                auth_header_value: "Basic test-credential".into(),
            },
            ..Default::default()
        };
        let llm = loader.load(&settings).await.unwrap();
        assert_eq!(llm.provider_name(), "llamacpp");
        assert_eq!(llm.model_name(), "test-model");
        let response = llm
            .complete(&CompletionRequest {
                input: vec![CompletionInput::Message {
                    role: "user".into(),
                    content: "Hi".into(),
                }],
                ..Default::default()
            })
            .await
            .unwrap();
        assert_eq!(response.text, "OK");
        assert_eq!(
            server.received_requests().await.unwrap().len(),
            if provider == LLMProvider::Auto { 3 } else { 2 }
        );
    }
}

#[tokio::test]
async fn utility_role_follows_the_chat_provider_policy_on_a_shared_host() {
    // One llama.cpp server is configured both as the llama.cpp connection
    // and as the "Ollama" URL. Under Auto the utility role must speak the
    // protocol chat speaks; llama.cpp serves none of Ollama's routes.
    use crate::application::contracts::settings::{LLMProvider, LlamaCppSettingsDto};
    use wiremock::{
        matchers::{method, path},
        Mock, MockServer, ResponseTemplate,
    };

    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/models"))
        .respond_with(ResponseTemplate::new(200).set_body_json(
            serde_json::json!({"data":[{"id":"chat-model"},{"id":"utility-model"}]}),
        ))
        .expect(1)
        .mount(&server)
        .await;
    for route in ["/api/tags", "/api/generate", "/api/chat"] {
        Mock::given(path(route))
            .respond_with(ResponseTemplate::new(404))
            .expect(0)
            .mount(&server)
            .await;
    }
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .connect_lazy("sqlite::memory:")
        .unwrap();
    let loader = ModelLoader::new(
        Arc::new(DownloadedModelRepository::new(pool)),
        None,
        LlmRuntime::default(),
    );
    let settings = LLMSettingsDto {
        provider: LLMProvider::Auto,
        ollama_url: server.uri(),
        model: "chat-model".into(),
        max_tokens: 512,
        llama_cpp: LlamaCppSettingsDto {
            url: server.uri(),
            model: "chat-model".into(),
            ..Default::default()
        },
        ..Default::default()
    };
    let llm = loader
        .load_utility_remote(
            &settings,
            "utility-model",
            ModelLoader::generation_config_from_settings(&settings),
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(llm.provider_name(), "llamacpp");
    assert_eq!(llm.model_name(), "utility-model");
}

#[tokio::test]
async fn utility_role_uses_ollama_only_when_ollama_is_the_provider() {
    use crate::application::contracts::settings::{LLMProvider, LlamaCppSettingsDto};
    use wiremock::{
        matchers::{method, path},
        Mock, MockServer, ResponseTemplate,
    };

    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/tags"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"models":[]})))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(path("/v1/models"))
        .respond_with(ResponseTemplate::new(404))
        .expect(0)
        .mount(&server)
        .await;
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .connect_lazy("sqlite::memory:")
        .unwrap();
    let loader = ModelLoader::new(
        Arc::new(DownloadedModelRepository::new(pool)),
        None,
        LlmRuntime::default(),
    );
    let settings = LLMSettingsDto {
        provider: LLMProvider::Ollama,
        ollama_url: server.uri(),
        model: "chat-model".into(),
        llama_cpp: LlamaCppSettingsDto {
            url: server.uri(),
            model: "chat-model".into(),
            ..Default::default()
        },
        ..Default::default()
    };
    let llm = loader
        .load_utility_remote(
            &settings,
            "utility-model",
            ModelLoader::generation_config_from_settings(&settings),
        )
        .await
        .unwrap()
        .unwrap();
    assert_ne!(llm.provider_name(), "llamacpp");
    assert_eq!(llm.model_name(), "utility-model");
}

#[tokio::test]
async fn missing_ollama_configuration_fails_before_network_access() {
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .connect_lazy("sqlite::memory:")
        .unwrap();
    let loader = ModelLoader::new(
        Arc::new(DownloadedModelRepository::new(pool)),
        None,
        LlmRuntime::default(),
    );
    let settings = LLMSettingsDto {
        provider: crate::application::contracts::settings::LLMProvider::Ollama,
        ollama_url: String::new(),
        ..Default::default()
    };
    assert!(matches!(
        loader.load(&settings).await,
        Err(AppError::InvalidConfig(_))
    ));
    assert!(matches!(
        loader
            .try_load_ollama_with_model(
                &settings,
                "router",
                ModelLoader::generation_config_from_settings(&settings),
            )
            .await,
        Err(AppError::InvalidConfig(_))
    ));
}
