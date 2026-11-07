//! A stand-in embedding model for tests of any feature that keeps a passage
//! index (a folder's, a course's), so each tests its index against the same
//! fake rather than one of its own.
//!
//! [`FakeEmbedder`] makes bag-of-words vectors: texts sharing words point the
//! same way, which is all "by meaning" needs to mean in a test. It records
//! every text it embeds, and a test can slow it, hold it at a gate, cancel a
//! build after some batches (a cut) or make it fail (a model that stops).

use super::live::EmbedderSource;
use async_trait::async_trait;
use lattice_core::application::ports::EmbeddingPort;
use lattice_core::shared::{AppError, Result};
use parking_lot::Mutex;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio_util::sync::CancellationToken;

/// The fake model's dimension.
pub const DIM: usize = 16;

/// Holds batches until the test lets them through: `allowed` pass at once,
/// the next waits for a permit and says so on `blocked`.
pub struct BatchGate {
    pub permits: tokio::sync::Semaphore,
    pub blocked: tokio::sync::Notify,
}

impl BatchGate {
    pub fn new(allowed: usize) -> Arc<Self> {
        Arc::new(Self {
            permits: tokio::sync::Semaphore::new(allowed),
            blocked: tokio::sync::Notify::new(),
        })
    }
}

/// Bag-of-words vectors, counting every passage it embeds.
pub struct FakeEmbedder {
    pub identity: String,
    pub embedded: Mutex<Vec<String>>,
    pub batches: AtomicUsize,
    pub delay_ms: AtomicU64,
    pub batch_gate: Mutex<Option<Arc<BatchGate>>>,
    /// Cancels the token once this many batches have been embedded: a build
    /// cut part-way through.
    pub cancel_after: Mutex<Option<(usize, CancellationToken)>>,
    /// Batches after this many fail: a model that stops answering.
    pub fail_after: AtomicUsize,
}

impl FakeEmbedder {
    pub fn new(identity: &str) -> Arc<Self> {
        Arc::new(Self {
            identity: identity.to_string(),
            embedded: Mutex::new(Vec::new()),
            batches: AtomicUsize::new(0),
            delay_ms: AtomicU64::new(0),
            batch_gate: Mutex::new(None),
            cancel_after: Mutex::new(None),
            fail_after: AtomicUsize::new(usize::MAX),
        })
    }

    pub fn embedded(&self) -> Vec<String> {
        self.embedded.lock().clone()
    }

    pub fn forget_calls(&self) {
        self.embedded.lock().clear();
        self.batches.store(0, Ordering::SeqCst);
    }

    pub fn vector(text: &str) -> Vec<f32> {
        let mut vector = [0.01f32; DIM];
        for word in text
            .split(|c: char| !c.is_alphanumeric())
            .filter(|word| word.len() > 2)
        {
            let bucket = word.to_lowercase().bytes().fold(7usize, |hash, byte| {
                hash.wrapping_mul(31).wrapping_add(byte as usize)
            }) % DIM;
            vector[bucket] += 1.0;
        }
        let norm = vector.iter().map(|x| x * x).sum::<f32>().sqrt();
        vector.iter().map(|x| x / norm).collect()
    }
}

#[async_trait]
impl EmbeddingPort for FakeEmbedder {
    async fn embed_single(&self, text: &str) -> Result<Vec<f32>> {
        Ok(Self::vector(text))
    }
    async fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        let gate = self.batch_gate.lock().clone();
        if let Some(gate) = gate {
            match gate.permits.try_acquire() {
                Ok(permit) => permit.forget(),
                Err(_) => {
                    gate.blocked.notify_one();
                    if let Ok(permit) = gate.permits.acquire().await {
                        permit.forget();
                    }
                }
            }
        }
        let delay = self.delay_ms.load(Ordering::SeqCst);
        if delay > 0 {
            tokio::time::sleep(Duration::from_millis(delay)).await;
        }
        if self.batches.load(Ordering::SeqCst) >= self.fail_after.load(Ordering::SeqCst) {
            return Err(AppError::EmbeddingFailed {
                reason: "the fake model stopped answering".into(),
            });
        }
        let done = self.batches.fetch_add(1, Ordering::SeqCst) + 1;
        self.embedded.lock().extend(texts.iter().cloned());
        if let Some((after, token)) = self.cancel_after.lock().as_ref() {
            if done >= *after {
                token.cancel();
            }
        }
        Ok(texts.iter().map(|text| Self::vector(text)).collect())
    }
    fn dimension(&self) -> usize {
        DIM
    }
    fn model_identity(&self) -> String {
        self.identity.clone()
    }
    async fn is_ready(&self) -> Result<bool> {
        Ok(true)
    }
}

/// An embedder source that always answers with the same model, or with
/// none: a test's stand-in for the app's active embedding model.
pub struct FixedEmbedders(pub Mutex<Option<Arc<FakeEmbedder>>>);

impl FixedEmbedders {
    pub fn new(embedder: Option<Arc<FakeEmbedder>>) -> Arc<Self> {
        Arc::new(Self(Mutex::new(embedder)))
    }

    /// The model the app has active from now on.
    pub fn set(&self, embedder: Option<Arc<FakeEmbedder>>) {
        *self.0.lock() = embedder;
    }
}

#[async_trait]
impl EmbedderSource for FixedEmbedders {
    async fn embedder(&self) -> Option<Arc<dyn EmbeddingPort>> {
        self.0
            .lock()
            .clone()
            .map(|embedder| embedder as Arc<dyn EmbeddingPort>)
    }
}
