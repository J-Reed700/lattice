use super::*;
use crate::features::llm::engine::sidecar_manager::Footprint;
use lattice_core::application::contracts::settings::LocalModelConfigDto;
use lattice_core::application::ports::llm_port::{
    CompletionRequest, CompletionResponse, ModelOrigin,
};
use lattice_core::domain::models::model_ref::CloudProvider;
use lattice_core::shared::runtime::jobs::JobFailure;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

const GIB: u64 = 1024 * 1024 * 1024;

/// A loaded model that says when it is unloaded.
struct FakeLlm {
    name: String,
    unloaded: Arc<AtomicBool>,
}

impl Drop for FakeLlm {
    fn drop(&mut self) {
        self.unloaded.store(true, Ordering::SeqCst);
    }
}

#[async_trait::async_trait]
impl LLMPort for FakeLlm {
    async fn complete(&self, _: &CompletionRequest) -> Result<CompletionResponse> {
        Ok(CompletionResponse::from_text(self.name.clone()))
    }
    fn model_name(&self) -> &str {
        &self.name
    }
    fn max_context_tokens(&self) -> usize {
        8_704
    }
    async fn is_ready(&self) -> Result<bool> {
        Ok(true)
    }
}

/// A budget that answers from `answer`, given whether the watched model has
/// been unloaded, and counts how often it was asked.
struct FakeBudget {
    answer: Box<dyn Fn(bool) -> Admission + Send + Sync>,
    watched: Arc<AtomicBool>,
    asked: AtomicUsize,
}

impl FakeBudget {
    fn always(admission: Admission) -> Self {
        Self::until_unloaded(Arc::new(AtomicBool::new(false)), move |_| admission)
    }

    fn until_unloaded(
        watched: Arc<AtomicBool>,
        answer: impl Fn(bool) -> Admission + Send + Sync + 'static,
    ) -> Self {
        Self {
            answer: Box::new(answer),
            watched,
            asked: AtomicUsize::new(0),
        }
    }
}

#[async_trait::async_trait]
impl GpuBudget for FakeBudget {
    async fn admission(
        &self,
        _: &std::path::Path,
        _: LocalServerOptions,
        _: Option<u32>,
    ) -> Admission {
        self.asked.fetch_add(1, Ordering::SeqCst);
        (self.answer)(self.watched.load(Ordering::SeqCst))
    }
    async fn auto_window(&self, _: &std::path::Path, _: LocalServerOptions) -> Option<u32> {
        None
    }
    fn trained_window(&self, _: &std::path::Path) -> Option<u32> {
        None
    }
}

/// A downloaded model whose file holds `bytes` bytes of weights.
fn local_model(dir: &tempfile::TempDir, name: &str, bytes: u64) -> LocalModel {
    let path = dir.path().join(format!("{name}.gguf"));
    std::fs::File::create(&path)
        .unwrap()
        .set_len(bytes)
        .unwrap();
    LocalModel {
        name: name.to_owned(),
        path,
    }
}

fn local_key(model_id: &str) -> ModelKey {
    ModelKey {
        model: ModelRef::Local {
            model_id: model_id.to_owned(),
        },
        launch: Some(LocalServerOptions::default()),
        tuning: Tuning::Standard,
    }
}

fn on_gpu(name: &str, unloaded: &Arc<AtomicBool>) -> CreatedLlm {
    CreatedLlm {
        llm: Arc::new(FakeLlm {
            name: name.to_owned(),
            unloaded: Arc::clone(unloaded),
        }),
        server: Some(ServerFacts {
            context_size: 8_704,
            n_gpu_layers: 99,
            degraded: None,
        }),
    }
}

fn chat_holder(title: &str) -> Holder {
    Holder {
        role: ModelRole::Chat,
        work: title.to_owned(),
    }
}

async fn acquire_local(
    instances: &LlmInstances,
    budget: &FakeBudget,
    local: &LocalModel,
    holder: Option<Holder>,
    unloaded: &Arc<AtomicBool>,
) -> Result<Acquired> {
    instances
        .acquire(
            Acquire {
                key: local_key(&local.name),
                epoch: instances.epoch(),
                role: ModelRole::Chat,
                holder,
                local: Some(local),
                floor: None,
            },
            budget,
            || async { Ok(on_gpu(&local.name, unloaded)) },
        )
        .await
}

fn short() -> Admission {
    Admission::Short {
        needed: Footprint {
            weights: 5 * GIB,
            cache: GIB,
            window: 8_704,
        },
        spare: 3 * GIB,
    }
}

fn fits() -> Admission {
    Admission::Fits {
        needed: Footprint {
            weights: 5 * GIB,
            cache: GIB,
            window: 8_704,
        },
        spare: 7 * GIB,
    }
}

#[tokio::test]
async fn a_server_that_fits_is_loaded_after_one_check() {
    let dir = tempfile::tempdir().unwrap();
    let local = local_model(&dir, "ornith", 1_000);
    let instances = LlmInstances::new();
    let budget = FakeBudget::always(fits());
    let unloaded = Arc::new(AtomicBool::new(false));

    let acquired = acquire_local(&instances, &budget, &local, None, &unloaded)
        .await
        .unwrap();

    assert_eq!(acquired.loaded.name, "ornith");
    assert_eq!(
        acquired.loaded.gpu_bytes,
        Some(1_000),
        "an unreadable header counts its weights"
    );
    assert_eq!(budget.asked.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn an_idle_model_is_evicted_to_make_room_and_the_new_one_loads() {
    let dir = tempfile::tempdir().unwrap();
    let (idle, wanted) = (
        local_model(&dir, "idle", 7 * GIB),
        local_model(&dir, "wanted", 1),
    );
    let instances = LlmInstances::new();
    let idle_unloaded = Arc::new(AtomicBool::new(false));
    acquire_local(
        &instances,
        &FakeBudget::always(fits()),
        &idle,
        None,
        &idle_unloaded,
    )
    .await
    .unwrap();

    let budget = FakeBudget::until_unloaded(Arc::clone(&idle_unloaded), |unloaded| {
        if unloaded {
            fits()
        } else {
            short()
        }
    });
    let wanted_unloaded = Arc::new(AtomicBool::new(false));
    let acquired = acquire_local(&instances, &budget, &wanted, None, &wanted_unloaded)
        .await
        .expect("evicting the idle model makes room");

    assert_eq!(acquired.loaded.name, "wanted");
    assert!(
        idle_unloaded.load(Ordering::SeqCst),
        "the idle model was unloaded"
    );
    assert_eq!(
        budget.asked.load(Ordering::SeqCst),
        2,
        "checked again after evicting"
    );
    let loaded: Vec<_> = instances.snapshot().into_iter().map(|m| m.model).collect();
    assert_eq!(loaded, vec![local_key("wanted").model]);
}

#[tokio::test]
async fn no_room_beside_running_work_is_refused_naming_it_and_unloads_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let (busy, wanted) = (
        local_model(&dir, "Qwen3.5-27B", 16 * GIB),
        local_model(&dir, "Ornith-1.5-9B", 1),
    );
    let instances = LlmInstances::new();
    let busy_unloaded = Arc::new(AtomicBool::new(false));
    let held = acquire_local(
        &instances,
        &FakeBudget::always(fits()),
        &busy,
        Some(chat_holder("Research notes")),
        &busy_unloaded,
    )
    .await
    .unwrap()
    .into_port();
    let loads = AtomicUsize::new(0);

    let refused = instances
        .acquire(
            Acquire {
                key: local_key("Ornith-1.5-9B"),
                epoch: instances.epoch(),
                role: ModelRole::Chat,
                holder: Some(chat_holder("Other chat")),
                local: Some(&wanted),
                floor: None,
            },
            &FakeBudget::always(short()),
            || async {
                loads.fetch_add(1, Ordering::SeqCst);
                Ok(on_gpu("Ornith-1.5-9B", &Arc::new(AtomicBool::new(false))))
            },
        )
        .await;

    let Err(error) = refused else {
        panic!("expected a refusal for memory");
    };
    // Memory held by running work is not an outage: a job fails once with
    // this message rather than retrying in the background.
    assert!(!JobFailure::is_transient(&error), "{error}");
    let AppError::ModelLoadFailed(message) = error else {
        panic!("expected a load failure, got {error:?}");
    };
    assert_eq!(
        message,
        "Can't load Ornith-1.5-9B for this chat: it needs 6.0 GB (5.0 GB weights + 1.0 GB \
         context at 8,704 tokens) and 3.0 GB is free. Qwen3.5-27B (16.0 GB) is answering in \
         'Research notes'. Wait for them to finish, stop them, or use Qwen3.5-27B for this chat."
    );
    assert_eq!(loads.load(Ordering::SeqCst), 0, "nothing was started");
    assert!(
        !busy_unloaded.load(Ordering::SeqCst),
        "the busy model was not unloaded"
    );
    assert_eq!(held.model_name(), "Qwen3.5-27B");
}

#[tokio::test]
async fn admission_is_skipped_when_the_gpu_reports_no_memory_and_for_remote_models() {
    let dir = tempfile::tempdir().unwrap();
    let local = local_model(&dir, "ornith", 1);
    let instances = LlmInstances::new();
    let unmetered = FakeBudget::always(Admission::Unmetered);
    assert!(acquire_local(
        &instances,
        &unmetered,
        &local,
        None,
        &Arc::new(AtomicBool::new(false))
    )
    .await
    .is_ok());

    let never = FakeBudget::always(short());
    let remote = instances
        .acquire(
            Acquire {
                key: ModelKey {
                    model: ModelRef::Ollama {
                        model: "qwen3:8b".into(),
                    },
                    launch: None,
                    tuning: Tuning::Standard,
                },
                epoch: instances.epoch(),
                role: ModelRole::Chat,
                holder: None,
                local: None,
                floor: None,
            },
            &never,
            || async {
                Ok(CreatedLlm {
                    llm: Arc::new(FakeLlm {
                        name: "qwen3:8b".into(),
                        unloaded: Arc::new(AtomicBool::new(false)),
                    }),
                    server: None,
                })
            },
        )
        .await
        .unwrap();
    assert_eq!(never.asked.load(Ordering::SeqCst), 0);
    assert_eq!(remote.loaded.gpu_bytes, None);
}

#[tokio::test]
async fn a_pinned_model_counts_its_holder_until_the_last_clone_drops() {
    let dir = tempfile::tempdir().unwrap();
    let local = local_model(&dir, "ornith", 1);
    let instances = LlmInstances::new();
    let pinned = acquire_local(
        &instances,
        &FakeBudget::always(fits()),
        &local,
        Some(chat_holder("Notes")),
        &Arc::new(AtomicBool::new(false)),
    )
    .await
    .unwrap()
    .into_port();
    let handed_to_a_subtask = Arc::clone(&pinned);
    let in_use = |instances: &LlmInstances| {
        let loaded = instances.snapshot().remove(0);
        (loaded.in_flight, loaded.pinned_by)
    };

    assert_eq!(in_use(&instances), (1, vec!["chat: Notes".to_owned()]));
    assert_eq!(
        pinned
            .complete(&CompletionRequest::default())
            .await
            .unwrap()
            .text,
        "ornith"
    );
    drop(pinned);
    assert_eq!(in_use(&instances).0, 1, "a clone still holds it");
    drop(handed_to_a_subtask);
    assert_eq!(in_use(&instances), (0, vec![]));
}

#[tokio::test]
async fn a_degraded_start_is_carried_with_the_model() {
    let dir = tempfile::tempdir().unwrap();
    let local = local_model(&dir, "ornith", 4_096);
    let instances = LlmInstances::new();
    let reason = "requested llama-server (GPU, 8704 ctx) but running llama-server (CPU, 4096 ctx)";

    let acquired = instances
        .acquire(
            Acquire {
                key: local_key("ornith"),
                epoch: instances.epoch(),
                role: ModelRole::Chat,
                holder: None,
                local: Some(&local),
                floor: None,
            },
            &FakeBudget::always(fits()),
            || async {
                Ok(CreatedLlm {
                    server: Some(ServerFacts {
                        context_size: 4_096,
                        n_gpu_layers: 0,
                        degraded: Some(reason.to_owned()),
                    }),
                    ..on_gpu("ornith", &Arc::new(AtomicBool::new(false)))
                })
            },
        )
        .await
        .unwrap();

    assert_eq!(acquired.loaded.degraded(), Some(reason));
    assert_eq!(
        acquired.loaded.gpu_bytes,
        Some(0),
        "a CPU server holds no GPU memory"
    );
    assert_eq!(instances.snapshot()[0].degraded.as_deref(), Some(reason));
}

#[tokio::test]
async fn the_loaded_chat_model_is_the_last_default_handed_out_and_never_a_load() {
    let dir = tempfile::tempdir().unwrap();
    let local = local_model(&dir, "ornith", 1);
    let instances = LlmInstances::new();
    assert!(instances.current_model().is_none());

    let acquired = acquire_local(
        &instances,
        &FakeBudget::always(fits()),
        &local,
        None,
        &Arc::new(AtomicBool::new(false)),
    )
    .await
    .unwrap();
    assert!(
        instances.current_model().is_none(),
        "not remembered as the default"
    );
    instances.remember_default(&local_key("ornith"));
    let current = instances.current_model().expect("the default is loaded");
    assert!(Arc::ptr_eq(&current, &acquired.loaded.llm));

    instances.invalidate(&InvalidationCause::ModelDeleted {
        model_id: "ornith".into(),
    });
    assert!(instances.current_model().is_none());
    assert_eq!(
        current.model_name(),
        "ornith",
        "holders keep what they hold"
    );
}

fn settings() -> LLMSettingsDto {
    LLMSettingsDto {
        local_model_configs: [("a".to_owned(), LocalModelConfigDto::default())].into(),
        ..LLMSettingsDto::default()
    }
}

/// One key of every kind, launched as `settings` launch them.
fn keys(settings: &LLMSettingsDto) -> Vec<ModelKey> {
    let model = |model: ModelRef| ModelKey {
        launch: crate::features::llm::loading::launch_profile_for(&model, settings),
        model,
        tuning: Tuning::Standard,
    };
    vec![
        model(ModelRef::Local {
            model_id: "a".into(),
        }),
        model(ModelRef::Local {
            model_id: "b".into(),
        }),
        model(ModelRef::Ollama {
            model: "qwen3:8b".into(),
        }),
        model(ModelRef::LlamaCpp {
            model: "qwen3.5-27b".into(),
        }),
        model(ModelRef::Cloud {
            provider: CloudProvider::Openai,
            model: "gpt-x".into(),
        }),
        ModelKey {
            tuning: Tuning::Router,
            ..model(ModelRef::Local {
                model_id: "b".into(),
            })
        },
    ]
}

/// The models `cause` makes stale, by their log form.
fn stale(before: &LLMSettingsDto, cause: InvalidationCause) -> Vec<String> {
    keys(before)
        .iter()
        .filter(|key| invalidation::matches(key, &cause))
        .map(|key| {
            format!(
                "{}{}",
                key.model,
                if key.tuning == Tuning::Router {
                    " (router)"
                } else {
                    ""
                }
            )
        })
        .collect()
}

fn changed(before: &LLMSettingsDto, change: impl FnOnce(&mut LLMSettingsDto)) -> InvalidationCause {
    let mut after = before.clone();
    change(&mut after);
    InvalidationCause::SettingsChanged {
        previous: Some(Box::new(before.clone())),
        current: Some(Box::new(after)),
    }
}

#[test]
fn what_a_role_resolves_to_unloads_nothing() {
    let before = settings();
    let resolution_only = changed(&before, |s| {
        s.provider = lattice_core::application::contracts::settings::LLMProvider::Ollama;
        s.model = "another".into();
        s.learning_model = Some(ModelRef::Local {
            model_id: "b".into(),
        });
        s.llama_cpp.model = "another".into();
        s.router.model = "another".into();
        s.ollama_utility_model = "another".into();
    });
    assert!(stale(&before, resolution_only).is_empty());
}

#[test]
fn a_models_own_window_drops_only_that_model() {
    let before = settings();
    let cause = changed(&before, |s| {
        s.local_model_configs.get_mut("a").unwrap().context_length = Some(8_704);
    });
    assert_eq!(stale(&before, cause), vec!["local:a"]);
}

#[test]
fn settings_each_client_is_built_from_drop_just_those_clients() {
    let before = settings();
    assert_eq!(
        stale(&before, changed(&before, |s| s.temperature += 0.1)),
        vec![
            "local:a",
            "local:b",
            "ollama:qwen3:8b",
            "llamacpp:qwen3.5-27b",
            "cloud:openai/gpt-x"
        ],
        "sampling is baked into every client but the router's, which has its own"
    );
    assert_eq!(
        stale(&before, changed(&before, |s| s.router.temperature += 0.1)),
        vec!["local:b (router)"]
    );
    assert_eq!(
        stale(
            &before,
            changed(&before, |s| s.ollama_url = "http://elsewhere:11434".into())
        ),
        vec!["ollama:qwen3:8b"]
    );
    assert_eq!(
        stale(
            &before,
            changed(&before, |s| s.llama_cpp.url =
                "http://elsewhere:8080".into())
        ),
        vec!["llamacpp:qwen3.5-27b"]
    );
    assert_eq!(
        stale(
            &before,
            changed(&before, |s| s.local_context_window = Some(16_384))
        ),
        vec!["local:a", "local:b", "local:b (router)"],
        "a server option every local model launches with"
    );
}

#[test]
fn deletions_keys_and_unreadable_settings_drop_what_they_name() {
    let before = settings();
    assert_eq!(
        stale(
            &before,
            InvalidationCause::ModelDeleted {
                model_id: "b".into()
            }
        ),
        vec!["local:b", "local:b (router)"]
    );
    assert_eq!(
        stale(
            &before,
            InvalidationCause::CredentialsChanged {
                provider: CloudProvider::Openai
            }
        ),
        vec!["cloud:openai/gpt-x"]
    );
    assert!(stale(
        &before,
        InvalidationCause::CredentialsChanged {
            provider: CloudProvider::Anthropic
        }
    )
    .is_empty());
    assert_eq!(
        stale(
            &before,
            InvalidationCause::SettingsChanged {
                previous: None,
                current: Some(Box::new(before.clone())),
            }
        )
        .len(),
        keys(&before).len(),
        "nothing to compare against: every model is stale"
    );
}

#[test]
fn refusals_say_what_is_wrong_with_the_numbers_and_the_fix() {
    use refusal::{no_tools, not_downloaded, window_too_small, Window};
    let message = |error: AppError| match error {
        AppError::InvalidConfig(message) | AppError::AiModelsNotInstalled(message) => message,
        other => panic!("unexpected refusal {other:?}"),
    };

    assert_eq!(
        message(window_too_small(
            ModelRole::Learning,
            "Qwen3.5-9B",
            20_480,
            Window::Set(8_704),
            Some(32_768),
        )),
        "Learning work needs a context window of at least 20,480 tokens. Your Learning model, \
         Qwen3.5-9B, is set to 8,704 in Settings → Models. Set it to Auto (≈32,768 on this Mac \
         right now) or 20,480 or more, or choose a larger Learning model."
    );
    assert!(
        !message(window_too_small(
            ModelRole::Learning,
            "Qwen3.5-9B",
            20_480,
            Window::Set(8_704),
            Some(16_384),
        ))
        .contains("Auto"),
        "Auto is not offered when it would not be enough"
    );
    assert_eq!(
        message(window_too_small(
            ModelRole::Chat,
            "Ornith-1.5-9B",
            12_000,
            Window::Running(8_192),
            None,
        )),
        "This chat needs a context window of at least 12,000 tokens. Ornith-1.5-9B is running \
         with 8,192 on this Mac right now. Set its context window to 12,000 or more in Settings \
         → Models, or choose a larger model for this chat."
    );
    assert_eq!(
        message(no_tools(ModelRole::Chat, "Ornith-1.5-9B")),
        "Ornith-1.5-9B cannot call tools, and this chat needs them. Choose a tool-capable model \
         for this chat."
    );
    assert_eq!(
        message(not_downloaded(
            ModelRole::Chat,
            ModelOrigin::ChatOverride,
            "ornith-1.5-9b"
        )),
        "This chat is set to ornith-1.5-9b, which is no longer downloaded."
    );
    assert_eq!(
        message(not_downloaded(
            ModelRole::Learning,
            ModelOrigin::LearningRole,
            "ornith-1.5-9b"
        )),
        "Your Learning model, ornith-1.5-9b, is not downloaded any more. Choose one in \
         Settings → Models under Learning."
    );
    assert_eq!(refusal::tokens(1_048_576), "1,048,576");
    assert_eq!(refusal::tokens(999), "999");
}

#[test]
fn a_learning_shortfall_lasts_until_its_model_serves_a_window_that_covers_it() {
    let instances = LlmInstances::new();
    let model = local_key("Ornith-1.5-9B").model;
    let other = local_key("Qwen3.5-27B").model;
    let shortfall = LearningShortfall {
        model: model.clone(),
        needed: 20_480,
        window: 8_704,
        window_is_saved: true,
    };
    instances.record_learning_shortfall(shortfall.clone());

    instances.learning_window_served(&model, 16_384);
    instances.learning_window_served(&other, 32_768);
    assert_eq!(
        instances.learning_shortfall(),
        Some(shortfall),
        "a window still too small, or another model, leaves it standing"
    );

    instances.learning_window_served(&model, 20_480);
    assert_eq!(instances.learning_shortfall(), None);
}

/// A GPU of `usable` bytes, shared by the servers started on it. A server
/// counts once its load has finished, as the registry counts one only after
/// it has started.
struct MeteredBudget {
    usable: u64,
    running: std::sync::Mutex<u64>,
}

impl MeteredBudget {
    fn footprint(path: &std::path::Path) -> Footprint {
        Footprint {
            weights: std::fs::metadata(path).unwrap().len(),
            cache: GIB,
            window: 8_704,
        }
    }

    fn started(&self, path: &std::path::Path) {
        *self.running.lock().unwrap() += Self::footprint(path).total();
    }
}

#[async_trait::async_trait]
impl GpuBudget for MeteredBudget {
    async fn admission(
        &self,
        path: &std::path::Path,
        _: LocalServerOptions,
        _: Option<u32>,
    ) -> Admission {
        let needed = Self::footprint(path);
        let spare = self.usable.saturating_sub(*self.running.lock().unwrap());
        if needed.total() <= spare {
            Admission::Fits { needed, spare }
        } else {
            Admission::Short { needed, spare }
        }
    }
    async fn auto_window(&self, _: &std::path::Path, _: LocalServerOptions) -> Option<u32> {
        None
    }
    fn trained_window(&self, _: &std::path::Path) -> Option<u32> {
        None
    }
}

/// Start `local` for the chat `title` on `budget`, taking a while to start
/// as a real server does.
async fn start_metered(
    instances: &LlmInstances,
    budget: &MeteredBudget,
    local: &LocalModel,
    title: &str,
    starts: &AtomicUsize,
) -> Result<Acquired> {
    const STARTUP: std::time::Duration = std::time::Duration::from_millis(20);
    instances
        .acquire(
            Acquire {
                key: local_key(&local.name),
                epoch: instances.epoch(),
                role: ModelRole::Chat,
                holder: Some(chat_holder(title)),
                local: Some(local),
                floor: None,
            },
            budget,
            || async {
                starts.fetch_add(1, Ordering::SeqCst);
                tokio::time::sleep(STARTUP).await;
                budget.started(&local.path);
                Ok(on_gpu(&local.name, &Arc::new(AtomicBool::new(false))))
            },
        )
        .await
}

#[tokio::test]
async fn two_models_loading_at_once_are_admitted_one_after_the_other() {
    let dir = tempfile::tempdir().unwrap();
    let (large, small) = (
        local_model(&dir, "Qwen3.5-27B", 15 * GIB),
        local_model(&dir, "Ornith-1.5-9B", 6 * GIB),
    );
    let instances = LlmInstances::new();
    let budget = MeteredBudget {
        usable: 20 * GIB,
        running: std::sync::Mutex::new(0),
    };
    let (large_starts, small_starts) = (AtomicUsize::new(0), AtomicUsize::new(0));

    let (first, second, other) = tokio::join!(
        start_metered(&instances, &budget, &large, "Research notes", &large_starts),
        start_metered(&instances, &budget, &large, "Second tab", &large_starts),
        start_metered(&instances, &budget, &small, "Other chat", &small_starts),
    );

    // One load for one key, however many ask for it at once.
    let (first, second) = (first.unwrap(), second.unwrap());
    assert_eq!(large_starts.load(Ordering::SeqCst), 1);
    assert_eq!(first.instance_id, second.instance_id);
    // The other model was priced after the first had started, not against
    // the memory both saw free.
    assert_eq!(
        small_starts.load(Ordering::SeqCst),
        0,
        "nothing was started"
    );
    let Err(AppError::ModelLoadFailed(message)) = other else {
        panic!("expected a refusal for memory");
    };
    // The model that won is named with what it holds; whether the second
    // tab has joined it yet depends on which waiter runs first.
    assert!(
        message.starts_with(
            "Can't load Ornith-1.5-9B for this chat: it needs 7.0 GB (6.0 GB weights + 1.0 GB \
             context at 8,704 tokens) and 4.0 GB is free. Qwen3.5-27B (15.0 GB) is answering \
             in 'Research notes'"
        ),
        "{message}"
    );
}

#[tokio::test]
async fn a_server_two_roles_hold_is_named_once_in_a_refusal() {
    let dir = tempfile::tempdir().unwrap();
    let (busy, wanted) = (
        local_model(&dir, "Qwen3.5-27B", 16 * GIB),
        local_model(&dir, "Ornith-1.5-9B", 1),
    );
    let instances = LlmInstances::new();
    let unloaded = Arc::new(AtomicBool::new(false));
    let mut held = Vec::new();
    for (tuning, holder) in [
        (Tuning::Standard, chat_holder("Research notes")),
        (
            Tuning::Utility,
            Holder {
                role: ModelRole::Utility,
                work: String::new(),
            },
        ),
    ] {
        let acquired = instances
            .acquire(
                Acquire {
                    key: ModelKey {
                        tuning,
                        ..local_key(&busy.name)
                    },
                    epoch: instances.epoch(),
                    role: holder.role,
                    holder: Some(holder),
                    local: Some(&busy),
                    floor: None,
                },
                &FakeBudget::always(fits()),
                || async { Ok(on_gpu(&busy.name, &unloaded)) },
            )
            .await
            .unwrap();
        held.push(acquired.into_port());
    }

    let refused = acquire_local(
        &instances,
        &FakeBudget::always(short()),
        &wanted,
        Some(chat_holder("Other chat")),
        &Arc::new(AtomicBool::new(false)),
    )
    .await;

    let Err(AppError::ModelLoadFailed(message)) = refused else {
        panic!("expected a refusal for memory");
    };
    assert_eq!(message.matches("Qwen3.5-27B (").count(), 1, "{message}");
    assert!(message.contains("Research notes"), "{message}");
}
