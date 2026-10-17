//! Every LLM loaded right now, for every role, keyed by what it is.
//!
//! One pool replaces the single-slot chat, router and utility caches: a chat
//! on one model, a chat on another, the utility model and the Learning model
//! can all be loaded at once, and picking a model for one chat unloads
//! nothing another is using. A key names a model, how its server is launched
//! and how its client generates; it never names a role, so two roles that
//! resolve to the same key share one client and one server.
//!
//! Before a new bundled server starts, admission checks that it fits in the
//! GPU memory the running servers leave, evicting idle models to make room
//! and refusing — with the sizes and who holds the memory — when that is not
//! enough. Nothing in use is ever unloaded.

use std::sync::{Arc, Mutex};

use lattice_core::application::contracts::settings::LLMSettingsDto;
use lattice_core::application::ports::llm_port::{InvalidationCause, ModelRole};
use lattice_core::application::ports::{LLMPort, LoadedChatModelPort};
use lattice_core::domain::models::model_ref::ModelRef;
use lattice_core::shared::error::{AppError, Result};

use crate::features::llm::engine::factory::{CreatedLlm, LocalServerOptions, ServerFacts};
use crate::features::llm::engine::sidecar_manager::{resident_bytes_of, Admission};
use crate::features::llm::engine::GenerationConfig;
use crate::features::llm::loading::LocalModel;
use crate::infrastructure::ml::instance_pool::{Alive, InstancePool, Lease, Loaded};

mod admission;
mod invalidation;
mod pinned;
pub(crate) mod refusal;

pub(crate) use admission::{GpuBudget, SidecarBudget};
pub(crate) use pinned::PinnedLlm;

/// How the bundled server for a model is launched: the server options its
/// sidecar's identity is made of. `None` for a model on a server Lattice
/// does not run.
pub(crate) type LaunchProfile = Option<LocalServerOptions>;

/// Output ceiling for the shared utility model.
///
/// Sized to the largest cap its callers send (conversation memory extraction
/// asks for 8192), because a ceiling below a caller's own cap does not shorten
/// that caller's work — it truncates its reply.
const UTILITY_MAX_OUTPUT_TOKENS: usize = 8_192;

/// The generation settings a client is built with. Most work generates as
/// Settings says; the utility and router roles have their own temperature or
/// output ceiling, baked into the client, so theirs is a client of its own
/// over the same server.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Tuning {
    /// Settings' sampling and output ceiling: chat, learning and maintenance.
    Standard,
    /// Settings' sampling under the utility role's output ceiling.
    Utility,
    /// The router's own temperature and output ceiling.
    Router,
}

impl Tuning {
    pub(crate) fn generation(self, settings: &LLMSettingsDto) -> GenerationConfig {
        let standard = GenerationConfig::from_settings(settings);
        match self {
            Self::Standard => standard,
            Self::Utility => GenerationConfig {
                // A ceiling, not an allowance: each caller sends the cap its own
                // work needs and `max_output_tokens` only ever tightens this one.
                max_tokens: UTILITY_MAX_OUTPUT_TOKENS,
                ..standard
            },
            Self::Router => GenerationConfig {
                temperature: settings.router.temperature,
                max_tokens: settings.router.max_tokens as usize,
                ..standard
            },
        }
    }
}

/// What a loaded model is: the model, how its server is launched, and how its
/// client generates. Roles are not part of it.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct ModelKey {
    pub(crate) model: ModelRef,
    pub(crate) launch: LaunchProfile,
    pub(crate) tuning: Tuning,
}

/// A loaded model.
pub(crate) struct LoadedLlm {
    pub(crate) llm: Arc<dyn LLMPort>,
    /// The name the user knows it by.
    pub(crate) name: String,
    /// The bundled server it runs on, when Lattice runs one.
    pub(crate) server: Option<ServerFacts>,
    /// GPU memory its server holds; `None` for a server Lattice does not run.
    pub(crate) gpu_bytes: Option<u64>,
}

impl LoadedLlm {
    /// `created` for `model`. A local model is named and priced by `local`.
    pub(crate) fn new(created: CreatedLlm, model: &ModelRef, local: Option<&LocalModel>) -> Self {
        let gpu_bytes = match (&created.server, local) {
            (Some(server), Some(local)) if server.n_gpu_layers > 0 => {
                Some(resident_bytes_of(&local.path, server.context_size))
            }
            (Some(_), _) => Some(0),
            (None, _) => None,
        };
        let name = match (local, model) {
            (Some(local), _) => local.name.clone(),
            (None, ModelRef::Local { model_id }) => model_id.clone(),
            (None, ModelRef::Ollama { model } | ModelRef::LlamaCpp { model }) => model.clone(),
            (None, ModelRef::Cloud { model, .. }) => model.clone(),
        };
        Self {
            llm: created.llm,
            name,
            server: created.server,
            gpu_bytes,
        }
    }

    /// Why it runs degraded — on the CPU, or with a smaller window than asked
    /// for — when it does.
    pub(crate) fn degraded(&self) -> Option<&str> {
        self.server.as_ref()?.degraded.as_deref()
    }
}

impl Alive for LoadedLlm {
    fn is_alive(&self) -> bool {
        self.llm.is_alive()
    }
}

/// Who holds a loaded model, and for what.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Holder {
    pub(crate) role: ModelRole,
    /// The chat's title, for a chat; empty when the work has no name.
    pub(crate) work: String,
}

impl Holder {
    /// The holder as diagnostics list it: the role, then the work.
    pub(crate) fn label(&self) -> String {
        let role = match self.role {
            ModelRole::Chat => "chat",
            ModelRole::Utility => "utility",
            ModelRole::Router => "router",
            ModelRole::Learning => "learning",
            ModelRole::Maintenance => "maintenance",
        };
        if self.work.is_empty() {
            role.to_owned()
        } else {
            format!("{role}: {}", self.work)
        }
    }
}

/// A model loaded now, for diagnostics.
#[derive(Debug, Clone)]
pub(crate) struct LoadedInstance {
    pub(crate) model: ModelRef,
    pub(crate) provider: String,
    pub(crate) context_tokens: u32,
    pub(crate) in_flight: u32,
    pub(crate) pinned_by: Vec<String>,
    pub(crate) loaded_at: String,
    pub(crate) last_used_at: String,
    pub(crate) gpu_bytes: Option<u64>,
    pub(crate) degraded: Option<String>,
}

/// What [`LlmInstances::acquire`] loads, and for whom.
pub(crate) struct Acquire<'a> {
    pub(crate) key: ModelKey,
    /// Read before the settings the key and its loader were built from.
    pub(crate) epoch: u64,
    pub(crate) role: ModelRole,
    /// Who pins it for the work; `None` hands it out unpinned.
    pub(crate) holder: Option<Holder>,
    /// The downloaded model behind a local key, which admission prices.
    pub(crate) local: Option<&'a LocalModel>,
    /// The smallest window the work can run in, in tokens.
    pub(crate) floor: Option<u32>,
}

/// A model handed out by [`LlmInstances::acquire`].
pub(crate) struct Acquired {
    pub(crate) loaded: Arc<LoadedLlm>,
    pub(crate) instance_id: u64,
    pub(crate) reused: bool,
    lease: Option<Lease<Holder>>,
}

impl Acquired {
    /// The client to hand on: pinned while it is held, when it was acquired
    /// for a holder.
    pub(crate) fn into_port(self) -> Arc<dyn LLMPort> {
        match self.lease {
            Some(lease) => Arc::new(PinnedLlm::new(self.loaded, lease)),
            None => Arc::clone(&self.loaded.llm),
        }
    }
}

/// The app's loaded models.
pub(crate) struct LlmInstances {
    pool: InstancePool<ModelKey, LoadedLlm, Holder>,
    /// The Settings default chat model most recently handed out, so work that
    /// only reads "the loaded chat model" finds it without loading one.
    last_default: Mutex<Option<ModelKey>>,
}

impl LlmInstances {
    pub(crate) fn new() -> Self {
        Self {
            pool: InstancePool::new(),
            last_default: Mutex::new(None),
        }
    }

    /// Read before the settings a request is built from.
    pub(crate) fn epoch(&self) -> u64 {
        self.pool.epoch()
    }

    /// The model `request.key` names, loading it with `load` when it is not
    /// loaded. A new bundled server is admitted first: it must fit in the GPU
    /// memory the others leave, after evicting idle models if that makes
    /// room; otherwise the request is refused with the sizes and who holds
    /// the memory.
    pub(crate) async fn acquire<F, Fut>(
        &self,
        request: Acquire<'_>,
        budget: &dyn GpuBudget,
        load: F,
    ) -> Result<Acquired>
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = Result<CreatedLlm>>,
    {
        let Acquire {
            key,
            epoch,
            role,
            holder,
            local,
            floor,
        } = request;
        let model = key.model.clone();
        let launch = key.launch;
        let loaded: Loaded<LoadedLlm, Holder> = self
            .pool
            .get_or_load(key, epoch, holder, || async {
                if let (Some(local), Some(launch)) = (local, launch) {
                    self.admit(&model, local, launch, floor, role, budget)
                        .await?;
                }
                Ok::<_, AppError>(LoadedLlm::new(load().await?, &model, local))
            })
            .await?;
        if !loaded.reused {
            if let Some(reason) = loaded.value.degraded() {
                tracing::warn!(
                    model = %model,
                    instance_id = loaded.instance_id,
                    reason,
                    "model.degraded"
                );
            }
        }
        Ok(Acquired {
            loaded: loaded.value,
            instance_id: loaded.instance_id,
            reused: loaded.reused,
            lease: loaded.lease,
        })
    }

    /// Remember `key` as the Settings default chat model just handed out.
    pub(crate) fn remember_default(&self, key: &ModelKey) {
        *self
            .last_default
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(key.clone());
    }

    /// Stop handing out the models `cause` makes stale; whoever holds one
    /// keeps it until their work ends. Returns the keys that matched.
    pub(crate) fn invalidate(&self, cause: &InvalidationCause) -> Vec<ModelKey> {
        let matched = self
            .pool
            .invalidate(|key| invalidation::matches(key, cause));
        tracing::info!(
            matched = ?matched.iter().map(|key| key.model.to_string()).collect::<Vec<_>>(),
            cause = invalidation::describe(cause),
            "model.invalidated"
        );
        matched
    }

    /// Every model loaded now, with who holds it.
    pub(crate) fn snapshot(&self) -> Vec<LoadedInstance> {
        self.pool
            .snapshot()
            .into_iter()
            .map(|entry| LoadedInstance {
                model: entry.key.model,
                provider: entry.value.llm.provider_name().to_owned(),
                context_tokens: u32::try_from(entry.value.llm.max_context_tokens())
                    .unwrap_or(u32::MAX),
                in_flight: entry.stats.in_flight,
                pinned_by: entry.stats.holders.iter().map(Holder::label).collect(),
                loaded_at: entry.stats.loaded_at.to_rfc3339(),
                last_used_at: entry.stats.last_used_at.to_rfc3339(),
                gpu_bytes: entry.value.gpu_bytes,
                degraded: entry.value.degraded().map(str::to_owned),
            })
            .collect()
    }

    /// Admit a new server for `local`: launch when it fits; else evict idle
    /// models and launch if that made room; else refuse.
    async fn admit(
        &self,
        model: &ModelRef,
        local: &LocalModel,
        launch: LocalServerOptions,
        floor: Option<u32>,
        role: ModelRole,
        budget: &dyn GpuBudget,
    ) -> Result<()> {
        let first = budget.admission(&local.path, launch, floor).await;
        let Admission::Short { needed, spare } = first else {
            admission::log(&local.name, first, &[]);
            return Ok(());
        };
        let evicted = self.pool.evict_idle(
            |_, loaded| loaded.gpu_bytes.filter(|bytes| *bytes > 0),
            needed.total().saturating_sub(spare),
        );
        for (key, bytes, stats) in &evicted {
            tracing::info!(
                model = %key.model,
                instance_id = stats.instance_id,
                bytes,
                idle_for_ms = (chrono::Utc::now() - stats.last_used_at)
                    .num_milliseconds()
                    .max(0),
                reason = "admission",
                "model.evicted"
            );
        }
        let evicted: Vec<(String, u64)> = evicted
            .into_iter()
            .map(|(key, bytes, _)| (key.model.to_string(), bytes))
            .collect();
        let outcome = if evicted.is_empty() {
            first
        } else {
            budget.admission(&local.path, launch, floor).await
        };
        admission::log(&local.name, outcome, &evicted);
        match outcome {
            Admission::Short { needed, spare } => Err(refusal::refused(
                role,
                Some(model),
                "no_memory",
                refusal::no_memory(role, &local.name, needed, spare, &self.pinners()),
            )),
            _ => Ok(()),
        }
    }

    /// The models held by running work, with their GPU memory and holders.
    fn pinners(&self) -> Vec<refusal::Pinner> {
        self.pool
            .snapshot()
            .into_iter()
            .filter(|entry| entry.stats.in_flight > 0)
            .filter_map(|entry| {
                Some(refusal::Pinner {
                    name: entry.value.name.clone(),
                    gpu_bytes: entry.value.gpu_bytes.filter(|bytes| *bytes > 0)?,
                    holders: entry.stats.holders,
                })
            })
            .collect()
    }
}

impl Default for LlmInstances {
    fn default() -> Self {
        Self::new()
    }
}

/// The Settings default chat model as last handed out, if it is still loaded.
/// Never loads one.
impl LoadedChatModelPort for LlmInstances {
    fn current_model(&self) -> Option<Arc<dyn LLMPort>> {
        let key = self
            .last_default
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()?;
        self.pool.get(&key).map(|loaded| Arc::clone(&loaded.llm))
    }
}

#[cfg(test)]
mod tests;
