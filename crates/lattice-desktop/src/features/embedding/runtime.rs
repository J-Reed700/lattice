//! Embedding lifecycle: readiness, single-flight loading, and failure memory.
use std::{
    future::Future,
    sync::Arc,
    time::{Duration, Instant},
};

use crate::features::embedding::loading::EmbeddingLoader;
use lattice_core::application::ports::{EmbeddingPort, LoadedEmbeddingModelPort};
use lattice_core::shared::error::{AppError, Result};

/// How long a model that failed to load is not loaded again.
const RELOAD_AFTER_FAILURE: Duration = Duration::from_secs(60);

/// A port that failed to load, with the generation and time it failed at.
struct FailedLoad {
    generation: u64,
    failed_at: Instant,
    port: Arc<dyn EmbeddingPort>,
}

use crate::infrastructure::ml::model_cache::ModelCache;

pub(crate) struct EmbeddingRuntime {
    cache: ModelCache<(), Arc<dyn EmbeddingPort>>,
    // Serializes failure updates with invalidation, never held across awaits.
    failed: parking_lot::Mutex<Option<FailedLoad>>,
}

impl EmbeddingRuntime {
    pub(crate) fn new() -> Self {
        Self {
            cache: ModelCache::new(),
            failed: parking_lot::Mutex::new(None),
        }
    }

    /// Whether the embedding model is being loaded now.
    pub(crate) fn is_loading(&self) -> bool {
        self.cache.is_loading()
    }

    /// Moves when a load of the embedding model starts and when it ends.
    pub(crate) fn changes(&self) -> tokio::sync::watch::Receiver<u64> {
        self.cache.changes()
    }

    pub(crate) fn invalidate(&self) {
        let mut failed = self.failed.lock();
        self.cache.invalidate();
        *failed = None;
    }

    fn invalidate_generation(&self, generation: u64) {
        let mut failed = self.failed.lock();
        if self.cache.generation() == generation {
            self.cache.invalidate();
            *failed = None;
        }
    }

    /// The failed port of the current generation, however long ago it failed.
    fn failed_port(&self) -> Option<Arc<dyn EmbeddingPort>> {
        let failed = self.failed.lock();
        failed
            .as_ref()
            .filter(|f| f.generation == self.cache.generation())
            .map(|f| Arc::clone(&f.port))
    }

    /// The failed port of `generation` while it is within the reload delay.
    fn recent_failure(&self, generation: u64) -> Option<Arc<dyn EmbeddingPort>> {
        let failed = self.failed.lock();
        failed
            .as_ref()
            .filter(|f| {
                f.generation == generation
                    && self.cache.generation() == generation
                    && f.failed_at.elapsed() < RELOAD_AFTER_FAILURE
            })
            .map(|f| Arc::clone(&f.port))
    }

    pub(crate) async fn get_or_load<F, Fut>(&self, load: F) -> Result<Arc<dyn EmbeddingPort>>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = Result<Arc<dyn EmbeddingPort>>>,
    {
        let generation = self.cache.generation();
        if let Some(model) = self.cache.get(&(), generation) {
            match model.is_ready().await {
                Ok(true) => return Ok(model),
                Ok(false) => self.invalidate_generation(generation),
                Err(error) => {
                    tracing::warn!(%error, "Cached embedding readiness check failed");
                    self.invalidate_generation(generation);
                }
            }
        }
        // Capture before the loader reads configuration or the active model.
        let generation = self.cache.generation();
        self.cache.get_or_load_with_policy((), generation, || async {
            if let Some(port) = self.recent_failure(generation) {
                return Ok::<_, AppError>((Some(port), false));
            }
            let model = load().await?;
            let cacheable = match model.is_ready().await {
                Ok(ready) => ready,
                Err(error) => {
                    tracing::warn!(%error, "Loaded embedding readiness check failed; not caching");
                    false
                }
            };
            {
                let mut failed = self.failed.lock();
                if self.cache.generation() == generation {
                    *failed = (!cacheable && model.load_failure().is_some()).then(|| FailedLoad {
                        generation,
                        failed_at: Instant::now(),
                        port: Arc::clone(&model),
                    });
                }
            }
            Ok((Some(model), cacheable))
        }).await?.ok_or_else(|| AppError::InternalError("Embedding loader returned no model".into()))
    }
}

/// The process's embedding runtime paired with the loader it falls back to:
/// everything `Container::get_or_load_embedding` uses, as a value that can be
/// held without the container. The vault note sink holds one, because the
/// vault writer is owned by the container and must not point back at it.
pub(crate) struct ActiveEmbedding {
    runtime: Arc<EmbeddingRuntime>,
    loader: EmbeddingLoader,
}

impl ActiveEmbedding {
    pub(crate) fn new(runtime: Arc<EmbeddingRuntime>, loader: EmbeddingLoader) -> Self {
        Self { runtime, loader }
    }

    /// A ready cached model, or a single-flight load through the loader,
    /// subject to the runtime's reload delay after a failed load.
    pub(crate) async fn get_or_load(&self) -> Result<Arc<dyn EmbeddingPort>> {
        self.runtime
            .get_or_load(|| async { Ok(self.loader.load().await) })
            .await
    }
}

impl LoadedEmbeddingModelPort for EmbeddingRuntime {
    fn current_model(&self) -> Option<Arc<dyn EmbeddingPort>> {
        self.cache
            .get(&(), self.cache.generation())
            .or_else(|| self.failed_port())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct TestModel(AtomicUsize);

    #[async_trait::async_trait]
    impl EmbeddingPort for TestModel {
        async fn embed_single(&self, _: &str) -> Result<Vec<f32>> {
            Ok(vec![1.0])
        }
        async fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
            Ok(vec![vec![1.0]; texts.len()])
        }
        fn dimension(&self) -> usize {
            1
        }
        async fn is_ready(&self) -> Result<bool> {
            match self.0.load(Ordering::SeqCst) {
                2 => Err(AppError::Other("readiness failed".into())),
                n => Ok(n == 1),
            }
        }
    }

    fn ready() -> Arc<dyn EmbeddingPort> {
        Arc::new(TestModel(AtomicUsize::new(1)))
    }

    #[tokio::test]
    async fn dynamic_consumers_follow_provider_and_preserve_degraded_errors() {
        use crate::features::embedding::service::DynamicEmbedding;

        let runtime = Arc::new(EmbeddingRuntime::new());
        let port = DynamicEmbedding::new(runtime.clone());
        let service = DynamicEmbedding::new(runtime.clone());
        assert!(matches!(
            port.embed_single("text").await,
            Err(AppError::AiModelsNotInstalled(_))
        ));
        assert!(!port.is_ready().await.unwrap());
        runtime.get_or_load(|| async { Ok(ready()) }).await.unwrap();
        assert_eq!(port.embed_single("text").await.unwrap(), vec![1.0]);
        assert_eq!(service.embed_single("text").await.unwrap(), vec![1.0]);
        runtime.invalidate();
        assert!(matches!(
            service.embed_single("text").await,
            Err(AppError::AiModelsNotInstalled(_))
        ));
        assert!(!port.is_ready().await.unwrap());
    }

    fn failed() -> Arc<dyn EmbeddingPort> {
        Arc::new(
            lattice_core::application::ports::UnavailableEmbedding::failed_to_load(
                AppError::ModelLoadFailed("weights are corrupt".into()),
            ),
        )
    }

    #[tokio::test]
    async fn a_dynamic_port_over_a_failed_load_reports_the_load_error() {
        use crate::features::embedding::service::DynamicEmbedding;

        let runtime = Arc::new(EmbeddingRuntime::new());
        let port = DynamicEmbedding::new(runtime.clone());
        runtime
            .get_or_load(|| async { Ok(failed()) })
            .await
            .unwrap();

        let error = port.embed_query("text").await.unwrap_err();

        assert!(
            matches!(&error, AppError::ModelLoadFailed(m) if m == "weights are corrupt"),
            "the load failure was lost: {error}"
        );
        assert!(port.load_failure().is_some());
    }

    #[tokio::test]
    async fn a_dynamic_port_over_a_failed_load_refuses_to_split_text_with_the_load_error() {
        use crate::features::embedding::service::DynamicEmbedding;

        let runtime = Arc::new(EmbeddingRuntime::new());
        let port = DynamicEmbedding::new(runtime.clone());
        runtime
            .get_or_load(|| async { Ok(failed()) })
            .await
            .unwrap();

        let error = port.split_text("text", "").unwrap_err();

        assert!(matches!(&error, AppError::ModelLoadFailed(m) if m == "weights are corrupt"));
    }

    #[tokio::test]
    async fn a_failed_load_is_not_retried_within_the_delay() {
        let runtime = EmbeddingRuntime::new();
        let calls = AtomicUsize::new(0);
        for _ in 0..2 {
            let port = runtime
                .get_or_load(|| async {
                    calls.fetch_add(1, Ordering::SeqCst);
                    Ok(failed())
                })
                .await
                .unwrap();
            assert!(port.load_failure().is_some());
        }
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn concurrent_loads_share_a_ready_model() {
        let runtime = EmbeddingRuntime::new();
        let calls = AtomicUsize::new(0);
        let load = || async {
            calls.fetch_add(1, Ordering::SeqCst);
            tokio::task::yield_now().await;
            Ok(ready())
        };
        let (a, b) = tokio::join!(runtime.get_or_load(load), runtime.get_or_load(load));
        assert!(Arc::ptr_eq(&a.unwrap(), &b.unwrap()));
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn degraded_and_readiness_error_models_are_returned_but_not_cached() {
        let runtime = EmbeddingRuntime::new();
        for state in [0, 2] {
            let model: Arc<dyn EmbeddingPort> = Arc::new(TestModel(AtomicUsize::new(state)));
            let returned = runtime
                .get_or_load(|| async { Ok(model.clone()) })
                .await
                .unwrap();
            assert!(Arc::ptr_eq(&model, &returned));
            assert!(runtime.current_model().is_none());
        }
        runtime.get_or_load(|| async { Ok(ready()) }).await.unwrap();
        assert!(runtime.current_model().is_some());
    }

    #[tokio::test]
    async fn unhealthy_cached_model_is_replaced() {
        for state in [0, 2] {
            let runtime = EmbeddingRuntime::new();
            let original = Arc::new(TestModel(AtomicUsize::new(1)));
            runtime
                .get_or_load(|| async { Ok(original.clone() as Arc<dyn EmbeddingPort>) })
                .await
                .unwrap();
            original.0.store(state, Ordering::SeqCst);
            let replacement = ready();
            let result = runtime
                .get_or_load(|| async { Ok(replacement.clone()) })
                .await
                .unwrap();
            assert!(Arc::ptr_eq(&result, &replacement));
        }
    }

    #[tokio::test]
    async fn a_failed_load_is_retried_after_the_delay_and_forgotten_on_invalidation() {
        let runtime = EmbeddingRuntime::new();
        runtime
            .get_or_load(|| async { Ok(failed()) })
            .await
            .unwrap();
        let again = runtime
            .get_or_load(|| async { panic!("the failure must suppress the loader") })
            .await
            .unwrap();
        assert!(again.load_failure().is_some());
        assert!(runtime.current_model().unwrap().load_failure().is_some());
        runtime.failed.lock().as_mut().unwrap().failed_at =
            Instant::now() - Duration::from_secs(61);
        runtime.get_or_load(|| async { Ok(ready()) }).await.unwrap();
        assert!(runtime.failed.lock().is_none());
        runtime.invalidate();
        runtime
            .get_or_load(|| async { Ok(failed()) })
            .await
            .unwrap();
        runtime.invalidate();
        assert!(runtime.current_model().is_none());
        runtime.get_or_load(|| async { Ok(ready()) }).await.unwrap();
    }

    #[tokio::test]
    async fn invalidated_load_cannot_publish_model_or_failure() {
        let runtime = EmbeddingRuntime::new();
        runtime
            .get_or_load(|| async {
                runtime.invalidate();
                Ok(ready())
            })
            .await
            .unwrap();
        assert!(runtime.current_model().is_none());
        runtime
            .get_or_load(|| async {
                runtime.invalidate();
                Ok(failed())
            })
            .await
            .unwrap();
        assert!(runtime.failed.lock().is_none());
        assert!(runtime.current_model().is_none());
        runtime.get_or_load(|| async { Ok(ready()) }).await.unwrap();
    }

    #[tokio::test]
    async fn stale_health_check_cannot_evict_new_generation() {
        let runtime = EmbeddingRuntime::new();
        let old_generation = runtime.cache.generation();
        runtime.invalidate();
        let model = ready();
        runtime
            .get_or_load(|| async { Ok(model.clone()) })
            .await
            .unwrap();
        runtime.invalidate_generation(old_generation);
        assert!(Arc::ptr_eq(&runtime.current_model().unwrap(), &model));
    }

    #[tokio::test]
    async fn cancelled_load_leaves_no_failure_and_allows_retry() {
        let runtime = Arc::new(EmbeddingRuntime::new());
        let task_runtime = runtime.clone();
        let (started, ready_rx) = tokio::sync::oneshot::channel();
        let task = tokio::spawn(async move {
            task_runtime
                .get_or_load(|| async {
                    started.send(()).unwrap();
                    std::future::pending::<Result<Arc<dyn EmbeddingPort>>>().await
                })
                .await
        });
        ready_rx.await.unwrap();
        task.abort();
        assert!(task.await.err().unwrap().is_cancelled());
        assert!(runtime.failed.lock().is_none());
        tokio::time::timeout(
            Duration::from_secs(1),
            runtime.get_or_load(|| async { Ok(ready()) }),
        )
        .await
        .unwrap()
        .unwrap();
    }
}
