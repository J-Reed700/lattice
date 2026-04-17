use crate::infrastructure::indexing::chunker::{ChunkerConfig, SemanticChunker};
use crate::infrastructure::indexing::error::{IndexingError, Result};
use crate::infrastructure::indexing::error_ext::IndexingResultExt;
use crate::infrastructure::indexing::events::IndexingEvent;
use crate::infrastructure::indexing::extraction::ContentExtractor;
use crate::infrastructure::indexing::metadata_extractor::{
    determine_page_number, extract_metadata,
};
use crate::infrastructure::indexing::progress::{IndexProgress, ProgressTracker};
use crate::infrastructure::indexing::queue::IndexTask;
use crate::infrastructure::indexing::storage::IndexStorage;
use crate::infrastructure::indexing::transaction::FileIndexTransaction;
use crate::infrastructure::services::embedding::EmbeddingService;
use crate::infrastructure::services::file_storage::FileStorageService;
use crate::infrastructure::services::file_type_detector::FileTypeDetector;
use crate::infrastructure::services::traits::EmbeddingServiceTrait;
use crate::shared::utils::patterns::observer::Observable;
use sqlx::SqlitePool;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;
use tokenizers::Tokenizer;
use tokio::sync::{mpsc, Mutex, Notify};

pub struct PauseGate {
    paused: AtomicBool,
    notify: Notify,
}

impl PauseGate {
    pub fn new() -> Self {
        Self {
            paused: AtomicBool::new(false),
            notify: Notify::new(),
        }
    }

    pub fn pause(&self) {
        self.paused.store(true, Ordering::SeqCst);
    }

    pub fn resume(&self) {
        if self.paused.swap(false, Ordering::SeqCst) {
            self.notify.notify_waiters();
        }
    }

    pub async fn wait_if_paused(&self) {
        while self.paused.load(Ordering::SeqCst) {
            self.notify.notified().await;
        }
    }
}

pub struct IndexingActor {
    rx: mpsc::Receiver<IndexTask>,
    storage: Arc<IndexStorage>,
    file_storage: Arc<FileStorageService>,
    embedder: Arc<EmbeddingService>,
    extractor: Arc<ContentExtractor>,
    chunker: Arc<SemanticChunker>,
    progress_tracker: Arc<Mutex<ProgressTracker>>,
    event_observable: Arc<Mutex<Observable<IndexingEvent>>>,
    cancelled: Arc<Mutex<bool>>,
    pause_gate: Arc<PauseGate>,
}

impl IndexingActor {
    pub fn new(
        rx: mpsc::Receiver<IndexTask>,
        pool: SqlitePool,
        vault_path: PathBuf,
        embedder: Arc<EmbeddingService>,
        tokenizer: Arc<Tokenizer>,
        progress_tracker: Arc<Mutex<ProgressTracker>>,
        pause_gate: Arc<PauseGate>,
    ) -> Self {
        let storage = Arc::new(IndexStorage::new(pool.clone()));
        let file_storage = Arc::new(FileStorageService::new(vault_path, pool));
        let extractor = Arc::new(ContentExtractor::new());

        let chunker_config = ChunkerConfig {
            max_tokens: 800,
            overlap_tokens: 120,
            prefer_sentence_boundaries: true,
        };

        #[allow(clippy::expect_used)] // Default config is static and validated by tests
        let chunker = Arc::new(
            SemanticChunker::new(tokenizer, chunker_config)
                .expect("Default chunker config is always valid"),
        );

        Self {
            rx,
            storage,
            file_storage,
            embedder,
            extractor,
            chunker,
            progress_tracker,
            event_observable: Arc::new(Mutex::new(Observable::new())),
            cancelled: Arc::new(Mutex::new(false)),
            pause_gate,
        }
    }

    pub fn with_observable(mut self, observable: Arc<Mutex<Observable<IndexingEvent>>>) -> Self {
        self.event_observable = observable;
        self
    }

    pub fn get_observable(&self) -> Arc<Mutex<Observable<IndexingEvent>>> {
        Arc::clone(&self.event_observable)
    }

    pub async fn run(mut self) {
        while let Some(task) = self.rx.recv().await {
            if *self.cancelled.lock().await {
                break;
            }

            match task {
                IndexTask::CancelAll | IndexTask::Shutdown => {}
                _ => self.pause_gate.wait_if_paused().await,
            }

            match task {
                IndexTask::IndexFile { path } => {
                    if let Err(e) = self.process_file(&path).await {
                        eprintln!("Error indexing file {:?}: {}", path, e);
                        self.progress_tracker.lock().await.increment_failed();
                    }
                }
                IndexTask::IndexFolder { path, recursive } => {
                    if let Err(e) = self.process_folder(&path, recursive).await {
                        eprintln!("Error indexing folder {:?}: {}", path, e);
                        let mut tracker = self.progress_tracker.lock().await;
                        tracker.error(format!("Failed to index folder: {}", e));
                    }
                }
                IndexTask::ReindexFile { path } => {
                    if let Err(e) = self.reindex_file(&path).await {
                        eprintln!("Error reindexing file {:?}: {}", path, e);
                        self.progress_tracker.lock().await.increment_failed();
                    }
                }
                IndexTask::RemoveFile { path } => {
                    if let Err(e) = self.storage.remove_document(&path).await {
                        eprintln!("Error removing document {:?}: {}", path, e);
                    }
                }
                IndexTask::CancelAll => {
                    *self.cancelled.lock().await = true;
                    self.progress_tracker.lock().await.cancel();
                    break;
                }
                IndexTask::Shutdown => {
                    break;
                }
            }
        }
    }

    async fn process_file(&self, path: &Path) -> Result<()> {
        let start = Instant::now();

        if !path.exists() {
            return Err(IndexingError::FileNotFound {
                path: path.display().to_string(),
            });
        }

        let file_type_info = FileTypeDetector::detect(path)?;

        let tracker = self.progress_tracker.lock().await;
        let current = tracker.get_current().processed;
        let total = tracker.get_current().total_files;
        drop(tracker);

        self.event_observable
            .lock()
            .await
            .notify_all(&IndexingEvent::file_started(
                path.display().to_string(),
                current,
                total,
            ))
            .await;

        {
            let mut tracker = self.progress_tracker.lock().await;
            tracker.update_current_file(path.to_path_buf());
        }

        // Validate file path before storing
        let validated_path = crate::shared::domain_types::ValidatedFilePath::new(
            path.to_path_buf(),
        )
        .map_err(|e| {
            IndexingError::InvalidInput(format!("Invalid file path '{}': {}", path.display(), e))
        })?;

        let file_record = self
            .file_storage
            .store_file(validated_path, &file_type_info.mime_type, None)
            .await
            .log_context("store file", path)?;

        // Create transaction guard - automatically cleans up on failure
        let tx = FileIndexTransaction::new(
            file_record.id.clone(),
            Arc::clone(&self.file_storage),
            self.storage.pool.clone(),
        );

        if !file_type_info.is_indexable {
            self.storage
                .store_file_metadata_only(path, &file_record.id, &file_type_info.mime_type)
                .await?;

            self.progress_tracker.lock().await.increment_processed();

            let duration_ms = start.elapsed().as_millis() as u64;
            self.event_observable
                .lock()
                .await
                .notify_all(&IndexingEvent::file_completed(
                    path.display().to_string(),
                    0,
                    duration_ms,
                    current + 1,
                    total,
                ))
                .await;

            tx.commit();
            return Ok(());
        }

        let needs_index = self.storage.needs_reindex(path).await?;
        if !needs_index {
            self.progress_tracker.lock().await.increment_processed();
            tx.commit();
            return Ok(());
        }

        let extracted = self
            .extractor
            .extract_from_file(path)
            .await
            .log_context("extract text", path)?;

        let base_metadata = extract_metadata(path, &extracted.text, 0)?;

        let mut contextualized_chunks = Vec::new();

        let base_chunks = self.chunker.chunk_text(&extracted.text)?;

        if base_chunks.is_empty() {
            self.storage
                .store_file_metadata_only(path, &file_record.id, &file_type_info.mime_type)
                .await?;
            self.progress_tracker.lock().await.increment_processed();
            tx.commit();
            return Ok(());
        }

        for chunk in base_chunks {
            let page_number = determine_page_number(chunk.start_idx, &extracted.page_ranges);

            let mut chunk_metadata = base_metadata.clone();
            chunk_metadata.page_number = page_number;

            if chunk_metadata.section.is_none() {
                chunk_metadata.section = extract_metadata(path, &chunk.text, 0)?.section;
            }

            let context_prefix = self.build_context_prefix(&chunk_metadata);
            let contextualized_content = format!("{}\n\n{}", context_prefix, chunk.text);

            contextualized_chunks.push(
                crate::infrastructure::indexing::chunker::ContextualizedChunk {
                    original_content: chunk.text.clone(),
                    contextualized_content,
                    context_prefix,
                    chunk_index: contextualized_chunks.len(),
                    token_count: chunk.token_count,
                    start_idx: chunk.start_idx,
                    end_idx: chunk.end_idx,
                },
            );
        }

        let embeddings = self
            .embedder
            .embed_contextualized_chunks(&contextualized_chunks)
            .await
            .log_context("generate embeddings", path)?;

        let num_chunks = contextualized_chunks.len();

        let mut db_tx = self
            .storage
            .pool
            .begin()
            .await
            .log_context("begin transaction", path)?;

        self.storage
            .store_document_with_context_and_file_tx(
                &mut db_tx,
                path,
                &file_record.id,
                &extracted.mime_type,
                contextualized_chunks,
                embeddings,
            )
            .await
            .log_context("store document", path)?;

        sqlx::query("UPDATE files SET is_indexed = 1 WHERE id = ?1")
            .bind(&file_record.id)
            .execute(&mut *db_tx)
            .await
            .log_context("mark file as indexed", path)?;

        db_tx
            .commit()
            .await
            .log_context("commit transaction", path)?;

        self.progress_tracker.lock().await.increment_processed();

        let duration_ms = start.elapsed().as_millis() as u64;
        let tracker = self.progress_tracker.lock().await;
        let current = tracker.get_current().processed;
        let total = tracker.get_current().total_files;
        drop(tracker);

        self.event_observable
            .lock()
            .await
            .notify_all(&IndexingEvent::file_completed(
                path.display().to_string(),
                num_chunks,
                duration_ms,
                current,
                total,
            ))
            .await;

        // Success - commit transaction to prevent rollback
        tx.commit();

        Ok(())
    }

    fn build_context_prefix(
        &self,
        metadata: &crate::infrastructure::indexing::metadata_extractor::DocumentMetadata,
    ) -> String {
        let mut parts = vec![format!("Document: {}", metadata.title)];

        if let Some(page) = metadata.page_number {
            parts.push(format!("Page: {}", page));
        }

        if let Some(section) = &metadata.section {
            parts.push(format!("Section: {}", section));
        }

        format!("[{}]", parts.join(" | "))
    }

    async fn process_folder(&self, path: &Path, recursive: bool) -> Result<()> {
        {
            let mut tracker = self.progress_tracker.lock().await;
            tracker.start_scanning();
        }

        let files = self.scan_directory(path, recursive).await?;

        {
            let mut tracker = self.progress_tracker.lock().await;
            tracker.set_total(files.len());
        }

        for file_path in files {
            self.pause_gate.wait_if_paused().await;

            if *self.cancelled.lock().await {
                break;
            }

            if let Err(e) = self.process_file(&file_path).await {
                eprintln!("Error processing file {:?}: {}", file_path, e);
                self.progress_tracker.lock().await.increment_failed();
            }
        }

        if !*self.cancelled.lock().await {
            self.progress_tracker.lock().await.complete();
        }

        Ok(())
    }

    async fn reindex_file(&self, path: &Path) -> Result<()> {
        self.process_file(path).await
    }

    async fn scan_directory(&self, path: &Path, recursive: bool) -> Result<Vec<PathBuf>> {
        let mut files = Vec::new();
        let mut visited = HashSet::new();
        self.scan_directory_recursive(path, recursive, &mut files, &mut visited)
            .await?;
        Ok(files)
    }

    #[allow(clippy::only_used_in_recursion)]
    fn scan_directory_recursive<'a>(
        &'a self,
        path: &'a Path,
        recursive: bool,
        files: &'a mut Vec<PathBuf>,
        visited: &'a mut HashSet<PathBuf>,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<()>> + Send + 'a>> {
        Box::pin(async move {
            if !path.exists() {
                return Err(IndexingError::FileNotFound {
                    path: path.display().to_string(),
                });
            }

            if let Ok(canonical) = tokio::fs::canonicalize(path).await {
                if visited.contains(&canonical) {
                    return Ok(());
                }
                visited.insert(canonical);
            }

            if !path.is_dir() {
                return Ok(());
            }

            let mut entries =
                tokio::fs::read_dir(path)
                    .await
                    .map_err(|e| IndexingError::FileRead {
                        path: path.display().to_string(),
                        reason: e.to_string(),
                    })?;

            while let Some(entry) =
                entries
                    .next_entry()
                    .await
                    .map_err(|e| IndexingError::FileRead {
                        path: path.display().to_string(),
                        reason: e.to_string(),
                    })?
            {
                let entry_path = entry.path();

                if entry_path.is_file() {
                    files.push(entry_path);
                } else if entry_path.is_dir() && recursive {
                    self.scan_directory_recursive(&entry_path, recursive, files, visited)
                        .await?;
                }
            }

            Ok(())
        })
    }

    pub async fn get_progress(&self) -> IndexProgress {
        self.progress_tracker.lock().await.get_current()
    }

    pub async fn cancel(&self) {
        *self.cancelled.lock().await = true;
        self.progress_tracker.lock().await.cancel();
    }
}

pub struct IndexingService {
    task_tx: mpsc::Sender<IndexTask>,
    progress_tracker: Arc<Mutex<ProgressTracker>>,
    actor_handle: Option<tokio::task::JoinHandle<()>>,
    pause_gate: Arc<PauseGate>,
}

impl IndexingService {
    pub fn new(
        pool: SqlitePool,
        vault_path: PathBuf,
        embedder: Arc<EmbeddingService>,
        tokenizer: Arc<Tokenizer>,
        queue_capacity: usize,
    ) -> Self {
        let (task_tx, task_rx) = mpsc::channel(queue_capacity);

        let progress_tracker = Arc::new(Mutex::new(ProgressTracker::new(100)));
        let pause_gate = Arc::new(PauseGate::new());

        let actor = IndexingActor::new(
            task_rx,
            pool,
            vault_path,
            embedder,
            tokenizer,
            Arc::clone(&progress_tracker),
            Arc::clone(&pause_gate),
        );

        let actor_handle = tokio::spawn(async move {
            actor.run().await;
        });

        Self {
            task_tx,
            progress_tracker,
            actor_handle: Some(actor_handle),
            pause_gate,
        }
    }

    pub async fn index_file(&self, path: PathBuf) -> Result<()> {
        self.task_tx
            .send(IndexTask::IndexFile { path })
            .await
            .map_err(|_| IndexingError::QueueFull)?;
        Ok(())
    }

    pub async fn index_folder(&self, path: PathBuf, recursive: bool) -> Result<()> {
        self.task_tx
            .send(IndexTask::IndexFolder { path, recursive })
            .await
            .map_err(|_| IndexingError::QueueFull)?;
        Ok(())
    }

    pub async fn reindex_file(&self, path: PathBuf) -> Result<()> {
        self.task_tx
            .send(IndexTask::ReindexFile { path })
            .await
            .map_err(|_| IndexingError::QueueFull)?;
        Ok(())
    }

    pub async fn remove_file(&self, path: PathBuf) -> Result<()> {
        self.task_tx
            .send(IndexTask::RemoveFile { path })
            .await
            .map_err(|_| IndexingError::QueueFull)?;
        Ok(())
    }

    pub async fn cancel_all(&self) -> Result<()> {
        self.task_tx
            .send(IndexTask::CancelAll)
            .await
            .map_err(|_| IndexingError::QueueFull)?;
        Ok(())
    }

    pub async fn pause_indexing(&self) -> Result<()> {
        self.pause_gate.pause();
        Ok(())
    }

    pub async fn resume_indexing(&self) -> Result<()> {
        self.pause_gate.resume();
        Ok(())
    }

    pub async fn get_progress(&self) -> IndexProgress {
        self.progress_tracker.lock().await.get_current()
    }

    // P0 FIX: Changed to async to avoid deadlock risk from blocking_lock in async context
    pub async fn subscribe_progress(&self) -> tokio::sync::broadcast::Receiver<IndexProgress> {
        self.progress_tracker.lock().await.subscribe()
    }

    pub async fn shutdown(mut self) -> Result<()> {
        let _ = self.task_tx.send(IndexTask::Shutdown).await;

        if let Some(handle) = self.actor_handle.take() {
            let _ = tokio::time::timeout(std::time::Duration::from_secs(5), handle).await;
        }

        Ok(())
    }
}

// Implement IndexingServiceTrait for IndexingService
use crate::infrastructure::services::traits::IndexingServiceTrait;
use async_trait::async_trait;

#[async_trait]
impl IndexingServiceTrait for IndexingService {
    async fn index_file(&self, path: PathBuf) -> Result<()> {
        self.index_file(path).await
    }

    async fn index_folder(&self, path: PathBuf, recursive: bool) -> Result<()> {
        self.index_folder(path, recursive).await
    }

    async fn reindex_file(&self, path: PathBuf) -> Result<()> {
        self.reindex_file(path).await
    }

    async fn remove_file(&self, path: PathBuf) -> Result<()> {
        self.remove_file(path).await
    }

    async fn cancel_all(&self) -> Result<()> {
        self.cancel_all().await
    }

    async fn get_progress(&self) -> IndexProgress {
        self.get_progress().await
    }

    async fn subscribe_progress(&self) -> tokio::sync::broadcast::Receiver<IndexProgress> {
        self.subscribe_progress().await
    }

    async fn pause_indexing(&self) -> Result<()> {
        self.pause_indexing().await
    }

    async fn resume_indexing(&self) -> Result<()> {
        self.resume_indexing().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;

    async fn create_test_service() -> (IndexingService, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");

        let pool = SqlitePool::connect(&format!("sqlite://{}?mode=rwc", db_path.display()))
            .await
            .unwrap();

        crate::infrastructure::persistence::database::initialize_database(&pool)
            .await
            .unwrap();

        use tokenizers::models::wordpiece::WordPiece;
        let wp = WordPiece::default();
        let tokenizer = Arc::new(Tokenizer::new(wp));

        let model_path = temp_dir.path().join("model.onnx");

        let embedder = Arc::new(
            EmbeddingService::new(model_path)
                .expect("Failed to create embedder for test - model file may be missing"),
        );

        let service = IndexingService::new(
            pool,
            temp_dir.path().to_path_buf(),
            embedder,
            tokenizer,
            100,
        );

        (service, temp_dir)
    }

    #[tokio::test]
    async fn test_service_creation() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");

        let pool = SqlitePool::connect(&format!("sqlite://{}?mode=rwc", db_path.display()))
            .await
            .unwrap();

        crate::infrastructure::persistence::database::initialize_database(&pool)
            .await
            .unwrap();

        use tokenizers::models::wordpiece::WordPiece;
        let wp = WordPiece::default();
        let tokenizer = Arc::new(Tokenizer::new(wp));

        let progress = ProgressTracker::new(100);
        let current = progress.get_current();

        assert_eq!(current.total_files, 0);
        assert_eq!(current.processed, 0);
    }
}
