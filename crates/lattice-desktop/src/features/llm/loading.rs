//! Opens a named model — a downloaded file, an Ollama tag, a llama.cpp
//! model or a cloud model — with the connection Settings gives its server.
//! Which model a role uses, and any fallback between them, belongs to the
//! caller.
use std::path::PathBuf;
use std::sync::Arc;

use crate::features::llm::engine::factory::{CreatedLlm, LLMConfig, LocalServerOptions};
use crate::features::llm::engine::system::BackendDevices;
use crate::features::llm::engine::types::LLMError;
use crate::features::llm::engine::vision::declared_vision;
use crate::features::llm::engine::GenerationConfig;
use crate::features::llm::instances::LaunchProfile;
use crate::features::llm::scheduler::SchedulerRegistry;
use lattice_core::application::contracts::settings::{LLMProvider, LLMSettingsDto};
use lattice_core::application::ports::active_models::ActiveModels;
use lattice_core::application::ports::{LLMPort, LoadedEmbeddingModelPort};
use lattice_core::domain::models::model_ref::{CloudProvider, ModelRef};
use lattice_core::domain::repositories::DownloadedModelRepository;
use lattice_core::shared::error::{AppError, Result};

/// A bundled llama-server that cannot execute is an install problem, not a
/// model problem. It must not be reported as a corrupt model, remembered
/// against the file, nor quietly replaced by another model.
fn unusable_sidecar(error: &LLMError) -> Option<AppError> {
    match error {
        LLMError::SidecarBinaryUnusable(message) => {
            Some(AppError::ServiceNotAvailable(message.clone()))
        }
        _ => None,
    }
}

/// Model files the bundled engine cannot read, keyed by path and stamped with
/// the file's size and mtime. Only that verdict is kept: a start that timed
/// out or was killed for memory says nothing about the file.
///
/// Starting a local model means launching a sidecar and reading several
/// gigabytes of tensors, and a file llama-server cannot parse fails only after
/// paying all of that. The utility role asks on every single turn, so one
/// unsupported GGUF would tax every message with a doomed load. Remembering
/// the failure makes the second ask cheap.
///
/// The stamp is what allows recovery without a restart: a re-downloaded or
/// repaired file has a different size or mtime, so it is a different key and
/// gets a fresh attempt.
#[derive(Default)]
pub(crate) struct UnloadableArtifacts {
    known: std::sync::Mutex<std::collections::HashMap<std::path::PathBuf, (ArtifactStamp, String)>>,
}

/// What every model load in one app shares: the per-backend schedulers, the
/// artifacts known not to start, the probe of the devices a local server can
/// offload to, and the embedding model that shares one with it. The container
/// owns one and hands it to each loader, so two containers never share any
/// of it.
#[derive(Clone, Default)]
pub(crate) struct LlmRuntime {
    pub(crate) schedulers: Arc<SchedulerRegistry>,
    pub(crate) unloadable: Arc<UnloadableArtifacts>,
    pub(crate) devices: Arc<BackendDevices>,
    /// The loaded embedding model, which runs inside the app; `None` where
    /// nothing loads one.
    pub(crate) embedding: Option<Arc<dyn LoadedEmbeddingModelPort>>,
}

impl LlmRuntime {
    /// The runtime for an app whose device probe is `devices` and whose
    /// embedding model is loaded through `embedding`.
    pub(crate) fn new(
        devices: Arc<BackendDevices>,
        embedding: Arc<dyn LoadedEmbeddingModelPort>,
    ) -> Self {
        Self {
            devices,
            embedding: Some(embedding),
            ..Self::default()
        }
    }

    /// Memory models running inside the app hold on the accelerator a local
    /// server offloads to: the embedding model's weights while it is loaded
    /// on one.
    pub(crate) fn in_app_model_bytes(&self) -> u64 {
        self.embedding
            .as_ref()
            .and_then(|embedding| embedding.current_model())
            .map_or(0, |model| model.accelerator_bytes())
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
    downloaded_models: Arc<dyn DownloadedModelRepository>,
    active_models: Arc<dyn ActiveModels>,
    app_handle: Option<tauri::AppHandle>,
    runtime: LlmRuntime,
}

/// A downloaded model ready to start: a chat model whose file is on disk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LocalModel {
    /// The name the user knows it by.
    pub(crate) name: String,
    pub(crate) path: PathBuf,
}

/// How the bundled server for `model` is launched: Settings' server options
/// with the model's own saved configuration over them. A model's
/// configuration is its own wherever it is used, so every role on one model
/// asks for the same server. `None` for a model on a server Lattice does not
/// run.
pub(crate) fn launch_profile_for(model: &ModelRef, settings: &LLMSettingsDto) -> LaunchProfile {
    match model {
        ModelRef::Local { model_id } => Some(
            LocalServerOptions::from_settings(settings)
                .with_model_config(settings.local_model_configs.get(model_id)),
        ),
        ModelRef::Ollama { .. } | ModelRef::LlamaCpp { .. } | ModelRef::Cloud { .. } => None,
    }
}

impl ModelLoader {
    pub(crate) fn new(
        downloaded_models: Arc<dyn DownloadedModelRepository>,
        active_models: Arc<dyn ActiveModels>,
        app_handle: Option<tauri::AppHandle>,
        runtime: LlmRuntime,
    ) -> Self {
        Self {
            downloaded_models,
            active_models,
            app_handle,
            runtime,
        }
    }

    /// The `model_id` of the row active for chat, when it is a downloaded
    /// file.
    pub(crate) async fn active_local_chat_model(&self) -> Result<Option<String>> {
        Ok(self
            .active_models
            .get_active_chat_model()
            .await?
            .filter(|model| model.location().is_local())
            .map(|model| model.model_id().to_owned()))
    }

    /// The row active for the utility role.
    pub(crate) async fn active_utility_model(
        &self,
    ) -> Result<Option<lattice_core::domain::DownloadedModel>> {
        self.active_models.get_active_utility_model().await
    }

    /// The downloaded row `model_id` names, ready to start, or `None` when no
    /// row names it.
    pub(crate) async fn local_model(&self, model_id: &str) -> Result<Option<LocalModel>> {
        let Some(row) = self.downloaded_models.find_by_model_id(model_id).await? else {
            return Ok(None);
        };
        row.validate_for_operation(true)?;
        let name = row.model_name().to_owned();
        let path = row.loadable_path().map(PathBuf::from).ok_or_else(|| {
            AppError::ModelLoadFailed(format!(
                "{name} is served by Ollama, not from a downloaded file. Choose it as an Ollama model."
            ))
        })?;
        if !path.exists() {
            return Err(AppError::ModelLoadFailed(format!(
                "{name}'s file is missing from {}.",
                path.display()
            )));
        }
        Ok(Some(LocalModel { name, path }))
    }

    /// Open `model` with `generation`, reaching its server through Settings.
    /// A downloaded model starts a bundled server launched with
    /// [`launch_profile_for`], or shares one already running.
    pub(crate) async fn load_ref(
        &self,
        model: &ModelRef,
        settings: &LLMSettingsDto,
        generation: GenerationConfig,
    ) -> Result<CreatedLlm> {
        let remote = |llm| CreatedLlm { llm, server: None };
        match model {
            ModelRef::Local { model_id } => {
                let local = self.local_model(model_id).await?.ok_or_else(|| {
                    AppError::AiModelsNotInstalled(format!(
                        "{model_id} is not downloaded any more. Download it in Settings → Models \
                         or pick another model."
                    ))
                })?;
                let server = launch_profile_for(model, settings).unwrap_or_default();
                self.load_local(&local, generation, server).await
            }
            ModelRef::Ollama { model } => self
                .load_ollama(settings, model, generation)
                .await
                .map(remote),
            ModelRef::LlamaCpp { model: name } => {
                let mut connection = with_generation(settings, &generation);
                connection.llama_cpp.model = name.clone();
                Ok(remote(
                    crate::features::llm::llama_cpp::LlamaCppLlm::new(&connection)?
                        .with_vision(declared_vision(settings, model))
                        .schedule(&self.runtime.schedulers)
                        .await,
                ))
            }
            ModelRef::Cloud {
                provider,
                model: name,
            } => {
                let key = cloud_api_key(*provider)?;
                let mut connection = with_generation(settings, &generation);
                connection.provider = llm_provider(*provider);
                connection.model = name.clone();
                Ok(remote(
                    crate::features::llm::cloud::CloudLlm::new(&connection, key)?
                        .with_vision(declared_vision(settings, model))
                        .schedule(&self.runtime.schedulers),
                ))
            }
        }
    }

    /// Start `local` on a bundled server launched with `server`, or share the
    /// one already running for it. A file the engine cannot read is reported
    /// again without another attempt until the file changes.
    pub(crate) async fn load_local(
        &self,
        local: &LocalModel,
        generation_config: GenerationConfig,
        server: LocalServerOptions,
    ) -> Result<CreatedLlm> {
        if let Some(error) = self.runtime.unloadable.known_failure(&local.path) {
            tracing::debug!(
                model = %local.name,
                path = %local.path.display(),
                "Reporting the model's remembered load failure without loading it again"
            );
            return Err(AppError::ModelLoadFailed(error));
        }
        tracing::info!(
            model = %local.name,
            path = %local.path.display(),
            "Loading local model"
        );
        let config = LLMConfig::Local {
            model_path: local.path.clone(),
            n_gpu_layers: -1,
            generation_config,
            // Populated when the app boots via Container::with_app_handle.
            // None in tests / sidecar-feature-off builds (where it's unused).
            app_handle: self.app_handle.clone(),
            server,
            in_app_model_bytes: self.runtime.in_app_model_bytes(),
        };
        crate::features::llm::engine::factory::create_llm(
            config,
            &self.runtime.schedulers,
            &self.runtime.devices,
        )
        .await
        .map_err(|error| self.load_failure(local, error))
    }

    /// The error for a local start that failed. Only a verdict on the file
    /// itself is remembered: a readiness timeout, a launch killed for memory
    /// or a GPU that failed to come up says nothing about the bytes, and the
    /// next request deserves a fresh attempt.
    fn load_failure(&self, local: &LocalModel, error: LLMError) -> AppError {
        if let Some(unusable) = unusable_sidecar(&error) {
            tracing::error!(
                model = %local.name,
                error = %error,
                "Local model cannot start: the bundled llama-server is unusable"
            );
            return unusable;
        }
        if let LLMError::ModelUnsupported(reason) = &error {
            // Another attempt reads the same bytes and fails the same way,
            // after paying for a launch and a load.
            let message = format!("{} can't be loaded: {reason}", local.name);
            self.runtime
                .unloadable
                .remember(&local.path, message.clone());
            tracing::warn!(
                model = %local.name,
                path = %local.path.display(),
                error = %error,
                "Local model is unsupported; not trying this file again until it changes"
            );
            return AppError::ModelLoadFailed(message);
        }
        tracing::warn!(
            model = %local.name,
            path = %local.path.display(),
            error = %error,
            "Local model failed to start; the next request tries again"
        );
        AppError::ModelLoadFailed(format!("{} did not start: {error}", local.name))
    }

    /// Connect to `model` on the Ollama server Settings names.
    pub(crate) async fn load_ollama(
        &self,
        settings: &LLMSettingsDto,
        model: &str,
        generation_config: GenerationConfig,
    ) -> Result<Arc<dyn LLMPort>> {
        use crate::features::llm::engine::factory::create_ollama_llm;

        let endpoint = settings.ollama_url.trim();
        if endpoint.is_empty() {
            return Err(AppError::InvalidConfig(
                "Ollama is not configured. Add its connection in Settings → Chat.".into(),
            ));
        }
        if model.trim().is_empty() {
            return Err(AppError::InvalidConfig(
                "The Ollama model is not configured. Choose one in Settings → Chat.".into(),
            ));
        }
        let auth_header_name = settings.ollama_auth_header_name.trim();
        let auth_header_value = settings.ollama_auth_header_value.trim();
        let auth_header = (!auth_header_name.is_empty() && !auth_header_value.is_empty())
            .then(|| (auth_header_name.to_string(), auth_header_value.to_string()));

        tracing::info!(endpoint, model, "Connecting to Ollama");
        let vision = declared_vision(
            settings,
            &ModelRef::Ollama {
                model: model.to_owned(),
            },
        );
        create_ollama_llm(
            endpoint,
            model,
            auth_header,
            generation_config,
            vision,
            &self.runtime.schedulers,
        )
        .await
        .map_err(|e| {
            tracing::warn!(endpoint, model, error = %e, "Failed to connect to Ollama");
            let message = format!(
                "Can't reach Ollama at {endpoint} for {model}: {e}. Make sure Ollama is running \
                 (ollama serve) and the model is pulled (ollama pull {model})."
            );
            // A refused or unreachable server is an outage: work waiting on
            // it is deferred, not ended as a repeated failure.
            match e {
                LLMError::Network(_) => AppError::Network(message),
                LLMError::InvalidConfig(_) => AppError::InvalidConfig(message),
                _ => AppError::ServiceNotAvailable(message),
            }
        })
    }
}

/// `settings` with the sampling temperature and output ceiling a client is
/// built with replaced by `generation`'s, for the clients that read them from
/// Settings rather than from a [`GenerationConfig`].
fn with_generation(settings: &LLMSettingsDto, generation: &GenerationConfig) -> LLMSettingsDto {
    LLMSettingsDto {
        temperature: generation.temperature,
        max_tokens: u32::try_from(generation.max_tokens).unwrap_or(u32::MAX),
        ..settings.clone()
    }
}

fn llm_provider(provider: CloudProvider) -> LLMProvider {
    match provider {
        CloudProvider::Openai => LLMProvider::Openai,
        CloudProvider::Anthropic => LLMProvider::Anthropic,
        CloudProvider::Google => LLMProvider::Google,
    }
}

/// Whether `provider` has an API key, looked up as [`cloud_api_key`] reads
/// it. The key itself is not returned.
pub(crate) fn cloud_key_saved(provider: CloudProvider) -> Result<bool> {
    match cloud_api_key(provider) {
        Ok(_) => Ok(true),
        Err(AppError::InvalidConfig(_)) => Ok(false),
        Err(error) => Err(error),
    }
}

/// The API key for `provider`, read as everywhere else: its environment
/// variable, else the keyring.
fn cloud_api_key(provider: CloudProvider) -> Result<String> {
    let (name, label) = match provider {
        CloudProvider::Openai => ("openai", "OpenAI"),
        CloudProvider::Anthropic => ("anthropic", "Anthropic"),
        CloudProvider::Google => ("google", "Google"),
    };
    crate::infrastructure::security::keyring_storage::SecureStorage::new()
        .cloud_api_key(name)?
        .ok_or_else(|| {
            AppError::InvalidConfig(format!(
                "No {label} API key is saved. Add it in Settings → Chat."
            ))
        })
}

#[cfg(test)]
mod tests;
