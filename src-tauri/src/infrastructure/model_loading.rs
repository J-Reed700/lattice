//! Opens local, Ollama, and explicit cloud models; fallback policy belongs to the application.
use std::sync::Arc;

use crate::application::contracts::settings::LLMSettingsDto;
use crate::application::ports::LLMPort;
use crate::infrastructure::persistence::repositories::DownloadedModelRepository;
use crate::shared::error::{AppError, Result};

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
            || self.try_load_active_model(config.clone()),
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
            .try_load_router_local_model(model_name, generation_config.clone())
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
        };

        match create_llm(llm_config).await {
            Ok(llm) => Ok(Some(llm)),
            Err(e) => Err(AppError::ModelLoadFailed(format!(
                "Failed to load router model '{}': {}",
                model.model_name(),
                e
            ))),
        }
    }

    pub(crate) async fn load_utility_local(
        &self,
        active: &crate::domain::DownloadedModel,
        generation_config: crate::features::llm::engine::GenerationConfig,
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

        let llm_config = crate::features::llm::engine::factory::LLMConfig::Local {
            model_path: model_path.clone(),
            n_gpu_layers: -1,
            generation_config,
            app_handle: self.app_handle.clone(),
        };

        tracing::info!(
            model_id = %model_id,
            path = %model_path.display(),
            "Loading utility LLM"
        );

        match crate::features::llm::engine::factory::create_llm(llm_config).await {
            Ok(llm) => Ok(Some(llm)),
            Err(e) => {
                tracing::warn!(
                    model_id = %model_id,
                    error = %e,
                    "Failed to load utility LLM — falling back to chat LLM"
                );
                Ok(None)
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
