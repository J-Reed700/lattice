//! Single-flight model cache with invalidation-safe publication.
use std::{future::Future, sync::RwLock};

struct State<K, V> {
    generation: u64,
    entry: Option<(K, V)>,
}

pub(crate) struct ModelCache<K, V> {
    state: RwLock<State<K, V>>,
    load_lock: tokio::sync::Mutex<()>,
}

impl<K: PartialEq, V: Clone> ModelCache<K, V> {
    pub(crate) fn new() -> Self {
        Self {
            state: RwLock::new(State {
                generation: 0,
                entry: None,
            }),
            load_lock: tokio::sync::Mutex::new(()),
        }
    }

    /// Capture before reading configuration so an intervening invalidation
    /// also prevents a queued request with old configuration from publishing.
    pub(crate) fn generation(&self) -> u64 {
        self.state
            .read()
            .unwrap_or_else(|p| p.into_inner())
            .generation
    }

    pub(crate) fn invalidate(&self) {
        let mut state = self.state.write().unwrap_or_else(|p| p.into_inner());
        state.generation = state.generation.wrapping_add(1);
        state.entry = None;
    }

    pub(crate) fn get(&self, key: &K, generation: u64) -> Option<V> {
        let state = self.state.read().unwrap_or_else(|p| p.into_inner());
        if state.generation != generation {
            return None;
        }
        state
            .entry
            .as_ref()
            .filter(|(k, _)| k == key)
            .map(|(_, v)| v.clone())
    }

    /// Errors and absent models are retryable, never cached. Cancellation
    /// releases the load lock. Invalidation does not cancel existing callers:
    /// they may receive their result, but cannot repopulate the new generation.
    pub(crate) async fn get_or_load<E, F, Fut>(
        &self,
        key: K,
        generation: u64,
        load: F,
    ) -> Result<Option<V>, E>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = Result<Option<V>, E>>,
    {
        self.get_or_load_with_policy(key, generation, || async {
            load().await.map(|value| (value, true))
        })
        .await
    }

    /// The loader may return a usable value without publishing it (for
    /// example, a degraded embedding model). This still serializes loads.
    pub(crate) async fn get_or_load_with_policy<E, F, Fut>(
        &self,
        key: K,
        generation: u64,
        load: F,
    ) -> Result<Option<V>, E>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = Result<(Option<V>, bool), E>>,
    {
        if let Some(value) = self.get(&key, generation) {
            return Ok(Some(value));
        }
        let _guard = self.load_lock.lock().await;
        if let Some(value) = self.get(&key, generation) {
            return Ok(Some(value));
        }
        let (value, cacheable) = load().await?;
        if let Some(value) = value.as_ref().filter(|_| cacheable) {
            let mut state = self.state.write().unwrap_or_else(|p| p.into_inner());
            if state.generation == generation {
                state.entry = Some((key, value.clone()));
            }
        }
        Ok(value)
    }
}

impl crate::application::ports::LoadedChatModelPort
    for ModelCache<(), std::sync::Arc<dyn crate::application::ports::LLMPort>>
{
    fn current_model(&self) -> Option<std::sync::Arc<dyn crate::application::ports::LLMPort>> {
        self.get(&(), self.generation())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::ports::{LLMPort, LoadedChatModelPort};
    use crate::features::llm::engine::factory::MockLLMPort;
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };

    #[tokio::test]
    async fn chat_provider_tracks_invalidation_without_revoking_existing_handles() {
        let cache = ModelCache::new();
        let provider: &dyn LoadedChatModelPort = &cache;
        assert!(provider.current_model().is_none());
        let model: Arc<dyn LLMPort> = Arc::new(MockLLMPort::new());
        cache
            .get_or_load((), cache.generation(), || async {
                Ok::<_, ()>(Some(model.clone()))
            })
            .await
            .unwrap();
        let retained = provider.current_model().unwrap();
        assert!(Arc::ptr_eq(&retained, &model));
        cache.invalidate();
        assert!(provider.current_model().is_none());
        assert_eq!(retained.model_name(), "mock-llm");
    }

    #[tokio::test]
    async fn chat_provider_never_exposes_invalidated_load() {
        let cache = ModelCache::new();
        let provider: &dyn LoadedChatModelPort = &cache;
        let model: Arc<dyn LLMPort> = Arc::new(MockLLMPort::new());
        let loaded = cache
            .get_or_load((), cache.generation(), || async {
                cache.invalidate();
                Ok::<_, ()>(Some(model))
            })
            .await
            .unwrap();
        assert!(loaded.is_some());
        assert!(provider.current_model().is_none());
    }

    #[tokio::test]
    async fn concurrent_misses_load_once() {
        let cache = ModelCache::new();
        let calls = AtomicUsize::new(0);
        let load = || async {
            calls.fetch_add(1, Ordering::SeqCst);
            tokio::task::yield_now().await;
            Ok::<_, ()>(Some(42))
        };
        let (a, b) = tokio::join!(
            cache.get_or_load("a", 0, load),
            cache.get_or_load("a", 0, load)
        );
        assert_eq!(a, Ok(Some(42)));
        assert_eq!(b, a);
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn invalidation_during_load_prevents_stale_publication() {
        let cache = ModelCache::new();
        let result = cache
            .get_or_load("a", 0, || async {
                cache.invalidate();
                Ok::<_, ()>(Some(1))
            })
            .await;
        assert_eq!(result, Ok(Some(1)));
        assert_eq!(cache.get(&"a", cache.generation()), None);
        assert_eq!(
            cache
                .get_or_load("a", cache.generation(), || async { Ok::<_, ()>(Some(2)) })
                .await,
            Ok(Some(2))
        );
    }

    #[tokio::test]
    async fn old_configuration_cannot_replace_new_generation() {
        let cache = ModelCache::new();
        let old = cache.generation();
        cache.invalidate();
        cache
            .get_or_load("a", cache.generation(), || async { Ok::<_, ()>(Some(2)) })
            .await
            .unwrap();
        cache
            .get_or_load("a", old, || async { Ok::<_, ()>(Some(1)) })
            .await
            .unwrap();
        assert_eq!(cache.get(&"a", cache.generation()), Some(2));
    }

    #[tokio::test]
    async fn failures_and_absence_are_retryable_and_keys_are_distinct() {
        let cache = ModelCache::new();
        assert_eq!(
            cache
                .get_or_load("a", 0, || async { Err::<Option<i32>, _>("failed") })
                .await,
            Err("failed")
        );
        assert_eq!(
            cache
                .get_or_load("a", 0, || async { Ok::<_, ()>(None) })
                .await,
            Ok(None)
        );
        assert_eq!(
            cache
                .get_or_load("a", 0, || async { Ok::<_, ()>(Some(1)) })
                .await,
            Ok(Some(1))
        );
        assert_eq!(
            cache
                .get_or_load("b", 0, || async { Ok::<_, ()>(Some(2)) })
                .await,
            Ok(Some(2))
        );
    }

    #[tokio::test]
    async fn cancelled_loader_releases_lock_for_retry() {
        let cache = Arc::new(ModelCache::new());
        let (started, ready) = tokio::sync::oneshot::channel();
        let task_cache = cache.clone();
        let task = tokio::spawn(async move {
            task_cache
                .get_or_load("a", 0, || async {
                    started.send(()).unwrap();
                    std::future::pending::<Result<Option<i32>, ()>>().await
                })
                .await
        });
        ready.await.unwrap();
        task.abort();
        assert!(task.await.unwrap_err().is_cancelled());
        let result = tokio::time::timeout(
            std::time::Duration::from_secs(1),
            cache.get_or_load("a", 0, || async { Ok::<_, ()>(Some(42)) }),
        )
        .await
        .unwrap();
        assert_eq!(result, Ok(Some(42)));
    }
}
