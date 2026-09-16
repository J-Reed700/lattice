//! Embedding lifecycle: readiness, single-flight loading, and retry cooldown.
use std::{
    future::Future,
    sync::Arc,
    time::{Duration, Instant},
};

use crate::application::ports::{EmbeddingPort, LoadedEmbeddingModelPort};
use crate::shared::error::{AppError, Result};

use super::model_cache::ModelCache;

pub(crate) struct EmbeddingRuntime {
    cache: ModelCache<(), Arc<dyn EmbeddingPort>>,
    // Serializes cooldown updates with invalidation, never held across awaits.
    cooldown: parking_lot::Mutex<Option<(Instant, AppError)>>,
}

impl EmbeddingRuntime {
    pub(crate) fn new() -> Self {
        Self {
            cache: ModelCache::new(),
            cooldown: parking_lot::Mutex::new(None),
        }
    }

    pub(crate) fn invalidate(&self) {
        let mut cooldown = self.cooldown.lock();
        self.cache.invalidate();
        *cooldown = None;
    }

    fn invalidate_generation(&self, generation: u64) {
        let mut cooldown = self.cooldown.lock();
        if self.cache.generation() == generation {
            self.cache.invalidate();
            *cooldown = None;
        }
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
            {
                let cooldown = self.cooldown.lock();
                if self.cache.generation() == generation {
                    if let Some((failed_at, error)) = cooldown.as_ref() {
                        if failed_at.elapsed() < Duration::from_secs(60) {
                            return Err(error.clone());
                        }
                    }
                }
            }
            let result = load().await;
            {
                let mut cooldown = self.cooldown.lock();
                if self.cache.generation() == generation {
                    *cooldown = result.as_ref().err().map(|e| (Instant::now(), e.clone()));
                }
            }
            let model = result?;
            let cacheable = match model.is_ready().await {
                Ok(ready) => ready,
                Err(error) => {
                    tracing::warn!(%error, "Loaded embedding readiness check failed; not caching");
                    false
                }
            };
            Ok((Some(model), cacheable))
        }).await?.ok_or_else(|| AppError::InternalError("Embedding loader returned no model".into()))
    }
}

impl LoadedEmbeddingModelPort for EmbeddingRuntime {
    fn current_model(&self) -> Option<Arc<dyn EmbeddingPort>> {
        self.cache.get(&(), self.cache.generation())
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
        use crate::features::embedding::service::{DynamicEmbedding, DynamicEmbeddingService};
        use crate::features::embedding::EmbeddingServiceTrait;

        let runtime = Arc::new(EmbeddingRuntime::new());
        let port = DynamicEmbedding::new(runtime.clone());
        let service = DynamicEmbeddingService::new(runtime.clone());
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
    async fn cooldown_suppresses_retries_until_expiry_or_invalidation() {
        let runtime = EmbeddingRuntime::new();
        assert!(runtime
            .get_or_load(|| async { Err(AppError::Other("load failed".into())) })
            .await
            .is_err());
        assert!(runtime
            .get_or_load(|| async { panic!("cooldown must suppress loader") })
            .await
            .is_err());
        runtime.cooldown.lock().as_mut().unwrap().0 = Instant::now() - Duration::from_secs(61);
        runtime.get_or_load(|| async { Ok(ready()) }).await.unwrap();
        assert!(runtime.cooldown.lock().is_none());
        runtime.invalidate();
        assert!(runtime
            .get_or_load(|| async { Err(AppError::Other("load failed".into())) })
            .await
            .is_err());
        runtime.invalidate();
        runtime.get_or_load(|| async { Ok(ready()) }).await.unwrap();
    }

    #[tokio::test]
    async fn invalidated_load_cannot_publish_model_or_failure_cooldown() {
        let runtime = EmbeddingRuntime::new();
        runtime
            .get_or_load(|| async {
                runtime.invalidate();
                Ok(ready())
            })
            .await
            .unwrap();
        assert!(runtime.current_model().is_none());
        assert!(runtime
            .get_or_load(|| async {
                runtime.invalidate();
                Err(AppError::Other("stale failure".into()))
            })
            .await
            .is_err());
        assert!(runtime.cooldown.lock().is_none());
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
    async fn cancelled_load_leaves_no_cooldown_and_allows_retry() {
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
        assert!(runtime.cooldown.lock().is_none());
        tokio::time::timeout(
            Duration::from_secs(1),
            runtime.get_or_load(|| async { Ok(ready()) }),
        )
        .await
        .unwrap()
        .unwrap();
    }
}
