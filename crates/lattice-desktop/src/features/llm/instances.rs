//! Every LLM loaded right now, for every role, in one pool keyed by what it
//! is: a model, how its server is launched and how its client generates,
//! never a role, so two roles resolving to one key share a client and server.
//! Before a new bundled server starts, admission prices it against free GPU
//! memory and unloads idle models to make room; nothing in use is unloaded,
//! and nothing is refused on that estimate.

use std::sync::{Arc, Mutex};

use lattice_core::application::contracts::settings::LLMSettingsDto;
use lattice_core::application::ports::llm_port::{InvalidationCause, ModelRole, RequestLoad};
use lattice_core::application::ports::{LLMPort, LoadedChatModelPort};
use lattice_core::domain::models::model_ref::ModelRef;
use lattice_core::shared::error::{AppError, Result};

use crate::features::llm::engine::factory::{CreatedLlm, LocalServerOptions, ServerFacts};
use crate::features::llm::engine::sidecar_manager::{
    resident_bytes_of, Admission, ServerBuffers, SidecarConfig,
};
use crate::features::llm::engine::GenerationConfig;
use crate::features::llm::loading::LocalModel;
use crate::infrastructure::ml::instance_pool::{Alive, InstancePool, Lease, Loaded};

mod admission;
mod invalidation;
mod pinned;
pub(crate) mod refusal;

pub(crate) use admission::{ContextFit, GpuBudget, IdleServer, SidecarBudget};
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

impl ModelKey {
    /// The server it is a client of. Keys that differ only in tuning are
    /// clients of one server, which is freed only when the last of them goes.
    fn server(&self) -> (ModelRef, LaunchProfile) {
        (self.model.clone(), self.launch)
    }
}

/// What a server outside the pool may take for a place on the GPU. What a
/// claim may not take stays where it is, and a server its claim does not seat
/// runs elsewhere.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GpuClaim {
    /// Memory nothing holds, the margin included.
    Spare,
    /// That, and what idle servers outside the pool hold.
    IdleServers,
    /// That, and what idle models hold.
    IdleModels,
}

impl GpuClaim {
    /// Whether `outcome` seats a server. A GPU that does not report its memory
    /// may be full, so only the claim that may unload takes the chance.
    fn seated_by(self, outcome: Admission) -> bool {
        match outcome {
            Admission::Fits { .. } | Admission::Tight { .. } | Admission::Shared => true,
            Admission::Unmetered => self == Self::IdleModels,
            Admission::Short { .. } | Admission::NoOffload => false,
        }
    }
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
            (Some(server), Some(local)) if server.n_gpu_layers > 0 => Some(resident_bytes_of(
                &local.path,
                server.context_size,
                ServerBuffers::chat(server.kv_cache_quantization),
            )),
            (Some(_), _) => Some(0),
            (None, _) => None,
        };
        Self {
            llm: created.llm,
            name: display_name(model, local),
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

/// The name the user knows `model` by. A local model is named by `local`.
fn display_name(model: &ModelRef, local: Option<&LocalModel>) -> String {
    match (local, model) {
        (Some(local), _) => local.name.clone(),
        (None, ModelRef::Local { model_id }) => model_id.clone(),
        (None, ModelRef::Ollama { model } | ModelRef::LlamaCpp { model }) => model.clone(),
        (None, ModelRef::Cloud { model, .. }) => model.clone(),
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
            ModelRole::Verifier => "verifier",
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
    /// The name the user knows it by.
    pub(crate) name: String,
    /// What its backend's scheduler is running and holding now.
    pub(crate) requests: Option<RequestLoad>,
}

/// A model being loaded now: its server starting, or its client connecting.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LoadingModel {
    pub(crate) name: String,
    pub(crate) role: ModelRole,
}

/// The loads in progress, each until its guard drops.
#[derive(Default)]
struct Loads {
    next: u64,
    live: Vec<(u64, LoadingModel)>,
}

/// One load's place in [`Loads`]; dropping it, however the load ended,
/// takes it out.
struct LoadingGuard<'a> {
    instances: &'a LlmInstances,
    id: u64,
}

impl Drop for LoadingGuard<'_> {
    fn drop(&mut self) {
        self.instances
            .loads
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .live
            .retain(|(id, _)| *id != self.id);
        self.instances.moved();
    }
}

/// Learning work refused because the Learning model's window was smaller
/// than it needed, kept until that model serves learning work with a window
/// that covers it, so Settings → Models can say so beside the model.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LearningShortfall {
    pub(crate) model: ModelRef,
    /// The window the work needed, in tokens.
    pub(crate) needed: u32,
    /// The window the model had, in tokens.
    pub(crate) window: u32,
    /// Whether that window was the one saved for the model in Settings →
    /// Models, rather than the one it was trained for or its server reports.
    pub(crate) window_is_saved: bool,
}

/// What [`LlmInstances::acquire`] loads, and for whom.
pub(crate) struct Acquire<'a> {
    pub(crate) key: ModelKey,
    /// Read before the settings the key and its loader were built from.
    pub(crate) epoch: u64,
    /// Who pins it for the work; `None` hands it out unpinned.
    pub(crate) holder: Option<Holder>,
    /// The downloaded model behind a local key, which admission prices.
    pub(crate) local: Option<&'a LocalModel>,
    /// What the model is loaded for, which the Activity drawer names while
    /// it starts.
    pub(crate) role: ModelRole,
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
    /// Taken by every admission and, when a new server is to start on the
    /// GPU, held until it has started. A server is invisible to admission
    /// until it is running, so two models admitted side by side would each
    /// be priced against memory the other is about to take.
    admission: tokio::sync::Mutex<()>,
    /// The Settings default chat model most recently handed out, so work that
    /// only reads "the loaded chat model" finds it without loading one.
    last_default: Mutex<Option<ModelKey>>,
    /// The last learning work refused for a window too small, until the
    /// model it named covers that window.
    learning_shortfall: Mutex<Option<LearningShortfall>>,
    /// Servers outside the pool that admission may stop while they are idle,
    /// once no idle LLM is left to unload: the embedding server.
    evictable: Vec<Arc<dyn IdleServer>>,
    /// Models being loaded now.
    loads: Mutex<Loads>,
    /// Moved when a load starts or ends.
    changed: tokio::sync::watch::Sender<u64>,
}

impl LlmInstances {
    pub(crate) fn new() -> Self {
        Self {
            pool: InstancePool::new(),
            admission: tokio::sync::Mutex::new(()),
            last_default: Mutex::new(None),
            learning_shortfall: Mutex::new(None),
            evictable: Vec::new(),
            loads: Mutex::default(),
            changed: tokio::sync::watch::channel(0).0,
        }
    }

    /// The models being loaded now, oldest load first.
    pub(crate) fn loading(&self) -> Vec<LoadingModel> {
        self.loads
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .live
            .iter()
            .map(|(_, loading)| loading.clone())
            .collect()
    }

    /// Moves when a load starts or ends.
    pub(crate) fn changes(&self) -> tokio::sync::watch::Receiver<u64> {
        self.changed.subscribe()
    }

    fn moved(&self) {
        self.changed
            .send_modify(|count| *count = count.wrapping_add(1));
    }

    /// Lists `loading` until the guard drops.
    fn loading_guard(&self, loading: LoadingModel) -> LoadingGuard<'_> {
        let id = {
            let mut loads = self.loads.lock().unwrap_or_else(|p| p.into_inner());
            let id = loads.next;
            loads.next = loads.next.wrapping_add(1);
            loads.live.push((id, loading));
            id
        };
        self.moved();
        LoadingGuard {
            instances: self,
            id,
        }
    }

    /// The same, with `server` among those admission may stop while idle.
    pub(crate) fn with_evictable(mut self, server: Arc<dyn IdleServer>) -> Self {
        self.evictable.push(server);
        self
    }

    /// Decide whether a server outside the pool, started for `config` and
    /// named `name`, runs on the GPU, unloading what `claim` allows when it
    /// comes up short. A seated server that is new gets the admission turn
    /// back, to hold until it has started, as for a model [`Self::acquire`]
    /// loads; one that is not seated is to run elsewhere.
    pub(crate) async fn claim_gpu(
        &self,
        name: &str,
        config: &SidecarConfig,
        claim: GpuClaim,
        budget: &dyn GpuBudget,
    ) -> (bool, Option<tokio::sync::MutexGuard<'_, ()>>) {
        let turn = self.admission.lock().await;
        let first = budget.server_admission(config).await;
        let evicted = match (first, claim) {
            (Admission::Short { .. }, GpuClaim::IdleServers) => {
                self.evict_idle_servers(first.shortfall().unwrap_or(0), Vec::new())
            }
            (Admission::Short { .. }, GpuClaim::IdleModels) => self.evict_for(first),
            _ => Vec::new(),
        };
        let outcome = if evicted.is_empty() {
            first
        } else {
            budget.server_admission(config).await
        };
        let seated = claim.seated_by(outcome);
        tracing::info!(
            model = name,
            ?claim,
            ?outcome,
            ?evicted,
            seated,
            "model.admission.claim"
        );
        let turn = if seated {
            starting(outcome, turn)
        } else {
            None
        };
        (seated, turn)
    }

    /// Read before the settings a request is built from.
    pub(crate) fn epoch(&self) -> u64 {
        self.pool.epoch()
    }

    /// The model `request.key` names, loading it with `load` when it is not
    /// loaded, after admitting a new bundled server one at a time (see `admit`).
    /// Requests for one key share one load.
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
            holder,
            local,
            role,
        } = request;
        let model = key.model.clone();
        let launch = key.launch;
        // The admission turn of a server that is starting, held until the
        // pool has published it, so the next admission both prices it and
        // can name who holds it.
        let starting = Mutex::new(None);
        let loaded: Result<Loaded<LoadedLlm, Holder>> = self
            .pool
            .get_or_load(key, epoch, holder, || async {
                let _loading = self.loading_guard(LoadingModel {
                    name: display_name(&model, local),
                    role,
                });
                if let (Some(local), Some(launch)) = (local, launch) {
                    let turn = self.admit(local, launch, budget).await;
                    *starting
                        .lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner()) = turn;
                }
                Ok::<_, AppError>(LoadedLlm::new(load().await?, &model, local))
            })
            .await;
        drop(starting);
        let loaded = loaded?;
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

    /// Remember that learning work was refused for a window too small.
    pub(crate) fn record_learning_shortfall(&self, shortfall: LearningShortfall) {
        *self
            .learning_shortfall
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(shortfall);
    }

    /// `model` served learning work with a window of `window` tokens: a
    /// shortfall it now covers is over.
    pub(crate) fn learning_window_served(&self, model: &ModelRef, window: u32) {
        let mut shortfall = self
            .learning_shortfall
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if shortfall
            .as_ref()
            .is_some_and(|short| short.model == *model && window >= short.needed)
        {
            *shortfall = None;
        }
    }

    /// The last learning work refused for a window too small, while the
    /// model it named has not covered that window since.
    pub(crate) fn learning_shortfall(&self) -> Option<LearningShortfall> {
        self.learning_shortfall
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
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
                name: entry.value.name.clone(),
                requests: entry.value.llm.requests(),
            })
            .collect()
    }

    /// Admit a new server for `local`: when it does not fit beside the
    /// others, unload idle models to make room, then start it either way.
    /// When a new server is to take GPU memory, the admission turn comes back
    /// and must be held until that server has started.
    async fn admit(
        &self,
        local: &LocalModel,
        launch: LocalServerOptions,
        budget: &dyn GpuBudget,
    ) -> Option<tokio::sync::MutexGuard<'_, ()>> {
        let turn = self.admission.lock().await;
        let first = budget.admission(&local.path, launch).await;
        let evicted = self.evict_for(first);
        let outcome = if evicted.is_empty() {
            first
        } else {
            budget.admission(&local.path, launch).await
        };
        admission::log(&local.name, outcome, &evicted);
        starting(outcome, turn)
    }

    /// Unload idle models until a server that came out `outcome` would fit:
    /// idle LLMs first, then idle servers outside the pool. Nothing when it
    /// already fits. Returns what was unloaded and the bytes each freed.
    fn evict_for(&self, outcome: Admission) -> Vec<(String, u64)> {
        let Some(shortfall) = outcome.shortfall() else {
            return Vec::new();
        };
        let evicted = self.pool.evict_idle(
            ModelKey::server,
            |_, loaded| loaded.gpu_bytes.filter(|bytes| *bytes > 0),
            shortfall,
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
        self.evict_idle_servers(shortfall, evicted)
    }

    /// Stop idle servers outside the pool until `evicted`, what has been
    /// unloaded so far, and they free `shortfall` bytes between them.
    fn evict_idle_servers(
        &self,
        shortfall: u64,
        mut evicted: Vec<(String, u64)>,
    ) -> Vec<(String, u64)> {
        let mut freed = evicted
            .iter()
            .map(|(_, bytes)| *bytes)
            .fold(0, u64::saturating_add);
        for server in &self.evictable {
            if freed >= shortfall {
                break;
            }
            let Some(bytes) = server.idle_gpu_bytes() else {
                continue;
            };
            server.stop();
            let name = server.name();
            tracing::info!(model = %name, bytes, reason = "admission", "model.evicted");
            freed = freed.saturating_add(bytes);
            evicted.push((name, bytes));
        }
        evicted
    }
}

/// The admission turn, kept only when `outcome` starts a server on the GPU,
/// whether or not it fits. A shared server is already running, and one on
/// the CPU or on a GPU that does not report its memory has nothing to be
/// priced against.
fn starting<T>(outcome: Admission, turn: T) -> Option<T> {
    matches!(
        outcome,
        Admission::Fits { .. } | Admission::Tight { .. } | Admission::Short { .. }
    )
    .then_some(turn)
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
