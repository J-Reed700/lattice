//! Which model each role runs on, loaded through the app's instances.
//!
//! A chat runs on its request override, else its own choice, else the
//! Settings default; the Learning and Verifier roles run on their own models
//! (the Verifier's may be a cloud model, checked for a saved key); the router and
//! utility roles on theirs. Only the Settings default keeps Auto's load-time
//! fallback — the active local model, then a llama.cpp server that is up, then
//! Ollama. A model named outright is used as named or refused, never swapped
//! for another.

use std::sync::Arc;
use std::time::Instant;

use lattice_core::application::contracts::settings::{LLMProvider, LLMSettingsDto};
use lattice_core::application::ports::llm_port::{
    ModelOrigin, ModelRequirements, ModelRole, PinnedModel, ResolvedModel, ToolSupport,
};
use lattice_core::application::ports::{LLMPort, SettingsRepositoryPort};
use lattice_core::application::services::chat_model_resolution::{
    default_chat_model, resolve, resolve_learning, resolve_verifier,
};
use lattice_core::application::services::model_selection::select_model;
use lattice_core::domain::conversation::ChatModelChoice;
use lattice_core::domain::models::model_ref::{CloudProvider, ModelRef};
use lattice_core::shared::error::{AppError, Result};

use crate::features::llm::engine::factory::LocalServerOptions;
use crate::features::llm::engine::vision::local_vision;
use crate::features::llm::instances::refusal::{self, Window};
use crate::features::llm::instances::{
    Acquire, Acquired, GpuBudget, Holder, LaunchProfile, LearningShortfall, LlmInstances, ModelKey,
    Tuning,
};
use crate::features::llm::loading::{cloud_key_saved, launch_profile_for, LocalModel, ModelLoader};

/// What a turn or job asks a model for.
pub(crate) struct ModelRequest {
    pub(crate) role: ModelRole,
    pub(crate) selection: ModelSelection,
    pub(crate) requirements: ModelRequirements,
    /// The work it is pinned for, as diagnostics and a refusal for memory
    /// name it: a chat's title; empty for unnamed work.
    pub(crate) work: String,
    pub(crate) conversation_id: Option<String>,
}

impl ModelRequest {
    /// The Settings default chat model, for maintenance work that belongs to
    /// no chat.
    pub(crate) fn default_chat() -> Self {
        Self {
            role: ModelRole::Maintenance,
            selection: ModelSelection::Default,
            requirements: ModelRequirements::default(),
            work: String::new(),
            conversation_id: None,
        }
    }
}

/// Which model a request wants.
pub(crate) enum ModelSelection {
    /// The Settings default chat model.
    Default,
    /// A chat's model: `request_override`, else `choice`, else the default.
    Chat {
        choice: ChatModelChoice,
        request_override: Option<ModelRef>,
    },
    /// The Learning model set in Settings; refused when none is.
    Learning,
    /// The Verifier model set in Settings; refused when none is, or when it
    /// names a cloud model with no API key saved.
    Verifier,
}

/// One model wanted for one piece of work.
struct Want<'a> {
    role: ModelRole,
    origin: ModelOrigin,
    holder: Option<Holder>,
    requirements: &'a ModelRequirements,
    tuning: Tuning,
    /// Refuse a llama.cpp server that does not list the model, so Auto moves
    /// on to the next candidate.
    must_list_model: bool,
}

/// Everything resolving and loading a role's model needs, without the
/// container, so work that loads later can carry it.
#[derive(Clone)]
pub(crate) struct RoleModels {
    pub(crate) instances: Arc<LlmInstances>,
    pub(crate) loader: Arc<ModelLoader>,
    pub(crate) budget: Arc<dyn GpuBudget>,
    pub(crate) settings: Arc<dyn SettingsRepositoryPort>,
}

impl RoleModels {
    async fn llm_settings(&self) -> Result<LLMSettingsDto> {
        self.settings
            .get_all()
            .await
            .map(|settings| settings.llm)
            .map_err(|e| AppError::InvalidConfig(format!("Failed to load settings: {e}")))
    }

    /// The model `request` asks for, pinned for its work.
    pub(crate) async fn llm_for(&self, request: &ModelRequest) -> Result<PinnedModel> {
        // Read before Settings, so a change that lands meanwhile keeps a load
        // built from the old Settings from being handed to anyone else.
        let epoch = self.instances.epoch();
        let settings = self.llm_settings().await?;
        let holder = Holder {
            role: request.role,
            work: request.work.clone(),
        };
        // The active local row, once read; `None` until something needs it.
        let mut active_local = None;
        let resolved = match &request.selection {
            ModelSelection::Default => None,
            ModelSelection::Learning => {
                let resolved = resolve_learning(&settings).map_err(|error| {
                    refusal::refused(request.role, None, "learning_unset", error)
                })?;
                Some(resolved)
            }
            ModelSelection::Verifier => {
                let resolved = resolve_verifier(&settings, cloud_key_saved).map_err(|error| {
                    refusal::refused(request.role, None, "verifier_unavailable", error)
                })?;
                Some(resolved)
            }
            ModelSelection::Chat {
                choice,
                request_override,
            } => {
                let local = self.loader.active_local_chat_model().await?;
                let default = default_chat_model(&settings, local.as_deref());
                let resolved = resolve(request_override.as_ref(), choice, &default)?;
                active_local = Some(local);
                (resolved.origin != ModelOrigin::Default).then_some(resolved)
            }
        };
        let started = Instant::now();
        let (key, acquired) = match &resolved {
            Some(resolved) => {
                let want = Want {
                    role: request.role,
                    origin: resolved.origin,
                    holder: Some(holder),
                    requirements: &request.requirements,
                    tuning: Tuning::Standard,
                    must_list_model: false,
                };
                self.pin(&resolved.model, &settings, epoch, &want).await?
            }
            None => {
                let active_local = match active_local {
                    Some(local) => local,
                    None => self.loader.active_local_chat_model().await?,
                };
                let pinned = self
                    .default_model(
                        &settings,
                        active_local,
                        epoch,
                        request.role,
                        holder,
                        &request.requirements,
                    )
                    .await?;
                self.instances.remember_default(&pinned.0);
                pinned
            }
        };
        let resolved = ResolvedModel {
            model: key.model.clone(),
            origin: resolved.map_or(ModelOrigin::Default, |resolved| resolved.origin),
        };
        tracing::info!(
            role = ?request.role,
            conversation_id = request.conversation_id.as_deref(),
            model = %resolved.model,
            origin = ?resolved.origin,
            provider = acquired.loaded.llm.provider_name(),
            context_tokens = acquired.loaded.llm.max_context_tokens(),
            instance_id = acquired.instance_id,
            reused = acquired.reused,
            load_ms = started.elapsed().as_millis() as u64,
            "model.resolved"
        );
        let instance_id = acquired.instance_id;
        let degraded = acquired.loaded.degraded().map(str::to_owned);
        Ok(PinnedModel {
            llm: acquired.into_port(),
            resolved,
            instance_id,
            degraded,
        })
    }

    /// The Settings default chat model, pinned for `holder`; `active_local`
    /// is the active row's `model_id`. Under Auto the first of the active
    /// local model, a llama.cpp server that lists its model, and Ollama that
    /// loads.
    async fn default_model(
        &self,
        settings: &LLMSettingsDto,
        active_local: Option<String>,
        epoch: u64,
        role: ModelRole,
        holder: Holder,
        requirements: &ModelRequirements,
    ) -> Result<(ModelKey, Acquired)> {
        let want = |must_list_model| Want {
            role,
            origin: ModelOrigin::Default,
            holder: Some(holder.clone()),
            requirements,
            tuning: Tuning::Standard,
            must_list_model,
        };
        if settings.provider != LLMProvider::Auto {
            let model = default_chat_model(settings, active_local.as_deref()).0?;
            return self.pin(&model, settings, epoch, &want(false)).await;
        }
        let llama_cpp = settings.llama_cpp.model.trim();
        let listed = want(true);
        select_model(
            LLMProvider::Auto,
            || async {
                match &active_local {
                    Some(model_id) => self
                        .pin(
                            &ModelRef::Local {
                                model_id: model_id.clone(),
                            },
                            settings,
                            epoch,
                            &want(false),
                        )
                        .await
                        .map(Some),
                    None => Ok(None),
                }
            },
            || self.configured_llama_cpp(llama_cpp, settings, epoch, &listed),
            || async {
                let ollama = ModelRef::Ollama {
                    model: settings.model.clone(),
                };
                self.pin(&ollama, settings, epoch, &want(false)).await
            },
        )
        .await
    }

    /// The router model. A downloaded model of that name runs locally;
    /// otherwise the configured provider serves the name, with Auto trying a
    /// llama.cpp server that lists it before Ollama.
    pub(crate) async fn router_llm(&self) -> Result<Arc<dyn LLMPort>> {
        let epoch = self.instances.epoch();
        let settings = self.llm_settings().await?;
        if !settings.router.enabled {
            return Err(AppError::InvalidConfig(
                "Router is required for follow-up handling and cannot be disabled.".to_string(),
            ));
        }
        let name = settings.router.model.trim();
        if name.is_empty() {
            return Err(AppError::InvalidConfig(
                "Router model is required. Configure llm.router.model in Settings → Model Catalog."
                    .to_string(),
            ));
        }
        let requirements = ModelRequirements::default();
        let want = |must_list_model| Want {
            role: ModelRole::Router,
            origin: ModelOrigin::Default,
            holder: None,
            requirements: &requirements,
            tuning: Tuning::Router,
            must_list_model,
        };
        let local = ModelRef::Local {
            model_id: name.to_owned(),
        };
        match self.loader.local_model(name).await {
            Ok(Some(_)) => {
                return Ok(self
                    .pin(&local, &settings, epoch, &want(false))
                    .await?
                    .1
                    .into_port())
            }
            Ok(None) => {}
            // A row whose file is gone, or one Ollama serves, falls through to
            // the configured provider, which serves models by name.
            Err(AppError::ModelLoadFailed(reason)) => {
                tracing::debug!(router_model = name, %reason, "Router model is not a usable local file");
            }
            Err(error) => return Err(error),
        }
        let acquired = match settings.provider {
            LLMProvider::Local => {
                return Err(AppError::AiModelsNotInstalled(format!(
                "Router model '{name}' is not downloaded. Download it in Settings → Model Catalog."
            )))
            }
            LLMProvider::Ollama => self
                .pin(&ollama(name), &settings, epoch, &want(false))
                .await
                .map_err(|e| {
                    AppError::AiModelsNotInstalled(format!(
                        "Router model '{name}' is not available locally and Ollama failed: {e}. \
                         Download the router model in Settings → Model Catalog or configure Ollama."
                    ))
                })?,
            LLMProvider::Auto => {
                let (listed, unlisted, ollama) = (want(true), want(false), ollama(name));
                select_model(
                    LLMProvider::Auto,
                    // A downloaded router model was looked for above.
                    || async { Ok(None) },
                    || self.configured_llama_cpp(name, &settings, epoch, &listed),
                    || self.pin(&ollama, &settings, epoch, &unlisted),
                )
                .await?
            }
            provider => {
                let model = remote_model(provider, name)?;
                self.pin(&model, &settings, epoch, &want(false)).await?
            }
        };
        Ok(acquired.1.into_port())
    }

    /// The utility model, or `None` when none is set, or when a remote one
    /// cannot be reached — its callers then use the default chat model.
    pub(crate) async fn utility_llm(&self) -> Result<Option<Arc<dyn LLMPort>>> {
        let epoch = self.instances.epoch();
        let settings = self.llm_settings().await?;
        let requirements = ModelRequirements::default();
        let want = |must_list_model| Want {
            role: ModelRole::Utility,
            origin: ModelOrigin::Default,
            holder: None,
            requirements: &requirements,
            tuning: Tuning::Utility,
            must_list_model,
        };
        if let Some(model) = &settings.utility_model {
            return match self.pin(model, &settings, epoch, &want(false)).await {
                Ok((_, acquired)) => Ok(Some(acquired.into_port())),
                Err(error) if matches!(model, ModelRef::Local { .. }) => Err(error),
                Err(e) => {
                    tracing::warn!(
                        utility_model = %model,
                        error = %e,
                        "Failed to reach the remote utility model — falling back to the default chat model"
                    );
                    Ok(None)
                }
            };
        }
        let active = match self.loader.active_utility_model().await {
            Ok(Some(active)) => active,
            Ok(None) => return Ok(None),
            Err(e) => {
                tracing::warn!(
                    error = %e,
                    "Failed to query active utility model — falling back to the default chat model"
                );
                return Ok(None);
            }
        };
        if active.location().is_local() {
            let model = ModelRef::Local {
                model_id: active.model_id().to_owned(),
            };
            let (_, acquired) = self.pin(&model, &settings, epoch, &want(false)).await?;
            return Ok(Some(acquired.into_port()));
        }
        let tag = match settings.ollama_utility_model.trim() {
            "" => settings.model.clone(),
            tag => tag.to_owned(),
        };
        let remote = match settings.provider {
            LLMProvider::Local => return Ok(None),
            LLMProvider::Ollama => {
                self.pin(&ollama(&tag), &settings, epoch, &want(false))
                    .await
            }
            // The configured provider decides the wire protocol, not the label
            // on the row: under Auto a llama.cpp server that lists the model is
            // tried before Ollama, as the chat model is, so one host is never
            // spoken to in two protocols.
            LLMProvider::Auto => {
                match self
                    .configured_llama_cpp(&tag, &settings, epoch, &want(true))
                    .await
                {
                    Ok(Some(acquired)) => Ok(acquired),
                    Ok(None) => {
                        self.pin(&ollama(&tag), &settings, epoch, &want(false))
                            .await
                    }
                    Err(error) => {
                        tracing::warn!(
                            utility_model = %tag,
                            %error,
                            "llama.cpp not available for the utility role, trying Ollama"
                        );
                        self.pin(&ollama(&tag), &settings, epoch, &want(false))
                            .await
                    }
                }
            }
            provider => match remote_model(provider, &tag) {
                Ok(model) => self.pin(&model, &settings, epoch, &want(false)).await,
                Err(error) => Err(error),
            },
        };
        match remote {
            Ok((_, acquired)) => Ok(Some(acquired.into_port())),
            Err(e) => {
                tracing::warn!(
                    model_id = %active.model_id(),
                    utility_model = %tag,
                    error = %e,
                    "Failed to reach the remote utility model — falling back to the default chat model"
                );
                Ok(None)
            }
        }
    }

    /// `model` on the llama.cpp server, when a llama.cpp connection is
    /// configured — Settings names a llama.cpp model — and the server lists
    /// it.
    async fn configured_llama_cpp(
        &self,
        model: &str,
        settings: &LLMSettingsDto,
        epoch: u64,
        want: &Want<'_>,
    ) -> Result<Option<(ModelKey, Acquired)>> {
        if settings.llama_cpp.model.trim().is_empty() {
            return Ok(None);
        }
        let llama_cpp = ModelRef::LlamaCpp {
            model: model.to_owned(),
        };
        self.pin(&llama_cpp, settings, epoch, want).await.map(Some)
    }

    /// `model` loaded through the instances and pinned for `want`, refused
    /// when it cannot serve the work: not downloaded, too small a window,
    /// no tools when they are needed, or no room for it.
    async fn pin(
        &self,
        model: &ModelRef,
        settings: &LLMSettingsDto,
        epoch: u64,
        want: &Want<'_>,
    ) -> Result<(ModelKey, Acquired)> {
        let role = want.role;
        let local = match model {
            ModelRef::Local { model_id } => Some(
                self.loader
                    .local_model(model_id)
                    .await
                    .map_err(|e| refusal::refused(role, Some(model), "file_missing", e))?
                    .ok_or_else(|| {
                        refusal::refused(
                            role,
                            Some(model),
                            "not_downloaded",
                            refusal::not_downloaded(role, want.origin, model_id),
                        )
                    })?,
            ),
            _ => None,
        };
        // A projector that finished downloading since this model's server
        // started makes a different server: the next request starts one that
        // sees, and the text-only one is evicted when its memory is wanted.
        let launch = launch_profile_for(model, settings).map(|launch| {
            let projector_ready = local
                .as_ref()
                .is_some_and(|local| local_vision(&local.path, launch.vision).is_some());
            LocalServerOptions {
                projector_ready,
                ..launch
            }
        });
        let key = ModelKey {
            model: model.clone(),
            launch,
            tuning: want.tuning,
        };
        let floor = want
            .requirements
            .min_context_tokens
            .map(|tokens| u32::try_from(tokens).unwrap_or(u32::MAX));
        if let (Some(floor), Some(local)) = (floor, &local) {
            if let Some(window) = self.window_short_of(floor, local, key.launch).await {
                let auto = self
                    .budget
                    .auto_window(&local.path, key.launch.unwrap_or_default())
                    .await;
                return Err(self.short_window(role, model, &local.name, floor, window, auto));
            }
        }
        let generation = want.tuning.generation(settings);
        let must_list_model = want.must_list_model;
        let acquired = self
            .instances
            .acquire(
                Acquire {
                    key: key.clone(),
                    epoch,
                    role,
                    holder: want.holder.clone(),
                    local: local.as_ref(),
                    floor,
                },
                self.budget.as_ref(),
                || async {
                    let created = self
                        .loader
                        .load_ref(model, settings, generation)
                        .await
                        .map_err(|e| refusal::refused(role, Some(model), load_failure(&e), e))?;
                    if must_list_model && !created.llm.is_ready().await? {
                        return Err(AppError::ServiceNotAvailable(format!(
                            "The llama.cpp server at {} does not list {}.",
                            settings.llama_cpp.url,
                            created.llm.model_name()
                        )));
                    }
                    Ok(created)
                },
            )
            .await?;

        let llm = &acquired.loaded.llm;
        let window = u32::try_from(llm.max_context_tokens()).unwrap_or(u32::MAX);
        // A window known too small was refused before loading; what is left
        // is a server Auto sized to fit beside the others, or one that came up
        // smaller than it was asked to, or a remote server's own window.
        if let Some(floor) = floor.filter(|floor| window < *floor) {
            let window = match local {
                Some(_) => Window::Running(window),
                None => Window::Reported(window),
            };
            return Err(self.short_window(role, model, &acquired.loaded.name, floor, window, None));
        }
        if want.requirements.tools == ToolSupport::Required && !llm.supports_tool_calling() {
            return Err(refusal::refused(
                role,
                Some(model),
                "no_tools",
                refusal::no_tools(role, &acquired.loaded.name),
            ));
        }
        if role == ModelRole::Learning {
            self.instances.learning_window_served(model, window);
        }
        Ok((key, acquired))
    }

    /// The refusal for `model`, whose `window` is below the work's `floor`.
    /// A Learning refusal is remembered, so Settings → Models can say so
    /// beside the model until it covers the window.
    /// The window `local` would have below `floor`, when that is known before
    /// loading it: one set in Settings → Models, or, under Auto, the length
    /// the model was trained for when Auto is below the floor too. Auto's
    /// own window is not enough on its own: evicting idle models to admit
    /// this one may give it more.
    async fn window_short_of(
        &self,
        floor: u32,
        local: &LocalModel,
        launch: LaunchProfile,
    ) -> Option<Window> {
        let launch = launch?;
        if let Some(set) = launch.context_window {
            return (set < floor).then_some(Window::Set(set));
        }
        let trained = self
            .budget
            .trained_window(&local.path)
            .filter(|trained| *trained < floor)?;
        let auto = self.budget.auto_window(&local.path, launch).await?;
        (auto < floor).then_some(Window::Trained(trained))
    }

    fn short_window(
        &self,
        role: ModelRole,
        model: &ModelRef,
        name: &str,
        floor: u32,
        window: Window,
        auto: Option<u32>,
    ) -> AppError {
        if role == ModelRole::Learning {
            let (tokens, window_is_saved) = match window {
                Window::Set(tokens) => (tokens, true),
                Window::Running(tokens) | Window::Reported(tokens) | Window::Trained(tokens) => {
                    (tokens, false)
                }
            };
            self.instances.record_learning_shortfall(LearningShortfall {
                model: model.clone(),
                needed: floor,
                window: tokens,
                window_is_saved,
            });
        }
        refusal::refused(
            role,
            Some(model),
            "window_too_small",
            refusal::window_too_small(role, name, floor, window, auto),
        )
    }
}

/// `name` on Ollama.
fn ollama(name: &str) -> ModelRef {
    ModelRef::Ollama {
        model: name.to_owned(),
    }
}

/// `name` on the remote server an explicit `provider` speaks to.
fn remote_model(provider: LLMProvider, name: &str) -> Result<ModelRef> {
    let cloud = |provider| ModelRef::Cloud {
        provider,
        model: name.to_owned(),
    };
    Ok(match provider {
        LLMProvider::Llamacpp => ModelRef::LlamaCpp {
            model: name.to_owned(),
        },
        LLMProvider::Ollama => ollama(name),
        LLMProvider::Openai => cloud(CloudProvider::Openai),
        LLMProvider::Anthropic => cloud(CloudProvider::Anthropic),
        LLMProvider::Google => cloud(CloudProvider::Google),
        LLMProvider::Local | LLMProvider::Auto => {
            return Err(AppError::InvalidConfig(
                "Local and Auto do not name a remote server".into(),
            ))
        }
    })
}

/// The `model.refused` reason a load failure is logged under.
fn load_failure(error: &AppError) -> &'static str {
    match error {
        AppError::AiModelsNotInstalled(_) => "not_downloaded",
        AppError::ModelLoadFailed(_) => "incompatible_gguf",
        AppError::InvalidConfig(message) if message.contains("API key") => "no_key",
        _ => "endpoint_unreachable",
    }
}

#[cfg(test)]
mod tests;
