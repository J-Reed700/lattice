//! Opens local, Ollama, and explicit cloud models; fallback policy belongs to the application.
use std::sync::Arc;

use crate::application::contracts::settings::LLMSettingsDto;
use crate::application::ports::LLMPort;
use crate::features::llm::engine::types::LLMError;
use crate::infrastructure::persistence::repositories::DownloadedModelRepository;
use crate::shared::error::{AppError, Result};

/// A bundled llama-server that cannot execute is an install problem, not a
/// model problem. It must not be reported as a corrupt model (a failed chat
/// warmup deactivates such models) nor quietly replaced by another model.
fn unusable_sidecar(error: &LLMError) -> Option<AppError> {
    match error {
        LLMError::SidecarBinaryUnusable(message) => {
            Some(AppError::ServiceNotAvailable(message.clone()))
        }
        _ => None,
    }
}

/// Artifacts whose llama-server refused to start, keyed by path and stamped
/// with the file's size and mtime.
///
/// Starting a local model means launching a sidecar and reading several
/// gigabytes of tensors, and a file llama-server cannot parse fails only after
/// paying all of that. The utility role retries on every single turn, so one
/// unsupported GGUF taxes every message with a doomed load before falling back
/// to the chat LLM. Remembering the failure makes the second turn cheap.
///
/// The stamp is what allows recovery without a restart: a re-downloaded or
/// repaired file has a different size or mtime, so it is a different key and
/// gets a fresh attempt.
static UNLOADABLE_ARTIFACTS: once_cell::sync::Lazy<
    std::sync::Mutex<std::collections::HashMap<std::path::PathBuf, ArtifactStamp>>,
> = once_cell::sync::Lazy::new(|| std::sync::Mutex::new(std::collections::HashMap::new()));

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct ArtifactStamp {
    len: u64,
    modified: Option<std::time::SystemTime>,
}

fn artifact_stamp(path: &std::path::Path) -> Option<ArtifactStamp> {
    let metadata = std::fs::metadata(path).ok()?;
    Some(ArtifactStamp {
        len: metadata.len(),
        modified: metadata.modified().ok(),
    })
}

/// True when this exact file has already failed to start in this session.
fn is_known_unloadable(path: &std::path::Path) -> bool {
    let Some(stamp) = artifact_stamp(path) else {
        return false;
    };
    let mut known = match UNLOADABLE_ARTIFACTS.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    };
    match known.get(path) {
        Some(recorded) if *recorded == stamp => true,
        // The file changed since it failed. Drop the stale entry so the new
        // bytes are judged on their own.
        Some(_) => {
            known.remove(path);
            false
        }
        None => false,
    }
}

fn remember_unloadable(path: &std::path::Path) {
    let Some(stamp) = artifact_stamp(path) else {
        return;
    };
    let mut known = match UNLOADABLE_ARTIFACTS.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    };
    known.insert(path.to_path_buf(), stamp);
}

pub(crate) struct ModelLoader {
    downloaded_models: Arc<DownloadedModelRepository>,
    app_handle: Option<tauri::AppHandle>,
}

impl ModelLoader {
    pub(crate) fn new(
        downloaded_models: Arc<DownloadedModelRepository>,
        app_handle: Option<tauri::AppHandle>,
    ) -> Self {
        Self {
            downloaded_models,
            app_handle,
        }
    }

    pub(crate) async fn load(&self, settings: &LLMSettingsDto) -> Result<Arc<dyn LLMPort>> {
        if settings.provider == crate::application::contracts::settings::LLMProvider::Llamacpp {
            return Ok(Arc::new(crate::features::llm::llama_cpp::LlamaCppLlm::new(
                settings,
            )?));
        }
        if matches!(
            settings.provider,
            crate::application::contracts::settings::LLMProvider::Openai
                | crate::application::contracts::settings::LLMProvider::Anthropic
        ) {
            let name = if settings.provider
                == crate::application::contracts::settings::LLMProvider::Openai
            {
                "openai"
            } else {
                "anthropic"
            };
            let storage = crate::infrastructure::security::keyring_storage::SecureStorage::new();
            let key = match std::env::var(format!("{}_API_KEY", name.to_uppercase())).ok() {
                Some(key) => Some(key),
                None => storage.get_api_key(name)?,
            }
            .filter(|k| !k.trim().is_empty())
            .ok_or_else(|| {
                AppError::InvalidConfig(format!("Configure the {name} API key in Chat settings"))
            })?;
            return Ok(Arc::new(crate::features::llm::cloud::CloudLlm::new(
                settings, key,
            )?));
        }
        let config = Self::generation_config_from_settings(settings);
        crate::application::services::model_selection::select_model(
            settings.provider,
            || self.try_load_active_model(config.clone(), settings.local_context_window),
            || self.try_load_llama_cpp(settings),
            || self.try_load_ollama(settings, config.clone()),
        )
        .await
    }

    async fn try_load_llama_cpp(
        &self,
        settings: &LLMSettingsDto,
    ) -> Result<Option<Arc<dyn LLMPort>>> {
        if settings.llama_cpp.model.trim().is_empty() {
            return Ok(None);
        }
        let llm = crate::features::llm::llama_cpp::LlamaCppLlm::new(settings)?;
        if llm.is_ready().await? {
            Ok(Some(Arc::new(llm)))
        } else {
            Ok(None)
        }
    }

    pub(crate) async fn load_router_llm(
        &self,
        settings: &crate::application::contracts::settings::LLMSettingsDto,
        model_name: &str,
    ) -> Result<Arc<dyn LLMPort>> {
        let router_settings = &settings.router;
        let generation_config = crate::features::llm::engine::GenerationConfig {
            temperature: router_settings.temperature,
            top_p: settings.top_p,
            top_k: settings.top_k,
            max_tokens: router_settings.max_tokens as usize,
            repeat_penalty: settings.repeat_penalty,
        };

        // 1) Try local downloaded model by ID (if present).
        if let Some(local_llm) = self
            .try_load_router_local_model(
                model_name,
                generation_config.clone(),
                settings.local_context_window,
            )
            .await?
        {
            return Ok(local_llm);
        }

        // 2) If no local router exists, use the explicitly configured provider.
        let loaded = match settings.provider {
            crate::application::contracts::settings::LLMProvider::Llamacpp => {
                let mut router = settings.clone();
                router.llama_cpp.model = model_name.to_owned();
                router.max_tokens = router_settings.max_tokens;
                router.temperature = router_settings.temperature;
                return self.load(&router).await;
            }
            crate::application::contracts::settings::LLMProvider::Openai
            | crate::application::contracts::settings::LLMProvider::Anthropic => {
                let mut router = settings.clone();
                router.model = model_name.to_owned();
                router.max_tokens = router_settings.max_tokens;
                router.temperature = router_settings.temperature;
                return self.load(&router).await;
            }
            crate::application::contracts::settings::LLMProvider::Local => {
                return Err(AppError::AiModelsNotInstalled(format!(
                    "Router model '{}' is not downloaded. Download it in Settings → Model Catalog.",
                    model_name
                )));
            }
            crate::application::contracts::settings::LLMProvider::Auto => {
                let mut router = settings.clone();
                router.llama_cpp.model = model_name.to_owned();
                router.max_tokens = router_settings.max_tokens;
                router.temperature = router_settings.temperature;
                crate::application::services::model_selection::select_model(
                    settings.provider,
                    || async { Ok(None) }, // Local router lookup already ran above.
                    || async {
                        if settings.llama_cpp.model.trim().is_empty() {
                            return Ok(None);
                        }
                        self.try_load_llama_cpp(&router).await
                    },
                    || self.try_load_ollama_with_model(settings, model_name, generation_config),
                ).await?
            }
            crate::application::contracts::settings::LLMProvider::Ollama => {
                self.try_load_ollama_with_model(settings, model_name, generation_config)
                    .await
                    .map_err(|e| {
                        AppError::AiModelsNotInstalled(format!(
                            "Router model '{}' is not available locally and Ollama failed: {}. \
                             Download the router model in Settings → Model Catalog or configure Ollama.",
                            model_name, e
                        ))
                    })?
            }
        };

        Ok(loaded)
    }

    async fn try_load_router_local_model(
        &self,
        model_id: &str,
        generation_config: crate::features::llm::engine::GenerationConfig,
        context_window: Option<u32>,
    ) -> Result<Option<Arc<dyn LLMPort>>> {
        use crate::features::llm::engine::factory::{create_llm, LLMConfig};

        let maybe_model = self.downloaded_models.find_by_model_id(model_id).await?;

        let model = match maybe_model {
            Some(model) => model,
            None => return Ok(None),
        };

        if let Err(reason) = model.validate_for_operation(true) {
            return Err(AppError::InvalidConfig(reason));
        }

        let model_path = match model.loadable_path() {
            Some(p) => p.to_path_buf(),
            None => return Ok(None),
        };
        if !model_path.exists() {
            return Ok(None);
        }

        tracing::info!(
            router_model = %model.model_name(),
            path = %model_path.display(),
            "Loading router model from local GGUF"
        );

        let llm_config = LLMConfig::Local {
            model_path,
            n_gpu_layers: -1,
            generation_config,
            // Populated when the app boots via Container::with_app_handle.
            // None in tests / sidecar-feature-off builds (where it's unused).
            app_handle: self.app_handle.clone(),
            context_window,
        };

        match create_llm(llm_config).await {
            Ok(llm) => Ok(Some(llm)),
            Err(e) => Err(unusable_sidecar(&e).unwrap_or_else(|| {
                AppError::ModelLoadFailed(format!(
                    "Failed to load router model '{}': {}",
                    model.model_name(),
                    e
                ))
            })),
        }
    }

    pub(crate) async fn load_utility_local(
        &self,
        active: &crate::domain::DownloadedModel,
        generation_config: crate::features::llm::engine::GenerationConfig,
        context_window: Option<u32>,
    ) -> Result<Option<Arc<dyn LLMPort>>> {
        let model_id = active.model_id();
        let model_path = match active.loadable_path() {
            Some(p) => p.to_path_buf(),
            None => {
                tracing::warn!(
                    model_id = %model_id,
                    "Active utility model has no loadable path — skipping"
                );
                return Ok(None);
            }
        };
        if !model_path.exists() {
            tracing::warn!(
                model_id = %model_id,
                path = %model_path.display(),
                "Active utility model artifact missing on disk — falling back to chat LLM"
            );
            return Ok(None);
        }

        if is_known_unloadable(&model_path) {
            tracing::debug!(
                model_id = %model_id,
                path = %model_path.display(),
                "Utility model already failed to start in this session — using the chat LLM \
                 without retrying the load"
            );
            return Ok(None);
        }

        let llm_config = crate::features::llm::engine::factory::LLMConfig::Local {
            model_path: model_path.clone(),
            n_gpu_layers: -1,
            generation_config,
            app_handle: self.app_handle.clone(),
            context_window,
        };

        tracing::info!(
            model_id = %model_id,
            path = %model_path.display(),
            "Loading utility LLM"
        );

        match crate::features::llm::engine::factory::create_llm(llm_config).await {
            Ok(llm) => Ok(Some(llm)),
            Err(e) => match unusable_sidecar(&e) {
                // Surfaced as an error so warmup reports it on the model's
                // row; HyDE and the other utility callers still fall back to
                // the chat LLM on `Err`.
                Some(error) => {
                    tracing::error!(
                        model_id = %model_id,
                        error = %e,
                        "Utility LLM cannot start: the bundled llama-server is unusable; \
                         utility work falls back to the chat LLM"
                    );
                    Err(error)
                }
                None => {
                    // The sidecar itself works, so this file is the problem:
                    // an unsupported architecture, a truncated download, or
                    // one too large for this machine. Retrying it next turn
                    // costs the same minute and fails the same way.
                    remember_unloadable(&model_path);
                    tracing::warn!(
                        model_id = %model_id,
                        path = %model_path.display(),
                        error = %e,
                        "Failed to load utility LLM — falling back to chat LLM and not \
                         retrying this file until it changes"
                    );
                    Ok(None)
                }
            },
        }
    }

    /// Resolve a remote utility assignment the way the chat role resolves:
    /// the configured provider decides the wire protocol, not the label on
    /// the assigned row. Under `Auto` a configured llama.cpp connection is
    /// tried before Ollama, exactly like `load`, so the same host is never
    /// spoken to with two different protocols.
    pub(crate) async fn load_utility_remote(
        &self,
        settings: &LLMSettingsDto,
        model: &str,
        generation_config: crate::features::llm::engine::GenerationConfig,
    ) -> Result<Option<Arc<dyn LLMPort>>> {
        use crate::application::contracts::settings::LLMProvider;

        let mut utility = settings.clone();
        utility.model = model.to_owned();
        utility.llama_cpp.model = model.to_owned();
        utility.max_tokens = u32::try_from(generation_config.max_tokens).unwrap_or(u32::MAX);

        match settings.provider {
            LLMProvider::Local => Ok(None),
            LLMProvider::Llamacpp | LLMProvider::Openai | LLMProvider::Anthropic => {
                self.load(&utility).await.map(Some)
            }
            LLMProvider::Ollama => self
                .try_load_ollama_with_model(settings, model, generation_config)
                .await
                .map(Some),
            LLMProvider::Auto => {
                if !settings.llama_cpp.model.trim().is_empty() {
                    match self.try_load_llama_cpp(&utility).await {
                        Ok(Some(llm)) => return Ok(Some(llm)),
                        Ok(None) => tracing::debug!(
                            model,
                            "llama.cpp server does not list the utility model, trying Ollama"
                        ),
                        Err(error) => tracing::warn!(
                            model,
                            %error,
                            "llama.cpp not available for the utility role, trying Ollama"
                        ),
                    }
                }
                self.try_load_ollama_with_model(settings, model, generation_config)
                    .await
                    .map(Some)
            }
        }
    }

    fn generation_config_from_settings(
        settings: &LLMSettingsDto,
    ) -> crate::features::llm::engine::GenerationConfig {
        crate::features::llm::engine::GenerationConfig {
            temperature: settings.temperature,
            top_p: settings.top_p,
            top_k: settings.top_k,
            max_tokens: settings.max_tokens as usize,
            repeat_penalty: settings.repeat_penalty,
        }
    }

    async fn try_load_active_model(
        &self,
        generation_config: crate::features::llm::engine::GenerationConfig,
        context_window: Option<u32>,
    ) -> Result<Option<Arc<dyn LLMPort>>> {
        use crate::features::llm::engine::factory::{create_llm, LLMConfig};

        let active_model = match self.downloaded_models.get_active_chat_model().await {
            Ok(Some(model)) => model,
            Ok(None) => {
                tracing::debug!("No active model configured in database");
                return Ok(None);
            }
            Err(e) => {
                tracing::warn!("Failed to query active model from database: {}", e);
                return Ok(None);
            }
        };

        tracing::info!(
            "Found active model in database: {} ({})",
            active_model.model_name(),
            active_model.model_id()
        );

        let model_path = match active_model.loadable_path() {
            Some(p) => p.to_path_buf(),
            None => return Ok(None),
        };

        if !model_path.exists() {
            tracing::warn!(
                "Active model artifact not found: {} - user may have deleted it manually",
                model_path.display()
            );
            return Ok(None);
        }

        tracing::info!("Loading GGUF model from: {}", model_path.display());

        // Attempt to load the GGUF model
        let llm_config = LLMConfig::Local {
            model_path: model_path.clone(),
            n_gpu_layers: -1, // Use Metal GPU on M-series Macs (all layers on GPU)
            generation_config,
            // Populated when the app boots via Container::with_app_handle.
            // None in tests / sidecar-feature-off builds (where it's unused).
            app_handle: self.app_handle.clone(),
            context_window,
        };

        match create_llm(llm_config).await {
            Ok(llm) => {
                tracing::info!(
                    "✅ Successfully loaded active model: {}",
                    active_model.model_name()
                );
                Ok(Some(llm))
            }
            Err(e) => {
                tracing::error!(
                    "Failed to load active model {}: {}",
                    active_model.model_name(),
                    e
                );

                if let Some(error) = unusable_sidecar(&e) {
                    return Err(error);
                }
                Err(AppError::ModelLoadFailed(format!(
                    "Failed to load model '{}': {}. \
                     The model file may be corrupted or incompatible. \
                     Try downloading a different model from Settings → Model Catalog.",
                    active_model.model_name(),
                    e
                )))
            }
        }
    }

    /// Try to load Ollama client from config
    ///
    /// Returns Ok(llm) if Ollama is configured and available
    /// Returns Err if config missing, invalid, or Ollama not reachable
    async fn try_load_ollama(
        &self,
        llm_settings: &LLMSettingsDto,
        generation_config: crate::features::llm::engine::GenerationConfig,
    ) -> Result<Arc<dyn LLMPort>> {
        use crate::features::llm::engine::factory::create_ollama_llm;

        let endpoint = llm_settings.ollama_url.clone();
        let model = llm_settings.model.clone();
        let auth_header_name = llm_settings.ollama_auth_header_name.trim();
        let auth_header_value = llm_settings.ollama_auth_header_value.trim();
        let auth_header = if !auth_header_name.is_empty() && !auth_header_value.is_empty() {
            Some((auth_header_name.to_string(), auth_header_value.to_string()))
        } else {
            None
        };

        if endpoint.is_empty() || model.is_empty() {
            return Err(AppError::InvalidConfig(
                "Ollama endpoint or model not configured".to_string(),
            ));
        }

        tracing::info!(
            "Attempting to connect to Ollama at {} with model {}",
            endpoint,
            model
        );

        // Try to create Ollama client (includes health check)
        match create_ollama_llm(&endpoint, &model, auth_header, generation_config).await {
            Ok(llm) => {
                tracing::info!("✅ Successfully connected to Ollama server");
                Ok(llm)
            }
            Err(e) => {
                tracing::warn!("Failed to connect to Ollama: {}", e);
                Err(AppError::ServiceNotAvailable(format!(
                    "Ollama server not available at {}: {}. \
                     Make sure Ollama is running (ollama serve) and the model '{}' is pulled (ollama pull {}).",
                    endpoint, e, model, model
                )))
            }
        }
    }

    /// Try to load Ollama client with an explicit model override.
    pub(crate) async fn try_load_ollama_with_model(
        &self,
        llm_settings: &LLMSettingsDto,
        model: &str,
        generation_config: crate::features::llm::engine::GenerationConfig,
    ) -> Result<Arc<dyn LLMPort>> {
        use crate::features::llm::engine::factory::create_ollama_llm;

        let endpoint = llm_settings.ollama_url.clone();
        let auth_header_name = llm_settings.ollama_auth_header_name.trim();
        let auth_header_value = llm_settings.ollama_auth_header_value.trim();
        let auth_header = if !auth_header_name.is_empty() && !auth_header_value.is_empty() {
            Some((auth_header_name.to_string(), auth_header_value.to_string()))
        } else {
            None
        };

        if endpoint.is_empty() || model.trim().is_empty() {
            return Err(AppError::InvalidConfig(
                "Ollama endpoint or router model not configured".to_string(),
            ));
        }

        tracing::info!(
            "Attempting to connect to Ollama at {} with router model {}",
            endpoint,
            model
        );

        match create_ollama_llm(&endpoint, model, auth_header, generation_config).await {
            Ok(llm) => {
                tracing::info!("✅ Successfully connected to Ollama router model");
                Ok(llm)
            }
            Err(e) => {
                tracing::warn!("Failed to connect to Ollama router model: {}", e);
                Err(AppError::ServiceNotAvailable(format!(
                    "Ollama router model '{}' not available at {}: {}",
                    model, endpoint, e
                )))
            }
        }
    }
}

#[cfg(test)]
mod tests {
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
    }

    #[test]
    fn a_file_that_failed_to_start_is_retried_only_after_it_changes() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("model.gguf");
        std::fs::write(&path, b"broken").expect("write");

        assert!(!is_known_unloadable(&path));
        remember_unloadable(&path);
        assert!(is_known_unloadable(&path));

        // A repaired or re-downloaded file is different bytes, so the memo
        // must not keep a working model from ever being tried again.
        std::fs::write(&path, b"repaired and longer").expect("rewrite");
        assert!(!is_known_unloadable(&path));
    }

    #[test]
    fn unusable_sidecar_is_not_reported_as_a_model_failure() {
        let mapped = unusable_sidecar(&LLMError::SidecarBinaryUnusable("broken".into()));
        assert!(matches!(mapped, Some(AppError::ServiceNotAvailable(m)) if m == "broken"));
        assert!(unusable_sidecar(&LLMError::GenerationFailed("bad gguf".into())).is_none());
        assert!(unusable_sidecar(&LLMError::Timeout).is_none());
    }

    #[tokio::test]
    async fn auto_and_explicit_llamacpp_use_their_connection_despite_stale_ollama_and_local_selections(
    ) {
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
            .respond_with(ResponseTemplate::new(200)
                .insert_header("content-type", "text/event-stream")
                .set_body_string(concat!(
                    "data: {\"choices\":[{\"delta\":{\"content\":\"OK\"},\"finish_reason\":null}]}\n\n",
                    "data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}]}\n\n",
                    "data: [DONE]\n\n"
                )))
            .expect(1)
            .mount(&server).await;
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
            // An uninitialized DB must never be queried for an explicit remote provider.
            let pool = sqlx::sqlite::SqlitePoolOptions::new()
                .connect_lazy("sqlite::memory:")
                .unwrap();
            let loader = ModelLoader::new(Arc::new(DownloadedModelRepository::new(pool)), None);
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
                if provider == LLMProvider::Auto { 2 } else { 1 }
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
        let loader = ModelLoader::new(Arc::new(DownloadedModelRepository::new(pool)), None);
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
            .respond_with(
                ResponseTemplate::new(200).set_body_json(serde_json::json!({"models":[]})),
            )
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
        let loader = ModelLoader::new(Arc::new(DownloadedModelRepository::new(pool)), None);
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
        let loader = ModelLoader::new(Arc::new(DownloadedModelRepository::new(pool)), None);
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
}
