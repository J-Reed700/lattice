//! Opens local, Ollama, and explicit cloud models; fallback policy belongs to the application.
use std::sync::Arc;

use crate::features::download::downloaded_model_repository::DownloadedModelRepository;
use crate::features::llm::engine::factory::LocalServerOptions;
use crate::features::llm::engine::system::BackendDevices;
use crate::features::llm::engine::types::LLMError;
use crate::features::llm::scheduler::SchedulerRegistry;
use lattice_core::application::contracts::settings::LLMSettingsDto;
use lattice_core::application::ports::LLMPort;
use lattice_core::shared::error::{AppError, Result};

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
#[derive(Default)]
pub(crate) struct UnloadableArtifacts {
    known: std::sync::Mutex<std::collections::HashMap<std::path::PathBuf, (ArtifactStamp, String)>>,
}

/// What every model load in one app shares: the per-backend schedulers, the
/// artifacts known not to start, and the probe of the devices a local server
/// can offload to. The container owns one and hands it to each loader, so two
/// containers never share any of it.
#[derive(Clone, Default)]
pub(crate) struct LlmRuntime {
    pub(crate) schedulers: Arc<SchedulerRegistry>,
    pub(crate) unloadable: Arc<UnloadableArtifacts>,
    pub(crate) devices: Arc<BackendDevices>,
}

impl LlmRuntime {
    /// The runtime for an app whose device probe is `devices`.
    pub(crate) fn new(devices: Arc<BackendDevices>) -> Self {
        Self {
            devices,
            ..Self::default()
        }
    }
}

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

impl UnloadableArtifacts {
    /// Preserve the failure as well as avoiding another expensive load.
    /// Returning `None` from the loader means no model was selected, which
    /// makes warmup claim success and hides the reason the selected utility
    /// model is not being used.
    fn known_failure(&self, path: &std::path::Path) -> Option<String> {
        let stamp = artifact_stamp(path)?;
        let mut known = match self.known.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        match known.get(path) {
            Some((recorded, error)) if *recorded == stamp => Some(error.clone()),
            // The file changed since it failed. Drop the stale entry so the new
            // bytes are judged on their own.
            Some(_) => {
                known.remove(path);
                None
            }
            None => None,
        }
    }

    fn remember(&self, path: &std::path::Path, error: String) {
        let Some(stamp) = artifact_stamp(path) else {
            return;
        };
        let mut known = match self.known.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        known.insert(path.to_path_buf(), (stamp, error));
    }
}

pub(crate) struct ModelLoader {
    downloaded_models: Arc<DownloadedModelRepository>,
    app_handle: Option<tauri::AppHandle>,
    runtime: LlmRuntime,
}

impl ModelLoader {
    pub(crate) fn new(
        downloaded_models: Arc<DownloadedModelRepository>,
        app_handle: Option<tauri::AppHandle>,
        runtime: LlmRuntime,
    ) -> Self {
        Self {
            downloaded_models,
            app_handle,
            runtime,
        }
    }

    async fn create_llm(
        &self,
        config: crate::features::llm::engine::factory::LLMConfig,
    ) -> std::result::Result<Arc<dyn LLMPort>, LLMError> {
        crate::features::llm::engine::factory::create_llm(
            config,
            &self.runtime.schedulers,
            &self.runtime.devices,
        )
        .await
    }

    pub(crate) async fn load(&self, settings: &LLMSettingsDto) -> Result<Arc<dyn LLMPort>> {
        if settings.provider
            == lattice_core::application::contracts::settings::LLMProvider::Llamacpp
        {
            return Ok(crate::features::llm::llama_cpp::LlamaCppLlm::new(settings)?
                .schedule(&self.runtime.schedulers)
                .await);
        }
        if matches!(
            settings.provider,
            lattice_core::application::contracts::settings::LLMProvider::Openai
                | lattice_core::application::contracts::settings::LLMProvider::Anthropic
                | lattice_core::application::contracts::settings::LLMProvider::Google
        ) {
            let name = match settings.provider {
                lattice_core::application::contracts::settings::LLMProvider::Openai => "openai",
                lattice_core::application::contracts::settings::LLMProvider::Anthropic => {
                    "anthropic"
                }
                lattice_core::application::contracts::settings::LLMProvider::Google => "google",
                _ => unreachable!(),
            };
            let storage = crate::infrastructure::security::keyring_storage::SecureStorage::new();
            let env_key = if name == "google" {
                std::env::var("GOOGLE_API_KEY")
                    .or_else(|_| std::env::var("GEMINI_API_KEY"))
                    .ok()
            } else {
                std::env::var(format!("{}_API_KEY", name.to_uppercase())).ok()
            };
            let key = match env_key {
                Some(key) => Some(key),
                None => storage.get_api_key(name)?,
            }
            .filter(|k| !k.trim().is_empty())
            .ok_or_else(|| {
                AppError::InvalidConfig(format!("Configure the {name} API key in Chat settings"))
            })?;
            return Ok(crate::features::llm::cloud::CloudLlm::new(settings, key)?
                .schedule(&self.runtime.schedulers));
        }
        let config = Self::generation_config_from_settings(settings);
        lattice_core::application::services::model_selection::select_model(
            settings.provider,
            || self.try_load_active_model(config.clone(), settings),
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
            Ok(Some(llm.schedule(&self.runtime.schedulers).await))
        } else {
            Ok(None)
        }
    }

    pub(crate) async fn load_router_llm(
        &self,
        settings: &lattice_core::application::contracts::settings::LLMSettingsDto,
        model_name: &str,
    ) -> Result<Arc<dyn LLMPort>> {
        let router_settings = &settings.router;
        let generation_config = crate::features::llm::engine::GenerationConfig {
            temperature: router_settings.temperature,
            max_tokens: router_settings.max_tokens as usize,
            ..crate::features::llm::engine::GenerationConfig::from_settings(settings)
        };

        // 1) Try local downloaded model by ID (if present).
        if let Some(local_llm) = self
            .try_load_router_local_model(
                model_name,
                generation_config.clone(),
                LocalServerOptions::from_settings(settings)
                    .with_model_config(settings.local_model_configs.get(model_name)),
            )
            .await?
        {
            return Ok(local_llm);
        }

        // 2) If no local router exists, use the explicitly configured provider.
        let loaded = match settings.provider {
            lattice_core::application::contracts::settings::LLMProvider::Llamacpp => {
                let mut router = settings.clone();
                router.llama_cpp.model = model_name.to_owned();
                router.max_tokens = router_settings.max_tokens;
                router.temperature = router_settings.temperature;
                return self.load(&router).await;
            }
            lattice_core::application::contracts::settings::LLMProvider::Openai
            | lattice_core::application::contracts::settings::LLMProvider::Anthropic
            | lattice_core::application::contracts::settings::LLMProvider::Google => {
                let mut router = settings.clone();
                router.model = model_name.to_owned();
                router.max_tokens = router_settings.max_tokens;
                router.temperature = router_settings.temperature;
                return self.load(&router).await;
            }
            lattice_core::application::contracts::settings::LLMProvider::Local => {
                return Err(AppError::AiModelsNotInstalled(format!(
                    "Router model '{}' is not downloaded. Download it in Settings → Model Catalog.",
                    model_name
                )));
            }
            lattice_core::application::contracts::settings::LLMProvider::Auto => {
                let mut router = settings.clone();
                router.llama_cpp.model = model_name.to_owned();
                router.max_tokens = router_settings.max_tokens;
                router.temperature = router_settings.temperature;
                lattice_core::application::services::model_selection::select_model(
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
            lattice_core::application::contracts::settings::LLMProvider::Ollama => {
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
        server: LocalServerOptions,
    ) -> Result<Option<Arc<dyn LLMPort>>> {
        use crate::features::llm::engine::factory::LLMConfig;

        let maybe_model = self.downloaded_models.find_by_model_id(model_id).await?;

        let model = match maybe_model {
            Some(model) => model,
            None => return Ok(None),
        };

        model.validate_for_operation(true)?;

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
            server,
        };

        match self.create_llm(llm_config).await {
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
        active: &lattice_core::domain::DownloadedModel,
        generation_config: crate::features::llm::engine::GenerationConfig,
        server: LocalServerOptions,
    ) -> Result<Option<Arc<dyn LLMPort>>> {
        let model_id = active.model_id();
        let model_path = match active.loadable_path() {
            Some(p) => p.to_path_buf(),
            None => {
                tracing::warn!(
                    model_id = %model_id,
                    "Selected utility model has no loadable path"
                );
                return Err(AppError::ModelLoadFailed(format!(
                    "The selected utility model '{}' has no loadable file. Select another model in Settings.",
                    active.model_name()
                )));
            }
        };
        if !model_path.exists() {
            tracing::warn!(
                model_id = %model_id,
                path = %model_path.display(),
                "Active utility model artifact missing on disk — falling back to chat LLM"
            );
            return Err(AppError::ModelLoadFailed(format!(
                "The selected utility model '{}' is missing from disk. Download it again or select another model in Settings.",
                active.model_name()
            )));
        }

        if let Some(error) = self.runtime.unloadable.known_failure(&model_path) {
            tracing::debug!(
                model_id = %model_id,
                path = %model_path.display(),
                "Reporting the selected utility model's cached load failure without retrying the load"
            );
            return Err(AppError::ModelLoadFailed(error));
        }

        let llm_config = crate::features::llm::engine::factory::LLMConfig::Local {
            model_path: model_path.clone(),
            n_gpu_layers: -1,
            generation_config,
            app_handle: self.app_handle.clone(),
            server,
        };

        tracing::info!(
            model_id = %model_id,
            path = %model_path.display(),
            "Loading utility LLM"
        );

        match self.create_llm(llm_config).await {
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
                    let error = format!(
                        "The selected utility model '{}' could not be loaded: {e}. \
                         Utility callers may use the chat model instead. Select a compatible \
                         utility model or update Lattice's local model engine.",
                        active.model_name()
                    );
                    self.runtime.unloadable.remember(&model_path, error.clone());
                    tracing::warn!(
                        model_id = %model_id,
                        path = %model_path.display(),
                        error = %e,
                        "Failed to load utility LLM — falling back to chat LLM and not \
                         retrying this file until it changes"
                    );
                    Err(AppError::ModelLoadFailed(error))
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
        use lattice_core::application::contracts::settings::LLMProvider;

        let mut utility = settings.clone();
        utility.model = model.to_owned();
        utility.llama_cpp.model = model.to_owned();
        utility.max_tokens = u32::try_from(generation_config.max_tokens).unwrap_or(u32::MAX);

        match settings.provider {
            LLMProvider::Local => Ok(None),
            LLMProvider::Llamacpp
            | LLMProvider::Openai
            | LLMProvider::Anthropic
            | LLMProvider::Google => self.load(&utility).await.map(Some),
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
        crate::features::llm::engine::GenerationConfig::from_settings(settings)
    }

    async fn try_load_active_model(
        &self,
        generation_config: crate::features::llm::engine::GenerationConfig,
        settings: &LLMSettingsDto,
    ) -> Result<Option<Arc<dyn LLMPort>>> {
        use crate::features::llm::engine::factory::LLMConfig;

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

        let server = LocalServerOptions::from_settings(settings)
            .with_model_config(settings.local_model_configs.get(active_model.model_id()));

        // Attempt to load the GGUF model
        let llm_config = LLMConfig::Local {
            model_path: model_path.clone(),
            n_gpu_layers: -1, // Use Metal GPU on M-series Macs (all layers on GPU)
            generation_config,
            // Populated when the app boots via Container::with_app_handle.
            // None in tests / sidecar-feature-off builds (where it's unused).
            app_handle: self.app_handle.clone(),
            server,
        };

        match self.create_llm(llm_config).await {
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
        match create_ollama_llm(
            &endpoint,
            &model,
            auth_header,
            generation_config,
            &self.runtime.schedulers,
        )
        .await
        {
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

        match create_ollama_llm(
            &endpoint,
            model,
            auth_header,
            generation_config,
            &self.runtime.schedulers,
        )
        .await
        {
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
mod tests;
