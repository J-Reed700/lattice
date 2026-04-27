use crate::infrastructure::indexing::actor::{IndexingActor, PauseGate};
use crate::infrastructure::indexing::progress::ProgressTracker;
use crate::features::embedding::service::EmbeddingService;
use crate::shared::error::{AppError, Result};
use sqlx::SqlitePool;
use std::marker::PhantomData;
use std::path::PathBuf;
use std::sync::Arc;
use tokenizers::Tokenizer;
use tokio::sync::{mpsc, Mutex};

pub struct Uninitialized;
pub struct WithPool;
pub struct WithEmbedder;
pub struct WithTokenizer;
pub struct Ready;

pub struct IndexingServiceBuilder<State = Uninitialized> {
    pool: Option<SqlitePool>,
    vault_path: Option<PathBuf>,
    embedder: Option<Arc<EmbeddingService>>,
    tokenizer: Option<Arc<Tokenizer>>,
    progress_tracker: Option<Arc<Mutex<ProgressTracker>>>,
    channel_size: usize,
    _state: PhantomData<State>,
}

impl IndexingServiceBuilder<Uninitialized> {
    pub fn new() -> Self {
        Self {
            pool: None,
            vault_path: None,
            embedder: None,
            tokenizer: None,
            progress_tracker: None,
            channel_size: 100,
            _state: PhantomData,
        }
    }

    pub fn pool(self, pool: SqlitePool) -> IndexingServiceBuilder<WithPool> {
        IndexingServiceBuilder {
            pool: Some(pool),
            vault_path: self.vault_path,
            embedder: self.embedder,
            tokenizer: self.tokenizer,
            progress_tracker: self.progress_tracker,
            channel_size: self.channel_size,
            _state: PhantomData,
        }
    }
}

impl Default for IndexingServiceBuilder<Uninitialized> {
    fn default() -> Self {
        Self::new()
    }
}

impl IndexingServiceBuilder<WithPool> {
    pub fn vault_path(mut self, path: PathBuf) -> Self {
        self.vault_path = Some(path);
        self
    }

    pub fn embedder(self, embedder: Arc<EmbeddingService>) -> IndexingServiceBuilder<WithEmbedder> {
        IndexingServiceBuilder {
            pool: self.pool,
            vault_path: self.vault_path,
            embedder: Some(embedder),
            tokenizer: self.tokenizer,
            progress_tracker: self.progress_tracker,
            channel_size: self.channel_size,
            _state: PhantomData,
        }
    }
}

impl IndexingServiceBuilder<WithEmbedder> {
    pub fn tokenizer(self, tokenizer: Arc<Tokenizer>) -> IndexingServiceBuilder<Ready> {
        IndexingServiceBuilder {
            pool: self.pool,
            vault_path: self.vault_path,
            embedder: self.embedder,
            tokenizer: Some(tokenizer),
            progress_tracker: self.progress_tracker,
            channel_size: self.channel_size,
            _state: PhantomData,
        }
    }
}

impl<State> IndexingServiceBuilder<State> {
    pub fn channel_size(mut self, size: usize) -> Self {
        self.channel_size = size;
        self
    }

    pub fn progress_tracker(mut self, tracker: Arc<Mutex<ProgressTracker>>) -> Self {
        self.progress_tracker = Some(tracker);
        self
    }
}

impl IndexingServiceBuilder<Ready> {
    pub fn build(
        self,
    ) -> Result<(
        mpsc::Sender<crate::infrastructure::indexing::queue::IndexTask>,
        IndexingActor,
    )> {
        let pool = self.pool.ok_or_else(|| {
            AppError::InvalidConfig("Database pool not set in IndexingServiceBuilder".to_string())
        })?;
        let vault_path = self.vault_path.ok_or_else(|| {
            AppError::InvalidConfig("Lattice path not set in IndexingServiceBuilder".to_string())
        })?;
        let embedder = self.embedder.ok_or_else(|| {
            AppError::InvalidConfig(
                "Embedding service not set in IndexingServiceBuilder".to_string(),
            )
        })?;
        let tokenizer = self.tokenizer.ok_or_else(|| {
            AppError::InvalidConfig("Tokenizer not set in IndexingServiceBuilder".to_string())
        })?;

        let progress_tracker = self
            .progress_tracker
            .unwrap_or_else(|| Arc::new(Mutex::new(ProgressTracker::new(100))));
        let pause_gate = Arc::new(PauseGate::new());

        let (tx, rx) = mpsc::channel(self.channel_size);

        let actor = IndexingActor::new(
            rx,
            pool,
            vault_path,
            embedder,
            tokenizer,
            progress_tracker,
            pause_gate,
        );

        Ok((tx, actor))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_builder_type_safety() {
        let builder = IndexingServiceBuilder::new();
    }
}
