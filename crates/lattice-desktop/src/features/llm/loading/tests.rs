use super::*;
use crate::infrastructure::persistence::repositories::downloaded_models::DownloadedModelRepository;

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
    let config = GenerationConfig::from_settings(&settings);
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

/// A loader over a migrated in-memory database holding `rows`.
async fn loader_with(rows: &[lattice_core::domain::DownloadedModel]) -> ModelLoader {
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    sqlx::migrate!("./migrations").run(&pool).await.unwrap();
    let models = Arc::new(DownloadedModelRepository::new(pool));
    for row in rows {
        models.save(row).await.unwrap();
    }
    ModelLoader::new(models.clone(), models, None, LlmRuntime::default())
}

/// A loader whose database must never be asked anything.
fn loader_without_database() -> ModelLoader {
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .connect_lazy("sqlite::memory:")
        .unwrap();
    let models = Arc::new(DownloadedModelRepository::new(pool));
    ModelLoader::new(models.clone(), models, None, LlmRuntime::default())
}

fn downloaded(model_id: &str, path: std::path::PathBuf) -> lattice_core::domain::DownloadedModel {
    lattice_core::domain::DownloadedModel::new(
        format!("row-{model_id}"),
        format!("{model_id} (display)"),
        model_id.into(),
        lattice_core::domain::models::downloaded::ModelLocation::LocalFile { path },
        13,
        "GGUF".into(),
        None,
    )
    .unwrap()
}

#[tokio::test]
async fn a_downloaded_model_is_found_by_id_and_a_missing_file_says_where_it_was() {
    let dir = tempfile::tempdir().unwrap();
    let present = dir.path().join("present.gguf");
    std::fs::write(&present, b"model fixture").unwrap();
    let missing = dir.path().join("missing.gguf");
    let loader = loader_with(&[
        downloaded("present", present.clone()),
        downloaded("missing", missing.clone()),
    ])
    .await;

    assert_eq!(
        loader.local_model("present").await.unwrap(),
        Some(LocalModel {
            name: "present (display)".into(),
            path: present,
        })
    );
    assert_eq!(loader.local_model("never-downloaded").await.unwrap(), None);
    assert!(matches!(
        loader.local_model("missing").await,
        Err(AppError::ModelLoadFailed(message))
            if message == format!("missing (display)'s file is missing from {}.", missing.display())
    ));
    assert!(matches!(
        loader
            .load_ref(
                &ModelRef::Local { model_id: "never-downloaded".into() },
                &LLMSettingsDto::default(),
                Tuning::Standard,
            )
            .await,
        Err(AppError::AiModelsNotInstalled(message)) if message.starts_with("never-downloaded is not downloaded")
    ));
}

#[tokio::test]
async fn a_remembered_load_failure_is_reported_without_loading_again() {
    let dir = tempfile::tempdir().unwrap();
    let local = LocalModel {
        name: "Test model".into(),
        path: dir.path().join("test-model.gguf"),
    };
    std::fs::write(&local.path, b"model fixture").unwrap();
    let loader = loader_without_database();
    loader.runtime.unloadable.remember(
        &local.path,
        "This engine does not support the architecture".into(),
    );

    for _ in 0..2 {
        let result = loader
            .load_local(
                &local,
                GenerationConfig::default(),
                LocalServerOptions::default(),
            )
            .await;
        assert!(matches!(result, Err(AppError::ModelLoadFailed(message))
            if message == "This engine does not support the architecture"));
    }
}

/// A model file on disk, for the failure-memory tests.
fn fixture_model(dir: &tempfile::TempDir) -> LocalModel {
    let local = LocalModel {
        name: "Test model".into(),
        path: dir.path().join("test-model.gguf"),
    };
    std::fs::write(&local.path, b"model fixture").unwrap();
    local
}

#[tokio::test]
async fn a_start_that_failed_for_the_machine_is_not_remembered_and_the_next_load_tries_again() {
    let dir = tempfile::tempdir().unwrap();
    let local = fixture_model(&dir);
    let loader = loader_without_database();

    for transient in [
        LLMError::GenerationFailed(
            "llama-server ran out of the 120 s budget shared by every startup attempt".into(),
        ),
        LLMError::GenerationFailed("llama-server exited during startup (signal 9)".into()),
        LLMError::Other("Failed to allocate ephemeral port".into()),
    ] {
        let reported = loader.load_failure(&local, transient);
        assert!(matches!(&reported, AppError::ModelLoadFailed(m)
            if m.starts_with("Test model did not start: ") && !m.contains("can't be loaded")));
        assert_eq!(loader.runtime.unloadable.known_failure(&local.path), None);
    }

    // The next load reaches the engine again; here, with no app to run a
    // server, it fails for that reason rather than replaying an old one.
    let next = loader
        .load_local(
            &local,
            GenerationConfig::default(),
            LocalServerOptions::default(),
        )
        .await;
    assert!(matches!(next, Err(AppError::ModelLoadFailed(m))
        if m.starts_with("Test model did not start: ") && m.contains("app_handle")));
}

#[tokio::test]
async fn a_file_the_engine_cannot_read_is_remembered_with_the_engine_reason() {
    let dir = tempfile::tempdir().unwrap();
    let local = fixture_model(&dir);
    let loader = loader_without_database();
    let reason = "Lattice's bundled local model engine does not support this model \
                  (unknown model architecture: 'laguna').";
    let expected = format!("Test model can't be loaded: {reason}");

    let reported = loader.load_failure(&local, LLMError::ModelUnsupported(reason.into()));
    assert!(matches!(&reported, AppError::ModelLoadFailed(m) if *m == expected));

    let again = loader
        .load_local(
            &local,
            GenerationConfig::default(),
            LocalServerOptions::default(),
        )
        .await;
    assert!(matches!(again, Err(AppError::ModelLoadFailed(m)) if m == expected));
}

#[test]
fn a_local_model_launches_with_its_own_saved_configuration_and_a_remote_one_with_none() {
    use lattice_core::application::contracts::settings::LocalModelConfigDto;
    let settings = LLMSettingsDto {
        local_context_window: Some(16_384),
        local_model_configs: [(
            "ornith".to_owned(),
            LocalModelConfigDto {
                context_length: Some(8_704),
                seed: Some(7),
                ..Default::default()
            },
        )]
        .into(),
        ..Default::default()
    };
    let local = |model_id: &str| {
        launch_profile_for(
            &ModelRef::Local {
                model_id: model_id.into(),
            },
            &settings,
        )
    };

    let ornith = local("ornith").expect("a local model is launched");
    assert_eq!((ornith.context_window, ornith.seed), (Some(8_704), Some(7)));
    assert_eq!(
        local("other").map(|launch| launch.context_window),
        Some(Some(16_384))
    );
    assert_eq!(
        launch_profile_for(
            &ModelRef::Ollama {
                model: "ornith".into()
            },
            &settings
        ),
        None
    );
}

#[test]
fn unusable_sidecar_is_not_reported_as_a_model_failure() {
    let mapped = unusable_sidecar(&LLMError::SidecarBinaryUnusable("broken".into()));
    assert!(matches!(
        mapped,
        Some(AppError::ModelServerCannotStart(m))
            if m == "The model server could not start: broken. Reinstall Lattice, or choose another model in Settings."
    ));
    assert!(unusable_sidecar(&LLMError::GenerationFailed("bad gguf".into())).is_none());
    assert!(unusable_sidecar(&LLMError::Timeout).is_none());
}

#[tokio::test]
async fn a_llama_cpp_model_uses_the_llama_cpp_connection_whatever_else_settings_name() {
    use lattice_core::application::{
        contracts::settings::{LLMProvider, LlamaCppSettingsDto},
        ports::llm_port::{CompletionInput, CompletionRequest},
    };
    use wiremock::{
        matchers::{header, method, path},
        Mock, MockServer, ResponseTemplate,
    };

    for provider in [
        LLMProvider::Auto,
        LLMProvider::Ollama,
        LLMProvider::Llamacpp,
    ] {
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
        // An uninitialized DB must never be queried for a remote model.
        let loader = loader_without_database();
        let settings = LLMSettingsDto {
            provider,
            ollama_url: server.uri(),
            model: "stale-ollama-model".into(),
            llama_cpp: LlamaCppSettingsDto {
                url: server.uri(),
                model: "stale-llama-cpp-model".into(),
                auth_header_name: "Authorization".into(),
                auth_header_value: "Basic test-credential".into(),
            },
            ..Default::default()
        };
        let llm = loader
            .load_ref(
                &ModelRef::LlamaCpp {
                    model: "test-model".into(),
                },
                &settings,
                Tuning::Standard,
            )
            .await
            .unwrap()
            .llm;
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
        assert_eq!(server.received_requests().await.unwrap().len(), 2);
    }
}

#[tokio::test]
async fn an_ollama_model_is_the_one_picked_not_the_settings_model() {
    use wiremock::{
        matchers::{method, path},
        Mock, MockServer, ResponseTemplate,
    };

    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/tags"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"models":[]})))
        .mount(&server)
        .await;
    let settings = LLMSettingsDto {
        provider: lattice_core::application::contracts::settings::LLMProvider::Ollama,
        ollama_url: server.uri(),
        model: "the-settings-model".into(),
        ..Default::default()
    };

    let llm = loader_without_database()
        .load_ref(
            &ModelRef::Ollama {
                model: "qwen3:8b".into(),
            },
            &settings,
            Tuning::Standard,
        )
        .await
        .unwrap()
        .llm;

    assert_eq!(llm.model_name(), "qwen3:8b");
}

#[tokio::test]
async fn missing_ollama_configuration_fails_before_network_access() {
    let loader = loader_without_database();
    let settings = LLMSettingsDto {
        provider: lattice_core::application::contracts::settings::LLMProvider::Ollama,
        ollama_url: String::new(),
        ..Default::default()
    };
    assert!(matches!(
        loader
            .load_ref(
                &ModelRef::Ollama {
                    model: "qwen3:8b".into()
                },
                &settings,
                Tuning::Standard,
            )
            .await,
        Err(AppError::InvalidConfig(_))
    ));
    let configured = LLMSettingsDto {
        ollama_url: "http://localhost:11434".into(),
        ..settings
    };
    assert!(matches!(
        loader
            .load_ollama(
                &configured,
                " ",
                GenerationConfig::from_settings(&configured)
            )
            .await,
        Err(AppError::InvalidConfig(_))
    ));
}

#[tokio::test]
async fn an_ollama_that_refuses_connections_is_an_outage_a_job_waits_out() {
    // A port that was just free: nothing listens there, so connecting is refused.
    let port = std::net::TcpListener::bind("127.0.0.1:0")
        .and_then(|listener| listener.local_addr())
        .map(|address| address.port())
        .unwrap();
    let settings = LLMSettingsDto {
        provider: lattice_core::application::contracts::settings::LLMProvider::Ollama,
        ollama_url: format!("http://127.0.0.1:{port}"),
        ..Default::default()
    };
    let loader = loader_without_database();
    let model = ModelRef::Ollama {
        model: "qwen3:8b".into(),
    };

    for _ in 0..2 {
        let error = loader
            .load_ref(&model, &settings, Tuning::Standard)
            .await
            .err();
        let Some(error @ AppError::Network(_)) = error else {
            panic!("expected a network outage, got {error:?}");
        };
        assert!(
            lattice_core::shared::runtime::jobs::JobFailure::is_outage(&error),
            "{error}"
        );
        assert!(
            error.to_string().contains("Make sure Ollama is running"),
            "{error}"
        );
    }
}

fn claude() -> ModelRef {
    ModelRef::Cloud {
        provider: CloudProvider::Anthropic,
        model: "claude-haiku-5-5".into(),
    }
}

const CLAUDE_REPORTS: ModelWindowDto = ModelWindowDto {
    context_window: Some(1_000_000),
    max_output_tokens: Some(128_000),
};

/// A saved `max_tokens` of 131,072 is past what Claude accepts, and a role that
/// picks a model never set it: the provider's ceiling decides. The window
/// stays at the default until someone chooses a larger one.
#[test]
fn a_cloud_model_keeps_the_default_window_under_its_providers_ceiling() {
    let settings = LLMSettingsDto {
        max_tokens: 131_072,
        ..Default::default()
    };
    let (connection, _) = cloud_connection(&settings, &claude(), Tuning::Standard, &CLAUDE_REPORTS);
    assert_eq!(connection.context_window, settings.context_window);
    assert_eq!(connection.max_tokens, 128_000);
}

#[test]
fn a_cloud_model_runs_at_what_was_chosen_for_it() {
    let mut settings = LLMSettingsDto::default();
    settings.model_windows.insert(
        claude().to_string(),
        ModelWindowDto {
            context_window: Some(64_000),
            max_output_tokens: Some(4_096),
        },
    );
    let (connection, _) = cloud_connection(&settings, &claude(), Tuning::Standard, &CLAUDE_REPORTS);
    assert_eq!(connection.context_window, 64_000);
    assert_eq!(connection.max_tokens, 4_096);
}

#[test]
fn a_model_nobody_reports_on_runs_at_the_settings_numbers() {
    let settings = LLMSettingsDto {
        context_window: 90_000,
        max_tokens: 8_192,
        ..Default::default()
    };
    let unreported = ModelWindowDto::default();
    let (connection, known) = cloud_connection(&settings, &claude(), Tuning::Standard, &unreported);
    assert!(!known);
    assert_eq!(connection.context_window, 90_000);
    assert_eq!(connection.max_tokens, 8_192);
}

/// A role's ceiling is its own, and is not raised by the model's.
#[test]
fn the_utility_role_keeps_its_own_output_ceiling() {
    let mut settings = LLMSettingsDto::default();
    settings.model_windows.insert(
        claude().to_string(),
        ModelWindowDto {
            context_window: None,
            max_output_tokens: Some(64_000),
        },
    );
    let (connection, _) = cloud_connection(&settings, &claude(), Tuning::Utility, &CLAUDE_REPORTS);
    assert_eq!(connection.max_tokens, 8_192);
}

#[test]
fn a_bundled_server_that_cannot_run_is_an_install_problem_the_learner_can_act_on() {
    use lattice_core::shared::runtime::jobs::JobFailure;
    let fault = "llama-server.exe was not found";
    let expected = "The model server could not start: llama-server.exe was not found. \
        Reinstall Lattice, or choose another model in Settings.";
    let error = LLMError::SidecarBinaryUnusable(fault.into());

    let mapped = unusable_sidecar(&error).expect("an unusable sidecar is reported as such");
    let converted = AppError::from(error);

    for error in [mapped, converted] {
        assert!(error.to_string().contains(expected), "{error}");
        assert!(
            !JobFailure::is_transient(&error),
            "waiting does not bring a missing server back: {error}"
        );
    }
}

#[test]
fn a_server_that_is_starting_or_unreachable_stays_transient() {
    use lattice_core::shared::runtime::jobs::JobFailure;
    for error in [
        LLMError::ClientUnavailable("llama-server is still starting".into()),
        LLMError::Network("connection reset".into()),
    ] {
        assert!(JobFailure::is_transient(&AppError::from(error)));
    }
}

#[test]
fn a_bundled_server_that_cannot_run_reaches_the_wire_as_a_model_load_error() {
    use lattice_core::shared::ipc::{ApiError, ErrorCode};
    let error = AppError::from(LLMError::SidecarBinaryUnusable(
        "llama-server.exe was not found".into(),
    ));
    let wire = ApiError::from(error);
    assert_eq!(wire.code, ErrorCode::ModelLoadError);
    assert!(
        wire.details
            .as_deref()
            .is_some_and(|details| details.contains(
                "The model server could not start: llama-server.exe was not found. \
             Reinstall Lattice, or choose another model in Settings."
            )),
        "{wire:?}"
    );
}

mod output_ceiling_on_the_wire {
    use super::*;
    use lattice_core::application::ports::llm_port::{CompletionInput, CompletionRequest};
    use lattice_core::application::ports::LLMPort;
    use serde_json::{json, Value};
    use wiremock::matchers::method;
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn model(provider: CloudProvider) -> ModelRef {
        ModelRef::Cloud {
            provider,
            model: "some-model".into(),
        }
    }

    /// The body a scheduled client built from `settings` sends for `request`,
    /// as a stub server records it.
    async fn sent_body(
        provider: CloudProvider,
        settings: &LLMSettingsDto,
        reported: &ModelWindowDto,
        request: CompletionRequest,
    ) -> Value {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(400))
            .mount(&server)
            .await;
        let model = model(provider);
        let client = cloud_client(
            settings,
            &model,
            Tuning::Standard,
            reported,
            "test-key".into(),
        )
        .unwrap()
        .with_endpoint(server.uri())
        .schedule(&crate::features::llm::scheduler::SchedulerRegistry::default());
        let _ = client.complete(&request).await;
        let received = server.received_requests().await.unwrap();
        serde_json::from_slice(&received[0].body).unwrap()
    }

    fn ask() -> CompletionRequest {
        CompletionRequest {
            input: vec![CompletionInput::Message {
                role: "user".into(),
                content: "When did the bridge open?".into(),
            }],
            ..Default::default()
        }
    }

    fn chosen(max_output_tokens: u32) -> LLMSettingsDto {
        let mut settings = LLMSettingsDto::default();
        settings.model_windows.insert(
            model(CloudProvider::Openai).to_string(),
            ModelWindowDto {
                context_window: None,
                max_output_tokens: Some(max_output_tokens),
            },
        );
        settings.model_windows.insert(
            model(CloudProvider::Google).to_string(),
            ModelWindowDto {
                context_window: None,
                max_output_tokens: Some(max_output_tokens),
            },
        );
        settings
    }

    #[tokio::test]
    async fn an_openai_request_sends_no_ceiling_when_none_is_known() {
        let body = sent_body(
            CloudProvider::Openai,
            &LLMSettingsDto::default(),
            &ModelWindowDto::default(),
            ask(),
        )
        .await;
        assert!(body.get("max_output_tokens").is_none(), "{body}");
    }

    #[tokio::test]
    async fn an_openai_request_keeps_a_chosen_ceiling() {
        let body = sent_body(
            CloudProvider::Openai,
            &chosen(4_096),
            &ModelWindowDto::default(),
            ask(),
        )
        .await;
        assert_eq!(body["max_output_tokens"], json!(4_096));
    }

    #[tokio::test]
    async fn an_openai_request_keeps_a_reported_ceiling() {
        let body = sent_body(
            CloudProvider::Openai,
            &LLMSettingsDto::default(),
            &ModelWindowDto {
                context_window: None,
                max_output_tokens: Some(16_384),
            },
            ask(),
        )
        .await;
        assert_eq!(body["max_output_tokens"], json!(16_384));
    }

    #[tokio::test]
    async fn a_google_request_sends_no_ceiling_when_none_is_known() {
        let body = sent_body(
            CloudProvider::Google,
            &LLMSettingsDto::default(),
            &ModelWindowDto::default(),
            ask(),
        )
        .await;
        assert!(body.get("max_tokens").is_none(), "{body}");
    }

    #[tokio::test]
    async fn a_google_request_keeps_a_chosen_ceiling() {
        let body = sent_body(
            CloudProvider::Google,
            &chosen(4_096),
            &ModelWindowDto::default(),
            ask(),
        )
        .await;
        assert_eq!(body["max_tokens"], json!(4_096));
    }

    #[tokio::test]
    async fn a_request_ceiling_is_sent_even_when_the_model_reports_none() {
        let request = CompletionRequest {
            max_output_tokens: Some(512),
            ..ask()
        };
        let openai = sent_body(
            CloudProvider::Openai,
            &LLMSettingsDto::default(),
            &ModelWindowDto::default(),
            request.clone(),
        )
        .await;
        assert_eq!(openai["max_output_tokens"], json!(512));
        let google = sent_body(
            CloudProvider::Google,
            &LLMSettingsDto::default(),
            &ModelWindowDto::default(),
            request,
        )
        .await;
        assert_eq!(google["max_tokens"], json!(512));
    }

    fn small_window() -> LLMSettingsDto {
        LLMSettingsDto {
            context_window: 32_768,
            ..Default::default()
        }
    }

    #[tokio::test]
    async fn an_openai_request_with_a_small_window_still_sends_no_ceiling() {
        let body = sent_body(
            CloudProvider::Openai,
            &small_window(),
            &ModelWindowDto::default(),
            ask(),
        )
        .await;
        assert!(body.get("max_output_tokens").is_none(), "{body}");
    }

    #[tokio::test]
    async fn a_google_request_with_a_small_window_still_sends_no_ceiling() {
        let body = sent_body(
            CloudProvider::Google,
            &small_window(),
            &ModelWindowDto::default(),
            ask(),
        )
        .await;
        assert!(body.get("max_tokens").is_none(), "{body}");
    }

    #[tokio::test]
    async fn an_openai_request_above_the_room_is_sent_the_room() {
        let request = CompletionRequest {
            max_output_tokens: Some(1_000_000),
            ..ask()
        };
        let body = sent_body(
            CloudProvider::Openai,
            &small_window(),
            &ModelWindowDto::default(),
            request,
        )
        .await;
        let sent = body["max_output_tokens"].as_u64().unwrap();
        assert!(sent > 0 && sent < 32_768, "{body}");
    }

    #[tokio::test]
    async fn an_anthropic_request_with_a_small_window_sends_a_ceiling_that_fits_it() {
        let body = sent_body(
            CloudProvider::Anthropic,
            &small_window(),
            &ModelWindowDto::default(),
            ask(),
        )
        .await;
        let sent = body["max_tokens"].as_u64().unwrap();
        assert!(sent > 0 && sent <= 32_768, "{body}");
    }

    #[tokio::test]
    async fn an_anthropic_request_still_sends_its_ceiling() {
        let body = sent_body(
            CloudProvider::Anthropic,
            &LLMSettingsDto::default(),
            &ModelWindowDto::default(),
            ask(),
        )
        .await;
        assert_eq!(body["max_tokens"], json!(128_000));
    }
}
