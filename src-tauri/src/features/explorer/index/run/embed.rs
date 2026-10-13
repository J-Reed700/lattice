//! Embedding what is new: batches through the model, progress with a rate and
//! time left, and saves of the vectors file that mark files embedded.

use super::{per_second, Embedded, Indexer, Outcome, Pace};
use crate::application::ports::vector_search_port::{VectorIndexEntry, VectorSearchPort};
use crate::application::ports::EmbeddingPort;
use crate::features::explorer::index::dto::FolderIndexState;
use crate::features::explorer::index::store::ChunkRow;
use crate::shared::{AppError, Result};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tracing::info;

/// The text one chunk is embedded as: the passage, or its head when the
/// passage is longer than the model's window.
fn embeddable(embedder: &dyn EmbeddingPort, chunk: &ChunkRow) -> Result<String> {
    let parts = embedder.window_parts(&chunk.text)?;
    Ok(parts
        .into_iter()
        .next()
        .map_or_else(|| chunk.text.clone(), |part| part.text))
}

impl Indexer {
    /// `count` more passages are in the index: the status moves, with a rate
    /// and time left once `pace` has enough behind it, and passing another
    /// tenth of the way gets a log line.
    fn advance(&self, count: usize, pace: &mut Pace, logged_tenths: &mut u32) {
        if count == 0 {
            return;
        }
        let now = Instant::now();
        pace.record(count, now);
        let added = u32::try_from(count).unwrap_or(u32::MAX);
        let rate = pace.rate(now);
        self.status.update(|status| {
            status.passages_embedded = status
                .passages_embedded
                .saturating_add(added)
                .min(status.passages_total);
            status.passages_per_second = rate.map(|rate| rate as f32);
            status.eta_seconds = pace.eta(
                status
                    .passages_total
                    .saturating_sub(status.passages_embedded),
                now,
            );
        });
        let status = self.status.get();
        let tenths = status.percent() / 10;
        if tenths > *logged_tenths && tenths < 10 {
            *logged_tenths = tenths;
            info!(
                root = %self.folder().display(),
                percent = status.percent(),
                embedded = status.passages_embedded,
                total = status.passages_total,
                per_second = %per_second(rate),
                eta_seconds = ?status.eta_seconds,
                "Folder index progress"
            );
        }
    }

    /// Embeds the chunks of every file not yet marked embedded, in small
    /// batches, saving the vectors file every `save_every` chunks. A file is
    /// marked only after a save that holds all of its chunks, so a crash
    /// costs at most the work since the last save.
    pub(super) async fn embed_pending(&self) -> Result<Embedded> {
        let started = Instant::now();
        let pending = self.store.pending_files(&self.prefix).await?;
        self.refresh_counts().await?;
        let mut embedded = Embedded {
            outcome: Outcome::Done,
            passages: 0,
            elapsed: Duration::ZERO,
        };
        if pending.is_empty() {
            self.save(&mut Vec::new()).await?;
            return Ok(embedded);
        }
        self.status.update(|status| {
            status.state = FolderIndexState::Indexing;
            status.passages_per_second = None;
            status.eta_seconds = None;
        });

        let mut pace = Pace::starting_at(started);
        let mut logged_tenths = self.status.get().percent() / 10;
        let mut batch: Vec<ChunkRow> = Vec::new();
        let mut waiting: HashMap<String, usize> = HashMap::new();
        let mut finished: Vec<String> = Vec::new();
        let mut since_save = 0usize;
        'files: for path in pending {
            if self.cancel.is_cancelled() {
                embedded.outcome = Outcome::Cancelled;
                break;
            }
            let chunks = self.store.chunks_of(&path).await?;
            if chunks.is_empty() {
                finished.push(path);
                continue;
            }
            waiting.insert(path, chunks.len());
            for chunk in chunks {
                batch.push(chunk);
                if batch.len() < self.limits.batch_size {
                    continue;
                }
                let count = self
                    .embed_batch(&mut batch, &mut waiting, &mut finished)
                    .await?;
                embedded.passages += count;
                since_save += count;
                self.advance(count, &mut pace, &mut logged_tenths);
                if since_save >= self.limits.save_every {
                    since_save = 0;
                    self.save(&mut finished).await?;
                }
                if self.cancel.is_cancelled() {
                    embedded.outcome = Outcome::Cancelled;
                    break 'files;
                }
            }
        }
        if embedded.outcome == Outcome::Done {
            let count = self
                .embed_batch(&mut batch, &mut waiting, &mut finished)
                .await?;
            embedded.passages += count;
            self.advance(count, &mut pace, &mut logged_tenths);
        }
        self.save(&mut finished).await?;
        embedded.elapsed = started.elapsed();
        if embedded.outcome == Outcome::Done {
            self.refresh_counts().await?;
        }
        Ok(embedded)
    }

    /// Embeds and publishes `batch`, emptying it. Files whose last chunk was
    /// in it move to `finished`. Returns how many chunks it embedded.
    async fn embed_batch(
        &self,
        batch: &mut Vec<ChunkRow>,
        waiting: &mut HashMap<String, usize>,
        finished: &mut Vec<String>,
    ) -> Result<usize> {
        if batch.is_empty() {
            return Ok(0);
        }
        let chunks = std::mem::take(batch);
        let texts = chunks
            .iter()
            .map(|chunk| embeddable(self.embedder.as_ref(), chunk))
            .collect::<Result<Vec<_>>>()?;
        let embeddings = self.embedder.embed_batch(&texts).await?;
        if embeddings.len() != chunks.len() {
            return Err(AppError::EmbeddingFailed {
                reason: format!(
                    "Folder index: {} embeddings came back for {} passages",
                    embeddings.len(),
                    chunks.len()
                ),
            });
        }
        let count = chunks.len();
        let mut done_paths = Vec::new();
        let entries = chunks
            .into_iter()
            .zip(embeddings)
            .map(|(chunk, embedding)| {
                if let Some(left) = waiting.get_mut(&chunk.path) {
                    *left = left.saturating_sub(1);
                    if *left == 0 {
                        done_paths.push(chunk.path.clone());
                    }
                }
                VectorIndexEntry {
                    id: chunk.id.to_string(),
                    embedding,
                    content: String::new(),
                    chunk_id: chunk.id.to_string(),
                    // The path is the vector's document, so a search confined
                    // to a sub-folder is scoped inside USearch, not after it.
                    document_id: chunk.path,
                }
            })
            .collect();
        self.vectors.publish_embeddings(entries)?;
        for path in done_paths {
            waiting.remove(&path);
            finished.push(path);
        }
        // A query embedding from a chat turn gets its turn between batches.
        tokio::task::yield_now().await;
        Ok(count)
    }

    /// Writes the vectors file, then marks `finished` files embedded.
    async fn save(&self, finished: &mut Vec<String>) -> Result<()> {
        let vectors = Arc::clone(&self.vectors);
        tokio::task::spawn_blocking(move || {
            if vectors.is_dirty() {
                vectors.save_to_disk()
            } else {
                Ok(())
            }
        })
        .await
        .map_err(|error| {
            AppError::InternalError(format!("Saving folder vectors failed: {error}"))
        })??;
        if finished.is_empty() {
            return Ok(());
        }
        let marked = std::mem::take(finished);
        self.store.mark_embedded(&marked).await?;
        let added = u32::try_from(marked.len()).unwrap_or(u32::MAX);
        self.status.update(|status| {
            status.files_indexed = status
                .files_indexed
                .saturating_add(added)
                .min(status.files_total)
        });
        Ok(())
    }
}
