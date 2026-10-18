use super::*;
use crate::features::llm::instances::SidecarBudget;
use crate::features::llm::loading::LlmRuntime;
use crate::infrastructure::persistence::repositories::downloaded_models::DownloadedModelRepository;
use lattice_core::application::contracts::settings::{LlamaCppSettingsDto, SettingsDto};
use lattice_core::application::ports::MockSettingsRepository;
use lattice_core::domain::models::downloaded::ModelLocation;
use lattice_core::domain::DownloadedModel;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// Role models over a migrated in-memory database and `settings`, with the
/// row `utility` active for the utility role when given.
async fn role_models(settings: LLMSettingsDto, utility: Option<DownloadedModel>) -> RoleModels {
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    sqlx::migrate!("./migrations").run(&pool).await.unwrap();
    let models = Arc::new(DownloadedModelRepository::new(pool));
    if let Some(row) = utility {
        models.save(&row).await.unwrap();
        models
            .set_active_utility_model(row.model_id())
            .await
            .unwrap();
    }
    RoleModels {
        instances: Arc::new(LlmInstances::new()),
        loader: Arc::new(ModelLoader::new(
            models.clone(),
            models,
            None,
            LlmRuntime::default(),
        )),
        budget: Arc::new(SidecarBudget::new(None, Arc::default())),
        settings: Arc::new(MockSettingsRepository::with_settings(SettingsDto {
            llm: settings,
            ..SettingsDto::default()
        })),
    }
}

/// An Ollama server that has every model.
async fn ollama() -> MockServer {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/tags"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"models":[]})))
        .mount(&server)
        .await;
    server
}

/// A llama.cpp server that lists `models`.
async fn llama_cpp(models: &[&str]) -> MockServer {
    let server = MockServer::start().await;
    let data: Vec<_> = models
        .iter()
        .map(|id| serde_json::json!({"id": id}))
        .collect();
    Mock::given(method("GET"))
        .and(path("/v1/models"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"data": data})))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/props"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(serde_json::json!({"total_slots": 1})),
        )
        .mount(&server)
        .await;
    server
}

fn on_ollama(server: &MockServer, model: &str) -> LLMSettingsDto {
    LLMSettingsDto {
        provider: LLMProvider::Ollama,
        ollama_url: server.uri(),
        model: model.into(),
        ..LLMSettingsDto::default()
    }
}

fn chat(choice: ChatModelChoice, request_override: Option<ModelRef>) -> ModelRequest {
    ModelRequest {
        role: ModelRole::Chat,
        selection: ModelSelection::Chat {
            choice,
            request_override,
        },
        requirements: ModelRequirements::default(),
        work: "Research notes".into(),
        conversation_id: Some("c1".into()),
    }
}

fn ollama_model(model: &str) -> ModelRef {
    ModelRef::Ollama {
        model: model.into(),
    }
}

#[tokio::test]
async fn a_chat_runs_on_its_override_before_its_choice_before_the_default() {
    let server = ollama().await;
    let models = role_models(on_ollama(&server, "default-model"), None).await;
    let explicit = ChatModelChoice::Explicit {
        model: ollama_model("chosen"),
    };

    let default = models
        .llm_for(&chat(ChatModelChoice::Default, None))
        .await
        .unwrap();
    let chosen = models.llm_for(&chat(explicit.clone(), None)).await.unwrap();
    let once = models
        .llm_for(&chat(explicit, Some(ollama_model("once"))))
        .await
        .unwrap();

    let ran = |pinned: &PinnedModel| (pinned.llm.model_name().to_owned(), pinned.resolved.clone());
    assert_eq!(
        ran(&default),
        (
            "default-model".into(),
            ResolvedModel {
                model: ollama_model("default-model"),
                origin: ModelOrigin::Default
            }
        )
    );
    assert_eq!(
        ran(&chosen),
        (
            "chosen".into(),
            ResolvedModel {
                model: ollama_model("chosen"),
                origin: ModelOrigin::ChatOverride
            }
        )
    );
    assert_eq!(
        ran(&once),
        (
            "once".into(),
            ResolvedModel {
                model: ollama_model("once"),
                origin: ModelOrigin::RequestOverride
            }
        )
    );
}

#[tokio::test]
async fn a_chat_set_to_a_model_that_is_gone_is_refused_never_given_another() {
    let server = ollama().await;
    let models = role_models(on_ollama(&server, "default-model"), None).await;

    let refused = models
        .llm_for(&chat(
            ChatModelChoice::Explicit {
                model: ModelRef::Local {
                    model_id: "ornith-1.5-9b".into(),
                },
            },
            None,
        ))
        .await;

    assert!(
        matches!(refused, Err(AppError::AiModelsNotInstalled(message))
        if message == "This chat is set to ornith-1.5-9b, which is no longer downloaded.")
    );
    assert!(server.received_requests().await.unwrap().is_empty());
    assert!(models.instances.snapshot().is_empty());
}

#[tokio::test]
async fn two_roles_on_one_model_share_one_instance_and_each_is_listed_as_a_holder() {
    let server = ollama().await;
    let settings = LLMSettingsDto {
        learning_model: Some(ollama_model("default-model")),
        ..on_ollama(&server, "default-model")
    };
    let models = role_models(settings, None).await;
    let learning = ModelRequest {
        role: ModelRole::Learning,
        selection: ModelSelection::Learning,
        requirements: ModelRequirements::default(),
        work: String::new(),
        conversation_id: None,
    };

    let turn = models
        .llm_for(&chat(ChatModelChoice::Default, None))
        .await
        .unwrap();
    let job = models.llm_for(&learning).await.unwrap();

    assert_eq!(turn.instance_id, job.instance_id);
    let loaded = models.instances.snapshot();
    assert_eq!(loaded.len(), 1);
    assert_eq!(
        loaded[0].pinned_by,
        vec!["chat: Research notes".to_owned(), "learning".to_owned()]
    );
}

#[tokio::test]
async fn auto_moves_on_from_a_llama_cpp_server_that_does_not_list_the_model() {
    let llama = llama_cpp(&["some-other-model"]).await;
    let ollama = ollama().await;
    let settings = LLMSettingsDto {
        provider: LLMProvider::Auto,
        ollama_url: ollama.uri(),
        model: "qwen3:8b".into(),
        llama_cpp: LlamaCppSettingsDto {
            url: llama.uri(),
            model: "qwen3.5-27b".into(),
            ..LlamaCppSettingsDto::default()
        },
        ..LLMSettingsDto::default()
    };
    let models = role_models(settings, None).await;

    let pinned = models.llm_for(&ModelRequest::default_chat()).await.unwrap();

    assert_eq!(pinned.resolved.model, ollama_model("qwen3:8b"));
    assert_eq!(pinned.resolved.origin, ModelOrigin::Default);
}

#[tokio::test]
async fn work_that_needs_a_larger_window_than_the_model_has_is_refused() {
    let llama = llama_cpp(&["test-model"]).await;
    let settings = LLMSettingsDto {
        provider: LLMProvider::Llamacpp,
        learning_model: Some(ModelRef::LlamaCpp {
            model: "test-model".into(),
        }),
        context_window: 8_192,
        llama_cpp: LlamaCppSettingsDto {
            url: llama.uri(),
            model: "test-model".into(),
            ..LlamaCppSettingsDto::default()
        },
        ..LLMSettingsDto::default()
    };
    let models = role_models(settings, None).await;
    let learning = ModelRequest {
        role: ModelRole::Learning,
        selection: ModelSelection::Learning,
        requirements: ModelRequirements {
            min_context_tokens: Some(20_480),
            tools: ToolSupport::NotNeeded,
        },
        work: String::new(),
        conversation_id: None,
    };

    let refused = models.llm_for(&learning).await;

    assert!(
        matches!(refused, Err(AppError::InvalidConfig(message)) if message ==
        "Learning work needs a context window of at least 20,480 tokens. Your Learning model, \
         test-model, has a window of 8,192. Choose a larger Learning model.")
    );
    let loaded = models.instances.snapshot();
    assert_eq!(
        (loaded.len(), loaded[0].in_flight),
        (1, 0),
        "the model stays loaded for work it can do; the refused job holds nothing"
    );
    assert_eq!(
        models.instances.learning_shortfall(),
        Some(LearningShortfall {
            model: ModelRef::LlamaCpp {
                model: "test-model".into()
            },
            needed: 20_480,
            window: 8_192,
            window_is_saved: false,
        }),
        "Settings → Models can say what the refused work needed"
    );
}

fn remote_utility_row() -> DownloadedModel {
    DownloadedModel::new(
        "row-utility".into(),
        "Utility".into(),
        "utility-model".into(),
        ModelLocation::RemoteOllama,
        0,
        "GGUF".into(),
        None,
    )
    .unwrap()
}

#[tokio::test]
async fn under_auto_the_utility_model_speaks_llama_cpp_to_a_shared_host() {
    // One llama.cpp server is configured both as the llama.cpp connection and
    // as the "Ollama" URL. Under Auto the utility role must speak the protocol
    // chat speaks; llama.cpp serves none of Ollama's routes.
    let server = llama_cpp(&["chat-model", "utility-model"]).await;
    for route in ["/api/tags", "/api/generate", "/api/chat"] {
        Mock::given(path(route))
            .respond_with(ResponseTemplate::new(404))
            .expect(0)
            .mount(&server)
            .await;
    }
    let settings = LLMSettingsDto {
        provider: LLMProvider::Auto,
        ollama_url: server.uri(),
        model: "chat-model".into(),
        ollama_utility_model: "utility-model".into(),
        llama_cpp: LlamaCppSettingsDto {
            url: server.uri(),
            model: "chat-model".into(),
            ..LlamaCppSettingsDto::default()
        },
        ..LLMSettingsDto::default()
    };
    let models = role_models(settings, Some(remote_utility_row())).await;

    let llm = models
        .utility_llm()
        .await
        .unwrap()
        .expect("a utility model");

    assert_eq!(llm.provider_name(), "llamacpp");
    assert_eq!(llm.model_name(), "utility-model");
}

#[tokio::test]
async fn the_utility_model_uses_ollama_only_when_ollama_is_the_provider() {
    let server = ollama().await;
    Mock::given(path("/v1/models"))
        .respond_with(ResponseTemplate::new(404))
        .expect(0)
        .mount(&server)
        .await;
    let settings = LLMSettingsDto {
        ollama_utility_model: "utility-model".into(),
        llama_cpp: LlamaCppSettingsDto {
            url: server.uri(),
            model: "chat-model".into(),
            ..LlamaCppSettingsDto::default()
        },
        ..on_ollama(&server, "chat-model")
    };
    let models = role_models(settings, Some(remote_utility_row())).await;

    let llm = models
        .utility_llm()
        .await
        .unwrap()
        .expect("a utility model");

    assert_ne!(llm.provider_name(), "llamacpp");
    assert_eq!(llm.model_name(), "utility-model");
}

/// A machine on which Auto gives a local model `auto` tokens, and whose
/// header says it was trained for `trained`.
struct Windows {
    auto: u32,
    trained: u32,
}

#[async_trait::async_trait]
impl GpuBudget for Windows {
    async fn admission(
        &self,
        _: &std::path::Path,
        _: crate::features::llm::engine::factory::LocalServerOptions,
        _: Option<u32>,
    ) -> crate::features::llm::engine::sidecar_manager::Admission {
        crate::features::llm::engine::sidecar_manager::Admission::Unmetered
    }
    async fn auto_window(
        &self,
        _: &std::path::Path,
        _: crate::features::llm::engine::factory::LocalServerOptions,
    ) -> Option<u32> {
        Some(self.auto)
    }
    fn trained_window(&self, _: &std::path::Path) -> Option<u32> {
        Some(self.trained)
    }
}

#[tokio::test]
async fn a_floor_above_what_auto_can_reach_is_refused_before_loading() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("short.gguf");
    std::fs::write(&path, b"not a model").unwrap();
    let row = DownloadedModel::new(
        "row-short".into(),
        "Short-8K".into(),
        "short-8k".into(),
        ModelLocation::LocalFile { path },
        11,
        "GGUF".into(),
        None,
    )
    .unwrap();
    let settings = LLMSettingsDto {
        provider: LLMProvider::Local,
        learning_model: Some(ModelRef::Local {
            model_id: "short-8k".into(),
        }),
        ..LLMSettingsDto::default()
    };
    let mut models = role_models(settings, Some(row)).await;
    models.budget = Arc::new(Windows {
        auto: 8_192,
        trained: 8_192,
    });
    let learning = ModelRequest {
        role: ModelRole::Learning,
        selection: ModelSelection::Learning,
        requirements: ModelRequirements {
            min_context_tokens: Some(20_000),
            tools: ToolSupport::NotNeeded,
        },
        work: String::new(),
        conversation_id: None,
    };

    let refused = models.llm_for(&learning).await;

    let Err(AppError::InvalidConfig(message)) = refused else {
        panic!("expected a window refusal");
    };
    assert_eq!(
        message,
        "Learning work needs a context window of at least 20,000 tokens. Your Learning model, \
         Short-8K, was trained for 8,192 tokens, and Auto never goes past that. Set its context \
         window to 20,000 or more in Settings → Models (a stretched window costs some quality), \
         or choose a larger Learning model."
    );
    assert!(models.instances.snapshot().is_empty(), "nothing was loaded");
    assert_eq!(
        models.instances.learning_shortfall().map(|short| (
            short.needed,
            short.window,
            short.window_is_saved
        )),
        Some((20_000, 8_192, false))
    );
}

fn verifier() -> ModelRequest {
    ModelRequest {
        role: ModelRole::Verifier,
        selection: ModelSelection::Verifier,
        requirements: ModelRequirements::default(),
        work: String::new(),
        conversation_id: None,
    }
}

#[tokio::test]
async fn the_verifier_runs_on_its_own_model_and_refuses_unset_without_loading() {
    let server = ollama().await;
    let unset = role_models(on_ollama(&server, "default-model"), None).await;
    let refused = unset.llm_for(&verifier()).await;
    assert!(
        matches!(&refused, Err(AppError::InvalidConfig(message))
            if message == "No Verifier model is set. Choose one in Settings → Models under Verifier."),
        "never the chat default or the Learning model: {:?}",
        refused.err()
    );
    assert!(unset.instances.snapshot().is_empty());

    let settings = LLMSettingsDto {
        learning_model: Some(ollama_model("default-model")),
        verifier_model: Some(ollama_model("audit-model")),
        ..on_ollama(&server, "default-model")
    };
    let models = role_models(settings, None).await;
    let pinned = models.llm_for(&verifier()).await.unwrap();
    assert_eq!(pinned.resolved.model, ollama_model("audit-model"));
    assert_eq!(pinned.resolved.origin, ModelOrigin::VerifierRole);
    let loaded = models.instances.snapshot();
    assert_eq!(loaded.len(), 1);
    assert_eq!(loaded[0].pinned_by, vec!["verifier".to_owned()]);
}
