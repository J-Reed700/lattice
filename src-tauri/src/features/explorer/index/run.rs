//! One folder's indexing work: the full run when a folder opens, and the
//! per-path updates the watcher asks for afterwards.
//!
//! Everything that writes goes through one [`Indexer`] on one task, so a
//! watcher update never races the run it follows. The run is incremental:
//! a file whose size and modification time match its row is not read, one
//! whose content hash matches is not re-chunked, and one whose chunks are all
//! in the saved vectors file is not embedded again.

use super::chunker::{self, Chunk};
use super::dto::{FolderIndexState, FolderIndexStatusDto};
use super::store::{ChunkRow, FileRow, FolderStore};
use crate::application::ports::vector_search_port::{VectorIndexEntry, VectorSearchPort};
use crate::application::ports::EmbeddingPort;
use crate::features::explorer::fs;
use crate::features::explorer::scope::Scope;
use crate::features::search::engine::vector_search::USearchVectorIndex;
use crate::shared::{AppError, Result};
use parking_lot::Mutex;
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant, UNIX_EPOCH};
use tokio_util::sync::CancellationToken;

/// Status events go out at most this often while counts move; a change of
/// state always goes out at once.
const STATUS_INTERVAL: Duration = Duration::from_millis(250);
/// Files read and chunked per trip to the blocking pool.
const PREPARE_GROUP: usize = 64;
/// How far past the cap a too-large walk keeps counting, so the message can
/// say how large rather than only "too large".
const COUNT_CEILING_FACTOR: usize = 10;

pub type StatusEmitter = Arc<dyn Fn(&FolderIndexStatusDto) + Send + Sync>;

/// The open folder's status, and the throttle on telling the UI about it.
pub struct StatusCell {
    status: Mutex<FolderIndexStatusDto>,
    last_emit: Mutex<Option<Instant>>,
    emit: StatusEmitter,
}

impl StatusCell {
    pub fn new(initial: FolderIndexStatusDto, emit: StatusEmitter) -> Self {
        Self {
            status: Mutex::new(initial),
            last_emit: Mutex::new(None),
            emit,
        }
    }

    pub fn get(&self) -> FolderIndexStatusDto {
        self.status.lock().clone()
    }

    /// Applies `change` and tells the UI: at once when the state moved,
    /// otherwise only if the last event is old enough.
    pub fn update(&self, change: impl FnOnce(&mut FolderIndexStatusDto)) {
        let (snapshot, state_moved) = {
            let mut status = self.status.lock();
            let before = status.state;
            change(&mut status);
            (status.clone(), status.state != before)
        };
        let mut last = self.last_emit.lock();
        let due = last.is_none_or(|at| at.elapsed() >= STATUS_INTERVAL);
        if state_moved || due {
            *last = Some(Instant::now());
            drop(last);
            (self.emit)(&snapshot);
        }
    }

    /// Sends the current status regardless of the throttle.
    pub fn announce(&self) {
        *self.last_emit.lock() = Some(Instant::now());
        (self.emit)(&self.get());
    }
}

/// Bounds on one folder's work.
#[derive(Debug, Clone, Copy)]
pub struct Limits {
    /// More indexable files than this and nothing is embedded.
    pub max_files: usize,
    /// Chunks per `embed_batch` call. Small, so a chat turn's query embedding
    /// waits behind at most one batch.
    pub batch_size: usize,
    /// Chunks embedded between saves of the vectors file.
    pub save_every: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_files: 20_000,
            batch_size: 16,
            save_every: 500,
        }
    }
}

/// How a run or an update ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Done,
    Cancelled,
    /// The walk found this many indexable files (or at least this many, when
    /// `capped`).
    TooLarge {
        count: usize,
        capped: bool,
    },
}

/// A file the walk found, before it is read.
#[derive(Debug, Clone)]
struct Candidate {
    relative: String,
    absolute: PathBuf,
    size: i64,
    mtime: i64,
}

enum Walked {
    Files(Vec<Candidate>),
    TooLarge { count: usize, capped: bool },
    Cancelled,
}

/// What reading one changed file decided.
enum Prepared {
    /// Same bytes as before; only the size or time moved.
    Touch,
    Replace {
        hash: String,
        chunks: Vec<Chunk>,
    },
    /// Gone, unreadable, or over the size cap since the walk.
    Gone,
}

fn mtime_of(metadata: &std::fs::Metadata) -> i64 {
    metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |since| {
            i64::try_from(since.as_nanos()).unwrap_or(i64::MAX)
        })
}

/// Whether the walk should consider a file at all: decided by name and size
/// alone, so counting a too-large folder reads nothing.
fn candidate(scope: &Scope, path: &Path) -> Option<Candidate> {
    let relative = scope.relative_of(path)?;
    if chunker::skipped_by_name(&relative) {
        return None;
    }
    let metadata = std::fs::metadata(path).ok()?;
    if !metadata.is_file() || metadata.len() > chunker::MAX_FILE_BYTES {
        return None;
    }
    Some(Candidate {
        relative,
        absolute: path.to_path_buf(),
        size: i64::try_from(metadata.len()).unwrap_or(i64::MAX),
        mtime: mtime_of(&metadata),
    })
}

/// Every indexable file under `prefix`, the way `search_files` walks:
/// `.gitignore` honoured, `.git` skipped, symlinks not followed. Stops early
/// when `cancel` fires, so closing a folder does not wait out a large walk.
fn walk(
    scope: &Scope,
    prefix: &str,
    max_files: usize,
    cancel: &CancellationToken,
) -> Result<Walked> {
    let start = fs::walk_start(scope, Some(prefix))?;
    let ceiling = max_files.saturating_mul(COUNT_CEILING_FACTOR);
    let mut files = Vec::new();
    let mut count = 0usize;
    let mut capped = false;
    let mut cancelled = false;
    fs::walk_files(scope, start.as_ref(), |path| {
        if cancel.is_cancelled() {
            cancelled = true;
            return false;
        }
        let Some(found) = candidate(scope, path) else {
            return true;
        };
        count += 1;
        if count > ceiling {
            capped = true;
            return false;
        }
        if count <= max_files {
            files.push(found);
        }
        true
    });
    Ok(if cancelled {
        Walked::Cancelled
    } else if count > max_files {
        Walked::TooLarge {
            count: count.min(ceiling),
            capped,
        }
    } else {
        Walked::Files(files)
    })
}

fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

/// Reads one changed file and decides what its rows become. Binary and
/// minified files are recorded with no chunks, so the next run skips them on
/// size and time without reading them again.
fn prepare(path: &Path, previous_hash: Option<&str>) -> Prepared {
    let Ok((bytes, cut)) = fs::read_prefix(path, chunker::MAX_FILE_BYTES) else {
        return Prepared::Gone;
    };
    if cut {
        return Prepared::Gone;
    }
    let hash = sha256_hex(&bytes);
    if previous_hash == Some(hash.as_str()) {
        return Prepared::Touch;
    }
    if fs::looks_binary(&bytes) {
        return Prepared::Replace {
            hash,
            chunks: Vec::new(),
        };
    }
    let text = String::from_utf8_lossy(&bytes);
    let chunks = if chunker::looks_minified(&text) {
        Vec::new()
    } else {
        chunker::chunk_text(&text)
    };
    Prepared::Replace { hash, chunks }
}

fn vector_ids(ids: &[i64]) -> Vec<String> {
    ids.iter().map(i64::to_string).collect()
}

/// The text one chunk is embedded as: the passage, or its head when the
/// passage is longer than the model's window.
fn embeddable(embedder: &dyn EmbeddingPort, chunk: &ChunkRow) -> Result<String> {
    let parts = embedder.window_parts(&chunk.text)?;
    Ok(parts
        .into_iter()
        .next()
        .map_or_else(|| chunk.text.clone(), |part| part.text))
}

pub struct Indexer {
    pub store: Arc<FolderStore>,
    pub vectors: Arc<USearchVectorIndex>,
    pub embedder: Arc<dyn EmbeddingPort>,
    /// The index root, which the walk starts from so its ignore rules apply.
    pub scope: Scope,
    /// The part of the index this folder covers: `""`, or the picked
    /// sub-folder when a parent's index is reused.
    pub prefix: String,
    pub status: Arc<StatusCell>,
    pub cancel: CancellationToken,
    pub limits: Limits,
}

impl Indexer {
    /// Walks the folder, brings the rows in line with it, and embeds what is
    /// new.
    pub async fn run(&self) -> Result<Outcome> {
        self.status
            .update(|status| status.state = FolderIndexState::Scanning);
        let scope = self.scope.clone();
        let prefix = self.prefix.clone();
        let max_files = self.limits.max_files;
        let cancel = self.cancel.clone();
        let walked = tokio::task::spawn_blocking(move || walk(&scope, &prefix, max_files, &cancel))
            .await
            .map_err(|error| AppError::InternalError(format!("Folder walk failed: {error}")))??;
        let candidates = match walked {
            Walked::TooLarge { count, capped } => {
                return Ok(Outcome::TooLarge { count, capped });
            }
            Walked::Cancelled => return Ok(Outcome::Cancelled),
            Walked::Files(files) => files,
        };
        let existing = self.store.files_under(&self.prefix).await?;
        let total = u32::try_from(candidates.len()).unwrap_or(u32::MAX);
        self.status.update(|status| status.files_total = total);

        let seen: HashSet<String> = candidates
            .iter()
            .map(|found| found.relative.clone())
            .collect();
        if self.sync_candidates(candidates, &existing).await? == Outcome::Cancelled {
            return Ok(Outcome::Cancelled);
        }
        let gone: Vec<String> = existing
            .into_keys()
            .filter(|path| !seen.contains(path))
            .collect();
        self.forget(&gone).await?;
        self.embed_pending().await
    }

    /// Brings the rows for `paths` (relative to the index root, as the
    /// watcher reported them) in line with the disk, then embeds what moved.
    pub async fn sync_paths(&self, paths: Vec<String>) -> Result<Outcome> {
        for relative in paths {
            if self.cancel.is_cancelled() {
                return Ok(Outcome::Cancelled);
            }
            let absolute = relative
                .split('/')
                .fold(self.scope.root().to_path_buf(), |path, part| {
                    path.join(part)
                });
            let scope = self.scope.clone();
            let probe = absolute.clone();
            // An ignored path is dropped, and so is anything the walk would
            // never have reached: `.git`, a symlink, a special file.
            let (kind, ignored) = tokio::task::spawn_blocking(move || {
                let kind = std::fs::symlink_metadata(&probe)
                    .ok()
                    .map(|m| m.file_type());
                let ignored = probe.ancestors().any(fs::is_git_dir)
                    || (kind.is_some() && fs::is_ignored(&scope, &probe));
                (kind, ignored)
            })
            .await
            .map_err(|error| AppError::InternalError(format!("Folder update failed: {error}")))?;
            match kind {
                _ if ignored => {
                    // It may have been indexed before the rule that hides it.
                    let removed = self.store.remove_under(&relative).await?;
                    self.drop_vectors(&removed)?;
                }
                Some(kind) if kind.is_dir() => {
                    if self.sync_tree(&relative).await? == Outcome::Cancelled {
                        return Ok(Outcome::Cancelled);
                    }
                }
                Some(kind) if kind.is_file() => {
                    let scope = self.scope.clone();
                    let found = tokio::task::spawn_blocking(move || candidate(&scope, &absolute))
                        .await
                        .map_err(|error| {
                            AppError::InternalError(format!("Folder update failed: {error}"))
                        })?;
                    match found {
                        Some(found) => {
                            let existing = self.store.files_under(&relative).await?;
                            self.sync_candidates(vec![found], &existing).await?;
                        }
                        None => self.forget(std::slice::from_ref(&relative)).await?,
                    }
                }
                _ => {
                    let removed = self.store.remove_under(&relative).await?;
                    self.drop_vectors(&removed)?;
                }
            }
        }
        self.refresh_counts().await?;
        self.embed_pending().await
    }

    /// A directory that appeared or changed: walk just it.
    async fn sync_tree(&self, relative: &str) -> Result<Outcome> {
        let scope = self.scope.clone();
        let start = relative.to_string();
        let max_files = self.limits.max_files;
        let cancel = self.cancel.clone();
        let walked = tokio::task::spawn_blocking(move || walk(&scope, &start, max_files, &cancel))
            .await
            .map_err(|error| AppError::InternalError(format!("Folder walk failed: {error}")))??;
        let candidates = match walked {
            Walked::Files(files) => files,
            Walked::Cancelled => return Ok(Outcome::Cancelled),
            // A single new directory over the whole cap is left to the next
            // full run to judge.
            Walked::TooLarge { .. } => return Ok(Outcome::Done),
        };
        let existing = self.store.files_under(relative).await?;
        let seen: HashSet<String> = candidates
            .iter()
            .map(|found| found.relative.clone())
            .collect();
        if self.sync_candidates(candidates, &existing).await? == Outcome::Cancelled {
            return Ok(Outcome::Cancelled);
        }
        let gone: Vec<String> = existing
            .into_keys()
            .filter(|path| !seen.contains(path))
            .collect();
        self.forget(&gone).await?;
        Ok(Outcome::Done)
    }

    /// Reads and re-chunks the candidates whose size or time moved.
    async fn sync_candidates(
        &self,
        candidates: Vec<Candidate>,
        existing: &HashMap<String, FileRow>,
    ) -> Result<Outcome> {
        let changed: Vec<(Candidate, Option<String>)> = candidates
            .into_iter()
            .filter_map(|found| match existing.get(&found.relative) {
                Some(row) if row.size == found.size && row.mtime == found.mtime => None,
                Some(row) => Some((found, Some(row.content_hash.clone()))),
                None => Some((found, None)),
            })
            .collect();
        let mut groups = changed.into_iter().peekable();
        while groups.peek().is_some() {
            if self.cancel.is_cancelled() {
                return Ok(Outcome::Cancelled);
            }
            let group: Vec<(Candidate, Option<String>)> =
                groups.by_ref().take(PREPARE_GROUP).collect();
            let prepared = tokio::task::spawn_blocking(move || {
                group
                    .into_iter()
                    .map(|(found, hash)| {
                        let decision = prepare(&found.absolute, hash.as_deref());
                        (found, decision)
                    })
                    .collect::<Vec<_>>()
            })
            .await
            .map_err(|error| AppError::InternalError(format!("Folder read failed: {error}")))?;
            for (found, decision) in prepared {
                match decision {
                    Prepared::Touch => {
                        self.store
                            .touch_file(&found.relative, found.size, found.mtime)
                            .await?
                    }
                    Prepared::Replace { hash, chunks } => {
                        let replaced = self
                            .store
                            .replace_file(&found.relative, found.size, found.mtime, &hash, &chunks)
                            .await?;
                        self.drop_vectors(&replaced)?;
                    }
                    Prepared::Gone => self.forget(std::slice::from_ref(&found.relative)).await?,
                }
            }
        }
        Ok(Outcome::Done)
    }

    async fn forget(&self, paths: &[String]) -> Result<()> {
        if paths.is_empty() {
            return Ok(());
        }
        let removed = self.store.remove_files(paths).await?;
        self.drop_vectors(&removed)
    }

    /// Takes vectors out of the in-memory index. They reach the file with the
    /// next save; until then a stale vector on disk is harmless, because a
    /// search keeps only hits whose chunk row still exists.
    fn drop_vectors(&self, ids: &[i64]) -> Result<()> {
        if ids.is_empty() {
            return Ok(());
        }
        self.vectors.remove_embeddings(&vector_ids(ids))
    }

    async fn refresh_counts(&self) -> Result<()> {
        let counts = self.store.counts(&self.prefix).await?;
        self.status.update(|status| {
            status.files_total = counts.files_total;
            status.files_indexed = counts.files_indexed;
            status.chunks = counts.chunks;
        });
        Ok(())
    }

    /// Embeds the chunks of every file not yet marked embedded, in small
    /// batches, saving the vectors file every `save_every` chunks. A file is
    /// marked only after a save that holds all of its chunks, so a crash
    /// costs at most the work since the last save.
    async fn embed_pending(&self) -> Result<Outcome> {
        let pending = self.store.pending_files(&self.prefix).await?;
        self.refresh_counts().await?;
        if pending.is_empty() {
            self.save(&mut Vec::new()).await?;
            return Ok(Outcome::Done);
        }
        self.status
            .update(|status| status.state = FolderIndexState::Indexing);

        let mut batch: Vec<ChunkRow> = Vec::new();
        let mut waiting: HashMap<String, usize> = HashMap::new();
        let mut finished: Vec<String> = Vec::new();
        let mut since_save = 0usize;
        for path in pending {
            if self.cancel.is_cancelled() {
                self.save(&mut finished).await?;
                return Ok(Outcome::Cancelled);
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
                since_save += self
                    .embed_batch(&mut batch, &mut waiting, &mut finished)
                    .await?;
                if since_save >= self.limits.save_every {
                    since_save = 0;
                    self.save(&mut finished).await?;
                }
                if self.cancel.is_cancelled() {
                    self.save(&mut finished).await?;
                    return Ok(Outcome::Cancelled);
                }
            }
        }
        self.embed_batch(&mut batch, &mut waiting, &mut finished)
            .await?;
        self.save(&mut finished).await?;
        Ok(Outcome::Done)
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
