//! Embedding what is new: pending documents' passages in small batches
//! through the model, saves of the vectors file that mark documents
//! embedded, and progress told to the owner as it goes.
//!
//! A document is marked embedded only after a save that holds all of its
//! passages, so a crash costs at most the work since the last save and a
//! resumed build picks up from the store.

use super::store::{Passage, PassageStore};
use super::vectors::Vectors;
use lattice_core::application::ports::EmbeddingPort;
use lattice_core::shared::{AppError, Result};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio_util::sync::CancellationToken;

/// The text a passage is embedded as, from its stored row. A folder embeds
/// what it stored (the line-referenced passage); a course can put the
/// source's title in front, which needs the owner's own lookup.
pub type EmbedText = Arc<dyn Fn(&Passage) -> String + Send + Sync>;

/// Passages per `embed_batch` call: small, so a chat turn's query embedding
/// waits behind at most one batch. Explorer's folder builds have always
/// embedded in batches this size; a course's index embeds the same way.
pub const BATCH_SIZE: usize = 16;

/// Passages embedded between saves of the vectors file, which is what a
/// crash can cost a build. Explorer's cadence, shared with a course's index.
pub const SAVE_EVERY: usize = 500;

/// How an index embeds: the model, and the owner's batch and save sizes.
#[derive(Clone)]
pub struct Embedding {
    pub embedder: Arc<dyn EmbeddingPort>,
    /// Passages per `embed_batch` call.
    pub batch_size: usize,
    /// Passages embedded between saves of the vectors file.
    pub save_every: usize,
    pub embed_text: EmbedText,
}

/// Embedding's progress, told to the owner as it moves. `embedded` comes
/// after every batch; `saved` after every save that marked documents.
pub trait Progress {
    /// Embedding is about to start on `documents` pending documents. Not
    /// called when nothing is pending.
    fn started(&mut self, documents: usize) {
        let _ = documents;
    }
    /// `count` more passages are in the in-memory vectors.
    fn embedded(&mut self, count: usize);
    /// A save marked `documents` more documents embedded.
    fn saved(&mut self, documents: usize);
}

/// What one round of embedding did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Embedded {
    pub passages: usize,
    pub elapsed: Duration,
    /// Stopped by `cancel` before every pending document was embedded.
    pub cancelled: bool,
}

/// The text one passage is embedded as: the owner's text, or its head when
/// that is longer than the model's window.
fn embeddable(embedding: &Embedding, passage: &Passage) -> Result<String> {
    let text = (embedding.embed_text)(passage);
    let parts = embedding.embedder.window_parts(&text)?;
    Ok(parts.into_iter().next().map_or(text, |part| part.text))
}

/// Embeds the passages of every document under `prefix` not yet marked
/// embedded, `batch_size` at a time, saving the vectors file every
/// `save_every` passages and once at the end.
pub async fn embed_pending(
    store: &PassageStore,
    vectors: &Vectors,
    embedding: &Embedding,
    prefix: &str,
    cancel: &CancellationToken,
    progress: &mut (dyn Progress + Send),
) -> Result<Embedded> {
    let started = Instant::now();
    let pending = store.pending(prefix).await?;
    let mut embedded = Embedded {
        passages: 0,
        elapsed: Duration::ZERO,
        cancelled: false,
    };
    if pending.is_empty() {
        save(store, vectors, &mut Vec::new()).await?;
        embedded.elapsed = started.elapsed();
        return Ok(embedded);
    }
    progress.started(pending.len());

    let mut round = Round {
        embedding,
        vectors,
        batch: Vec::new(),
        waiting: HashMap::new(),
        finished: Vec::new(),
    };
    let mut since_save = 0usize;
    'documents: for key in pending {
        if cancel.is_cancelled() {
            embedded.cancelled = true;
            break;
        }
        let passages = store.passages_of(&key).await?;
        if passages.is_empty() {
            round.finished.push(key);
            continue;
        }
        round.waiting.insert(key, passages.len());
        for passage in passages {
            round.batch.push(passage);
            if round.batch.len() < embedding.batch_size.max(1) {
                continue;
            }
            let count = round.embed_batch().await?;
            embedded.passages += count;
            since_save += count;
            progress.embedded(count);
            if since_save >= embedding.save_every {
                since_save = 0;
                let marked = save(store, vectors, &mut round.finished).await?;
                if marked > 0 {
                    progress.saved(marked);
                }
            }
            if cancel.is_cancelled() {
                embedded.cancelled = true;
                break 'documents;
            }
        }
    }
    if !embedded.cancelled {
        let count = round.embed_batch().await?;
        embedded.passages += count;
        if count > 0 {
            progress.embedded(count);
        }
    }
    let marked = save(store, vectors, &mut round.finished).await?;
    if marked > 0 {
        progress.saved(marked);
    }
    embedded.elapsed = started.elapsed();
    Ok(embedded)
}

/// Writes the vectors file, then marks `finished` documents embedded,
/// emptying it. Returns how many it marked.
pub async fn save(
    store: &PassageStore,
    vectors: &Vectors,
    finished: &mut Vec<String>,
) -> Result<usize> {
    vectors.save().await?;
    if finished.is_empty() {
        return Ok(0);
    }
    let marked = std::mem::take(finished);
    store.mark_embedded(&marked).await?;
    Ok(marked.len())
}

/// The batch being filled, and which documents wait on how many passages.
struct Round<'a> {
    embedding: &'a Embedding,
    vectors: &'a Vectors,
    batch: Vec<Passage>,
    waiting: HashMap<String, usize>,
    /// Documents whose last passage is in the in-memory vectors, to be
    /// marked at the next save.
    finished: Vec<String>,
}

impl Round<'_> {
    /// Embeds and publishes the batch, emptying it. Documents whose last
    /// passage was in it move to `finished`. Returns how many passages it
    /// embedded.
    async fn embed_batch(&mut self) -> Result<usize> {
        if self.batch.is_empty() {
            return Ok(0);
        }
        let passages = std::mem::take(&mut self.batch);
        let texts = passages
            .iter()
            .map(|passage| embeddable(self.embedding, passage))
            .collect::<Result<Vec<_>>>()?;
        let embeddings = self.embedding.embedder.embed_batch(&texts).await?;
        if embeddings.len() != passages.len() {
            return Err(AppError::EmbeddingFailed {
                reason: format!(
                    "Passage index: {} embeddings came back for {} passages",
                    embeddings.len(),
                    passages.len()
                ),
            });
        }
        let count = passages.len();
        let mut done = Vec::new();
        let entries = passages
            .into_iter()
            .zip(embeddings)
            .map(|(passage, embedding)| {
                if let Some(left) = self.waiting.get_mut(&passage.document) {
                    *left = left.saturating_sub(1);
                    if *left == 0 {
                        done.push(passage.document.clone());
                    }
                }
                (passage.id, passage.document, embedding)
            })
            .collect();
        self.vectors.add(entries)?;
        for key in done {
            self.waiting.remove(&key);
            self.finished.push(key);
        }
        // A query embedding from a chat turn gets its turn between batches.
        tokio::task::yield_now().await;
        Ok(count)
    }
}
