//! Batch imports as jobs, over a real database and library: progress that
//! survives a failure, cancellation, restart recovery, retry as a new attempt,
//! history, and the attachment rules.
use super::dto::FileIndexingOptionsDto;
use super::file_job::{EmbeddingLoader, FileImporter};
use super::imports::{BatchImports, FileImport, FILE_IMPORT};
use super::items::{BatchItems, ItemState};
use super::url_job::UrlImporter;
use crate::application::ports::EmbeddingPort;
use crate::features::embedding::repository::EmbeddingRepository;
use crate::features::indexing::dto::{ChunkingStrategyDto, IndexFileRequestDto};
use crate::features::indexing::use_cases::IndexFileUseCase;
use crate::features::web::use_cases::IngestWebUrlUseCase;
use crate::features::web::{WebIngestionResult, WebIngestionServiceTrait};
use crate::infrastructure::persistence::repositories::document_scope::SqliteDocumentScope;
use crate::infrastructure::{
    adapters::content_extraction_adapter::ContentExtractionAdapter,
    file_system::SecureFileStorage,
    persistence::{
        database::{initialize_database, DatabaseConnection},
        repositories::{unit_of_work::SqliteUnitOfWorkFactory, DocumentRepositoryImpl},
    },
    storage::ContentAddressedStorage,
};
use crate::shared::{
    error::{AppError, Result},
    runtime::jobs::{JobRuntime, JobStatus, JobStore, NewJob},
};
use async_trait::async_trait;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Notify;

const WAIT: Duration = Duration::from_secs(10);

struct GatedEmbedding {
    entered: Notify,
    release: Notify,
}

impl GatedEmbedding {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            entered: Notify::new(),
            release: Notify::new(),
        })
    }
}

#[async_trait]
impl EmbeddingPort for GatedEmbedding {
    async fn embed_single(&self, _text: &str) -> Result<Vec<f32>> {
        Ok(vec![0.25; 384])
    }
    async fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        if texts.iter().any(|text| text.contains("pause here")) {
            self.entered.notify_one();
            self.release.notified().await;
        }
        Ok(vec![vec![0.25; 384]; texts.len()])
    }
    fn dimension(&self) -> usize {
        384
    }
    async fn is_ready(&self) -> Result<bool> {
        Ok(true)
    }
}

/// Ingests a URL as a document row of its own, the way a real page would.
struct RecordingIngestion {
    pool: sqlx::SqlitePool,
    ingested: parking_lot::Mutex<Vec<String>>,
}

#[async_trait]
impl WebIngestionServiceTrait for RecordingIngestion {
    async fn ingest_url(&self, url: &str) -> Result<WebIngestionResult> {
        self.ingested.lock().push(url.to_string());
        if url.contains("broken") {
            return Err(AppError::Network("connection refused".into()));
        }
        let id = uuid::Uuid::new_v4().to_string();
        sqlx::query("INSERT INTO documents (id,file_path,file_name,size_bytes,modified_at,checksum,source_type) VALUES (?,?,?,100,'2026-10-09',?,'web')")
            .bind(&id)
            .bind(url)
            .bind(url)
            .bind(&id)
            .execute(&self.pool)
            .await
            .map_err(|e| AppError::Database(e.to_string()))?;
        Ok(WebIngestionResult {
            document_id: id,
            url: url.to_string(),
            title: "Page".into(),
            word_count: 10,
            chunks_created: 1,
            site_name: None,
            author: None,
            reading_time_minutes: None,
        })
    }
}

/// Everything a batch import needs, over one real database and library.
struct ImportHarness {
    db: DatabaseConnection,
    indexer: Arc<IndexFileUseCase>,
    embedding: Arc<GatedEmbedding>,
    ingestion: Arc<RecordingIngestion>,
    jobs: Arc<JobRuntime>,
    imports: BatchImports,
    library: PathBuf,
}

impl ImportHarness {
    async fn new(dir: &Path) -> anyhow::Result<Self> {
        Self::with_indexer(dir, GatedEmbedding::new(), |indexer| indexer).await
    }

    async fn with_indexer(
        dir: &Path,
        embedding: Arc<GatedEmbedding>,
        configure: impl FnOnce(IndexFileUseCase) -> IndexFileUseCase,
    ) -> anyhow::Result<Self> {
        let db = DatabaseConnection::new(dir.join("test.db")).await?;
        initialize_database(db.pool()).await?;
        let uow = Arc::new(SqliteUnitOfWorkFactory::new(db.pool().clone()));
        let library = dir.join("library");
        let indexer = Arc::new(configure(IndexFileUseCase::new(
            Arc::new(ContentAddressedStorage::with_root(library.clone())),
            Arc::new(SecureFileStorage::new()),
            Arc::new(ContentExtractionAdapter::new()),
            embedding.clone(),
            Arc::new(DocumentRepositoryImpl::new(db.pool().clone())),
            Arc::new(EmbeddingRepository::new(db.pool().clone())),
            uow,
        )));
        let ingestion = Arc::new(RecordingIngestion {
            pool: db.pool().clone(),
            ingested: parking_lot::Mutex::new(Vec::new()),
        });
        let jobs = JobRuntime::with_poll_interval(db.pool().clone(), Duration::from_millis(50));
        let imports = BatchImports::new(
            jobs.clone(),
            BatchItems::new(db.pool().clone()),
            Arc::new(SqliteDocumentScope::new(db.pool().clone())),
        );
        let harness = Self {
            db,
            indexer,
            embedding,
            ingestion,
            jobs,
            imports,
            library,
        };
        harness.start().await?;
        Ok(harness)
    }

    /// Registers the import kinds on the harness's runtime, as the app does
    /// at start.
    async fn start(&self) -> Result<()> {
        let embedding = self.embedding.clone();
        let load: EmbeddingLoader = Arc::new(move || {
            let embedding = embedding.clone() as Arc<dyn EmbeddingPort>;
            Box::pin(async move { Ok(embedding) })
        });
        let pool = self.db.pool().clone();
        super::worker::register(
            &self.jobs,
            &BatchItems::new(pool.clone()),
            FileImporter {
                index_file: self.indexer.clone(),
                uow_factory: Arc::new(SqliteUnitOfWorkFactory::new(pool.clone())),
                document_scope: Arc::new(SqliteDocumentScope::new(pool)),
                load_embedding: load.clone(),
            },
            UrlImporter {
                ingest: Arc::new(IngestWebUrlUseCase::new(
                    self.ingestion.clone() as Arc<dyn WebIngestionServiceTrait>
                )),
                load_embedding: load,
            },
        )
        .await
    }

    /// The app closes and opens again: the runtime stops at its workers'
    /// cancellation points, and a new one takes over the saved jobs.
    async fn restart(&mut self) -> Result<()> {
        self.jobs.close();
        self.jobs.drain().await;
        self.jobs =
            JobRuntime::with_poll_interval(self.db.pool().clone(), Duration::from_millis(50));
        self.imports = BatchImports::new(
            self.jobs.clone(),
            BatchItems::new(self.db.pool().clone()),
            Arc::new(SqliteDocumentScope::new(self.db.pool().clone())),
        );
        self.start().await
    }

    async fn files(&self, paths: &[PathBuf], space_id: Option<&str>) -> Result<String> {
        self.imports
            .start_files(FileImport {
                file_paths: paths
                    .iter()
                    .map(|path| path.to_string_lossy().into_owned())
                    .collect(),
                space_id: space_id.map(str::to_owned),
                owner_conversation_id: None,
                indexing: None,
            })
            .await
    }

    async fn wait(
        &self,
        job_id: &str,
    ) -> anyhow::Result<crate::features::batch::dto::BatchJobStatusDto> {
        Ok(tokio::time::timeout(WAIT, async {
            loop {
                let status = self.imports.status(job_id).await?;
                if matches!(status.status.as_str(), "completed" | "failed" | "cancelled") {
                    return Ok::<_, AppError>(status);
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await??)
    }

    async fn count(&self, sql: &str) -> anyhow::Result<i64> {
        Ok(sqlx::query_scalar::<_, i64>(sql)
            .fetch_one(self.db.pool())
            .await?)
    }

    /// Blob directories currently on disk.
    fn blobs(&self) -> usize {
        std::fs::read_dir(&self.library)
            .map(|entries| entries.count())
            .unwrap_or(0)
    }

    async fn item_states(&self, job_id: &str) -> anyhow::Result<Vec<String>> {
        Ok(self
            .imports
            .status(job_id)
            .await?
            .items
            .into_iter()
            .map(|item| item.status)
            .collect())
    }
}

fn write_files(dir: &Path, files: &[(&str, &str)]) -> Vec<PathBuf> {
    files
        .iter()
        .map(|(name, text)| {
            let path = dir.join(name);
            std::fs::write(&path, text).unwrap();
            path
        })
        .collect()
}

#[tokio::test]
async fn each_file_is_committed_before_the_next_and_a_later_failure_keeps_them(
) -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let harness = ImportHarness::new(dir.path()).await?;
    sqlx::query("INSERT OR IGNORE INTO conversation_spaces (id, name) VALUES ('retry-space', 'Patent Training')").execute(harness.db.pool()).await?;
    let mut paths = write_files(
        dir.path(),
        &[
            ("first.txt", "first document to keep"),
            ("second.txt", "pause here while preparing second document"),
            ("fail.txt", "third document must roll back"),
        ],
    );
    // Fail after the document is inserted, while saving its embeddings.
    sqlx::query("CREATE TRIGGER fail_third_embedding BEFORE INSERT ON text_embeddings WHEN EXISTS (SELECT 1 FROM text_chunks c JOIN documents d ON c.document_id = d.id WHERE c.id = NEW.chunk_id AND d.file_name = 'fail.txt') BEGIN SELECT RAISE(ABORT, 'injected embedding save failure'); END").execute(harness.db.pool()).await?;
    paths.push(paths[0].clone());
    let job_id = harness.files(&paths, Some("retry-space")).await?;
    tokio::time::timeout(WAIT, harness.embedding.entered.notified()).await?;
    let status = harness.imports.status(&job_id).await?;
    assert_eq!(status.completed_items, 1);
    assert_eq!(
        status
            .items
            .iter()
            .filter(|item| item.status == "processing")
            .count(),
        1
    );
    let job = harness.jobs.store().get(&job_id).await?;
    assert_eq!(
        (job.status, job.progress_current, job.progress_total),
        (JobStatus::Running, 1, 4),
        "the job's progress is the files done"
    );
    assert_eq!(harness.count("SELECT count(*) FROM documents").await?, 1);
    assert!(
        harness
            .count("SELECT count(*) FROM text_embeddings")
            .await?
            > 0
    );

    harness.embedding.release.notify_one();
    let status = harness.wait(&job_id).await?;
    assert_eq!(status.status, "failed");
    assert_eq!((status.completed_items, status.failed_items), (3, 1));
    assert_eq!(harness.count("SELECT count(*) FROM documents").await?, 2);
    assert_eq!(
        harness
            .count("SELECT count(*) FROM documents WHERE status = 'indexed'")
            .await?,
        2
    );
    let first: Vec<_> = status
        .items
        .iter()
        .filter(|item| item.target.ends_with("first.txt"))
        .collect();
    assert_eq!(first[0].document_id, first[1].document_id);
    assert!(status.items.iter().any(|item| item
        .error_message
        .as_deref()
        .is_some_and(|message| message.contains("injected embedding save failure"))));
    let job = harness.jobs.store().get(&job_id).await?;
    assert_eq!(job.error_code.as_deref(), Some("items_failed"));

    // A retry is a new attempt that takes over the items; the failure repeats
    // and the successes stay untouched.
    let (attempt, retried) = harness.imports.retry(&job_id, None, None).await?;
    assert_ne!(attempt, job_id);
    assert_eq!(retried, 1);
    let failed_again = harness.wait(&attempt).await?;
    assert_eq!(
        (failed_again.completed_items, failed_again.failed_items),
        (3, 1)
    );
    assert!(harness.imports.status(&job_id).await?.items.is_empty());
    let failed_item = failed_again
        .items
        .iter()
        .find(|item| item.status == "failed")
        .unwrap()
        .clone();
    let replacement = dir.path().join("corrected.txt");
    std::fs::write(&replacement, "A corrected replacement document")?;
    // An unknown item rolls the whole retry back.
    assert!(harness
        .imports
        .retry(&attempt, Some("missing-item"), None)
        .await
        .is_err());
    assert_eq!(
        harness.jobs.store().get(&attempt).await?.status,
        JobStatus::Failed
    );
    // Only one of two concurrent retries takes the attempt.
    let replacement = replacement.to_string_lossy().into_owned();
    let (one, two) = tokio::join!(
        harness
            .imports
            .retry(&attempt, Some(&failed_item.item_id), Some(&replacement)),
        harness
            .imports
            .retry(&attempt, Some(&failed_item.item_id), Some(&replacement)),
    );
    assert_ne!(one.is_ok(), two.is_ok());
    let (resolved_id, _) = one.or(two)?;
    let resolved = harness.wait(&resolved_id).await?;
    assert_eq!(resolved.status, "completed");
    assert_eq!((resolved.completed_items, resolved.failed_items), (4, 0));
    assert!(resolved.items.iter().all(|item| failed_again
        .items
        .iter()
        .any(|original| original.item_id == item.item_id)));
    assert!(resolved
        .items
        .iter()
        .all(|item| item.error_message.is_none()));
    let replaced = resolved
        .items
        .iter()
        .find(|item| item.item_id == failed_item.item_id)
        .unwrap();
    assert_eq!(replaced.target, replacement);
    assert!(replaced.document_id.is_some());

    // Retries add no rows: the history lists the import once, as its latest
    // attempt.
    assert_eq!(
        harness
            .count("SELECT count(*) FROM batch_import_items")
            .await?,
        4
    );
    let history = harness.imports.list(None, None).await?;
    assert_eq!(history.len(), 1);
    assert_eq!(history[0].job_id, resolved_id);
    assert_eq!(history[0].job_type, "file_import");
    assert_eq!(
        harness
            .count("SELECT count(*) FROM document_space_memberships WHERE space_id = 'retry-space'")
            .await?,
        3
    );
    assert_eq!(harness.count("SELECT count(*) FROM documents").await?, 3);

    // Deleting it takes every attempt and item, and keeps the documents.
    harness.imports.delete(&resolved_id).await?;
    assert!(harness.imports.list(None, None).await?.is_empty());
    assert_eq!(harness.count("SELECT count(*) FROM jobs").await?, 0);
    assert_eq!(
        harness
            .count("SELECT count(*) FROM batch_import_items")
            .await?,
        0
    );
    assert_eq!(harness.count("SELECT count(*) FROM documents").await?, 3);
    Ok(())
}

#[tokio::test]
async fn an_import_the_app_left_running_resumes_with_its_unfinished_files() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let mut harness = ImportHarness::new(dir.path()).await?;
    let paths = write_files(
        dir.path(),
        &[
            ("first.txt", "first document to keep"),
            ("second.txt", "second document"),
        ],
    );
    let done = harness.files(&paths[..1], None).await?;
    let first_document = harness.wait(&done).await?.items[0].document_id.clone();
    harness.restart().await?;
    harness.jobs.close();
    harness.jobs.drain().await;

    // The shape a crash leaves: the first file committed, the second in
    // flight, the job running.
    let store = JobStore::new(harness.db.pool().clone());
    let targets: Vec<String> = paths
        .iter()
        .map(|path| path.to_string_lossy().into_owned())
        .collect();
    let mut tx = harness.db.pool().begin().await?;
    let admitted = JobStore::submit_in(
        &mut tx,
        &NewJob {
            kind: FILE_IMPORT.into(),
            subject_id: None,
            operation_id: "interrupted".into(),
            payload_hash: "interrupted".into(),
            requested: serde_json::json!({}),
            progress_total: 2,
            message: "Queued".into(),
        },
    )
    .await?;
    BatchItems::insert_in(&mut tx, &admitted.id, &targets).await?;
    tx.commit().await?;
    store.claim(&admitted.id).await?;
    let items = BatchItems::new(harness.db.pool().clone());
    let saved = items.list(&admitted.id).await?;
    items
        .finish(
            &saved[0].id,
            ItemState::Completed,
            first_document.as_deref(),
            None,
        )
        .await?;
    sqlx::query("UPDATE batch_import_items SET status='processing' WHERE id=?")
        .bind(&saved[1].id)
        .execute(harness.db.pool())
        .await?;

    harness.restart().await?;
    let status = harness.wait(&admitted.id).await?;
    assert_eq!(status.status, "completed");
    assert_eq!((status.completed_items, status.failed_items), (2, 0));
    assert_eq!(status.items[0].document_id, first_document);
    assert_eq!(harness.count("SELECT count(*) FROM documents").await?, 2);
    Ok(())
}

#[tokio::test]
async fn cancelling_mid_file_releases_its_library_blob_and_nothing_restarts_it(
) -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let mut harness = ImportHarness::new(dir.path()).await?;
    let paths = write_files(
        dir.path(),
        &[
            (
                "in_flight.txt",
                "pause here while this document is prepared",
            ),
            ("queued.txt", "this document is never reached"),
        ],
    );
    let job_id = harness.files(&paths, None).await?;
    tokio::time::timeout(WAIT, harness.embedding.entered.notified()).await?;
    // The blob is already in the library, and nothing references it yet.
    assert_eq!(harness.blobs(), 1);

    assert_eq!(harness.imports.cancel(&job_id).await?, 2);
    assert_eq!(
        harness.item_states(&job_id).await?,
        ["cancelled", "cancelled"],
        "the file in flight is settled at once"
    );
    harness.embedding.release.notify_one();
    tokio::time::timeout(WAIT, async {
        while harness.jobs.store().get(&job_id).await.is_ok() && harness.blobs() > 0 {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await?;
    assert_eq!(
        harness.blobs(),
        0,
        "the blob copied for the cancelled file is released at cancel time"
    );
    assert_eq!(harness.count("SELECT count(*) FROM documents").await?, 0);
    assert_eq!(
        harness.jobs.store().get(&job_id).await?.status,
        JobStatus::Cancelled
    );

    // Recovery must not restart work the user cancelled.
    harness.restart().await?;
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert_eq!(
        harness.item_states(&job_id).await?,
        ["cancelled", "cancelled"]
    );
    assert_eq!(harness.count("SELECT count(*) FROM documents").await?, 0);
    assert!(
        harness.imports.retry(&job_id, None, None).await.is_err(),
        "nothing failed, so nothing to retry"
    );
    Ok(())
}

#[tokio::test]
async fn an_import_reads_its_status_and_its_files_from_one_snapshot() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let harness = ImportHarness::new(dir.path()).await?;
    // Nothing runs it: the job is only read while it is rewritten.
    harness.jobs.close();
    harness.jobs.drain().await;
    let job_id = harness
        .files(&[dir.path().join("chapter.pdf")], None)
        .await?;
    // Repeated atomic transitions model a retry racing status reads. A read
    // may never combine the job's heading from one state with rows from
    // another.
    let writer = async {
        for n in 0..250 {
            let done = n % 2 == 0;
            let mut tx = harness.db.pool().begin().await?;
            sqlx::query("UPDATE jobs SET status=?, finished_at=?, result_ref=? WHERE id=?")
                .bind(if done { "completed" } else { "pending" })
                .bind(done.then_some(1_i64))
                .bind(done.then_some("done"))
                .bind(&job_id)
                .execute(&mut *tx)
                .await?;
            sqlx::query("UPDATE batch_import_items SET status=? WHERE job_id=?")
                .bind(if done { "completed" } else { "pending" })
                .bind(&job_id)
                .execute(&mut *tx)
                .await?;
            tx.commit().await?;
        }
        Ok::<_, anyhow::Error>(())
    };
    let reader = async {
        for _ in 0..250 {
            let snapshot = harness.imports.status(&job_id).await?;
            assert_eq!(snapshot.items[0].status, snapshot.status);
            assert_eq!(
                snapshot.completed_items,
                i64::from(snapshot.status == "completed")
            );
        }
        Ok::<_, anyhow::Error>(())
    };
    let (written, read) = tokio::join!(writer, reader);
    written?;
    read?;
    Ok(())
}

struct PublicationProbe {
    index: crate::features::search::engine::vector_search::USearchVectorIndex,
    fail_first: std::sync::atomic::AtomicBool,
    entered: Notify,
    release: std::sync::Mutex<std::sync::mpsc::Receiver<()>>,
}

impl crate::application::ports::VectorSearchPort for PublicationProbe {
    fn search(
        &self,
        vector: &[f32],
        count: usize,
        threshold: f32,
    ) -> Result<Vec<crate::features::search::dto::SearchResultPortDto>> {
        crate::application::ports::VectorSearchPort::search(&self.index, vector, count, threshold)
    }
    fn search_scoped(
        &self,
        vector: &[f32],
        count: usize,
        threshold: f32,
        allowed_document_ids: Option<&std::collections::HashSet<String>>,
    ) -> Result<Vec<crate::features::search::dto::SearchResultPortDto>> {
        crate::application::ports::VectorSearchPort::search_scoped(
            &self.index,
            vector,
            count,
            threshold,
            allowed_document_ids,
        )
    }
    fn add_embedding(&self, _id: String, _embedding: Vec<f32>) -> Result<()> {
        panic!("Document publication must use a batch")
    }
    fn publish_embeddings(
        &self,
        entries: Vec<crate::application::ports::vector_search_port::VectorIndexEntry>,
    ) -> Result<()> {
        if self
            .fail_first
            .swap(false, std::sync::atomic::Ordering::SeqCst)
        {
            return Err(AppError::Other("injected search disk failure".into()));
        }
        self.entered.notify_one();
        self.release
            .lock()
            .unwrap()
            .recv_timeout(std::time::Duration::from_secs(5))
            .unwrap();
        self.index.publish_embeddings(entries)
    }
    fn remove_embedding(&self, id: &str) -> Result<()> {
        self.index.remove_embedding(id)
    }
    fn clear(&self) -> Result<()> {
        self.index.clear()
    }
    fn count(&self) -> usize {
        self.index.count()
    }
    fn dimension(&self) -> usize {
        self.index.dimension()
    }
}

#[tokio::test]
async fn search_publication_failure_is_retryable_and_does_not_block_chat() -> anyhow::Result<()> {
    use crate::application::ports::VectorSearchPort;
    let dir = tempfile::tempdir()?;
    let (release, receiver) = std::sync::mpsc::channel();
    let search = Arc::new(PublicationProbe {
        index: crate::features::search::engine::vector_search::USearchVectorIndex::new(
            384,
            Some(dir.path().join("search.usearch")),
        )?,
        fail_first: std::sync::atomic::AtomicBool::new(true),
        entered: Notify::new(),
        release: std::sync::Mutex::new(receiver),
    });
    let probe = search.clone();
    let harness = ImportHarness::with_indexer(dir.path(), GatedEmbedding::new(), |indexer| {
        indexer.with_vector_search(probe)
    })
    .await?;
    let path = dir.path().join("publication.txt");
    std::fs::write(
        &path,
        "This source must be searchable before its import is complete. ".repeat(100),
    )?;
    let job_id = harness.files(std::slice::from_ref(&path), None).await?;
    let failed = harness.wait(&job_id).await?;
    assert_eq!(failed.status, "failed");
    assert_eq!((failed.completed_items, failed.failed_items), (0, 1));
    assert!(failed.items[0]
        .error_message
        .as_ref()
        .unwrap()
        .contains("search indexing failed"));
    let chunks = harness.count("SELECT COUNT(*) FROM text_chunks").await?;
    assert!(chunks > 0);
    assert_eq!(
        harness
            .count("SELECT COUNT(*) FROM text_embeddings")
            .await?,
        chunks
    );
    let (attempt, _) = harness.imports.retry(&job_id, None, None).await?;
    tokio::time::timeout(Duration::from_secs(2), search.entered.notified()).await?;
    // Publication must not hold the database, so chat can still write.
    let conversation_id = uuid::Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO conversations (id, title, model_name) VALUES (?, 'During import', 'test')",
    )
    .bind(&conversation_id)
    .execute(harness.db.pool())
    .await?;
    let conversations = crate::features::conversation::repository::ConversationRepository::new(
        harness.db.pool().clone(),
    );
    tokio::time::timeout(
        Duration::from_secs(1),
        conversations.add_message_with_status(
            &conversation_id,
            crate::domain::conversation::MessageRole::User,
            "Chat stays writable",
            4,
            None,
            "pending",
        ),
    )
    .await??;
    let running = harness.imports.status(&attempt).await?;
    assert_eq!(running.completed_items, 0);
    assert_eq!(running.items[0].status, "processing");
    release.send(())?;
    let finished = harness.wait(&attempt).await?;
    assert_eq!((finished.completed_items, finished.failed_items), (1, 0));
    assert_eq!(search.count(), chunks as usize);
    assert_eq!(harness.count("SELECT COUNT(*) FROM documents").await?, 1);
    // The standalone entry point uses the same prepare/commit/batch path.
    let second = dir.path().join("standalone.txt");
    std::fs::write(&second, "Standalone imports also publish after committing")?;
    release.send(())?;
    let result = harness
        .indexer
        .execute(IndexFileRequestDto {
            path: second.to_string_lossy().into_owned(),
            chunking_strategy: ChunkingStrategyDto::Semantic { max_tokens: 800 },
            tags: None,
            metadata: None,
            space_id: None,
        })
        .await?;
    assert_eq!(result.status, "imported");
    assert!(result.chunks_created > 0);
    assert_eq!(search.count(), chunks as usize + result.chunks_created);
    Ok(())
}

#[tokio::test]
async fn related_source_rebuild_preserves_identity_context_order_and_failed_index(
) -> anyhow::Result<()> {
    use crate::application::ports::RepositoryPort;
    use crate::domain::value_objects::source_context::{SourceGroup, StructureMode};
    let dir = tempfile::tempdir()?;
    let harness = ImportHarness::new(dir.path()).await?;
    let docs = DocumentRepositoryImpl::new(harness.db.pool().clone());
    let paths = write_files(
        dir.path(),
        &[
            (
                "chapter10.md",
                "# Chapter 10\nAdvanced topics.\n## 1001 Exceptions\nThese are exceptions.",
            ),
            (
                "chapter2.md",
                "# Chapter 2\nFoundations.\n## 201 Introduction\nStart learning here.",
            ),
        ],
    );
    let first = harness.files(&paths, None).await?;
    let first = harness.wait(&first).await?;
    assert_eq!(first.failed_items, 0);
    let ids: Vec<String> = first
        .items
        .iter()
        .map(|i| i.document_id.clone().unwrap())
        .collect();
    let original_chunks: Vec<String> =
        sqlx::query_scalar("SELECT id FROM text_chunks WHERE document_id = ? ORDER BY chunk_index")
            .bind(&ids[1])
            .fetch_all(harness.db.pool())
            .await?;
    sqlx::raw_sql("CREATE TRIGGER fail_context_rebuild BEFORE INSERT ON text_embeddings WHEN EXISTS (SELECT 1 FROM text_chunks c JOIN documents d ON d.id=c.document_id WHERE c.id=NEW.chunk_id AND d.file_name='chapter2.md') BEGIN SELECT RAISE(ABORT,'injected'); END").execute(harness.db.pool()).await?;
    let group = SourceGroup {
        id: uuid::Uuid::new_v4().to_string(),
        title: "Zirconium Handbook".into(),
        edition: Some("Second edition".into()),
        description: Some("One ordered book".into()),
        ordered: true,
        structure: StructureMode::Sections,
    };
    let second = harness
        .imports
        .start_files(FileImport {
            file_paths: vec![
                paths[1].to_string_lossy().into_owned(),
                paths[0].to_string_lossy().into_owned(),
            ],
            space_id: None,
            owner_conversation_id: None,
            indexing: Some(FileIndexingOptionsDto {
                source_group: Some(group.clone()),
            }),
        })
        .await?;
    let status = harness.wait(&second).await?;
    assert_eq!((status.completed_items, status.failed_items), (1, 1));
    assert_eq!(harness.count("SELECT count(*) FROM documents").await?, 2);
    assert_eq!(
        original_chunks,
        sqlx::query_scalar::<_, String>(
            "SELECT id FROM text_chunks WHERE document_id = ? ORDER BY chunk_index"
        )
        .bind(&ids[1])
        .fetch_all(harness.db.pool())
        .await?
    );
    sqlx::raw_sql("DROP TRIGGER fail_context_rebuild")
        .execute(harness.db.pool())
        .await?;
    let failed = status.items.iter().find(|i| i.status == "failed").unwrap();
    let replacement = dir.path().join("replacement.md");
    std::fs::copy(&paths[1], &replacement)?;
    let (attempt, _) = harness
        .imports
        .retry(&second, Some(&failed.item_id), replacement.to_str())
        .await?;
    let status = harness.wait(&attempt).await?;
    assert_eq!((status.completed_items, status.failed_items), (2, 0));
    let saved = docs.find_by_id(&ids[1]).await?.unwrap();
    let context = saved.source_context().unwrap();
    assert_eq!(
        (context.group.id.as_str(), context.position),
        (group.id.as_str(), 0)
    );
    assert_eq!(saved.id().as_str(), ids[1]);
    assert!(saved
        .chunks()
        .iter()
        .all(|c| c.start_char().is_some() && c.end_char().is_some()));
    assert!(saved
        .chunks()
        .iter()
        .any(|c| c.section().is_some_and(|s| s.contains("201 Introduction"))));
    assert!(saved
        .chunks()
        .iter()
        .all(|c| c.context_prefix().unwrap().contains("Zirconium")));
    assert!(saved
        .chunks()
        .iter()
        .all(|c| !c.content().contains("Zirconium")));
    assert!(
        harness
            .count("SELECT count(*) FROM chunks_fts WHERE chunks_fts MATCH 'Zirconium'")
            .await?
            > 0
    );
    let repo = crate::features::conversation::repository::ConversationRepository::new(
        harness.db.pool().clone(),
    );
    let allowed: std::collections::HashSet<_> = ids.iter().cloned().collect();
    let catalog = repo.retrieval_catalog(&allowed).await?;
    assert_eq!(catalog[0].id, ids[1]);
    assert!(!catalog[0].sections.is_empty());
    let section = repo
        .retrieval_section_passages(&["201".into()], &allowed)
        .await?;
    assert!(!section.is_empty());
    assert!(section.iter().all(|s| s.document_id == ids[1]));
    assert!(repo
        .retrieval_section_passages(&["201".into()], &[ids[0].clone()].into())
        .await?
        .is_empty());
    Ok(())
}

/// The attachment rule on the Duplicate path: a file already attached to one
/// chat moves to the chat that attaches it next, a file filed in the library
/// stays filed, and a document this item committed before its owner stamp
/// failed is stamped on retry.
#[tokio::test]
async fn attaching_an_already_indexed_file_follows_the_owner_rule() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let harness = ImportHarness::new(dir.path()).await?;
    let pool = harness.db.pool().clone();
    sqlx::query("INSERT INTO conversations (id, title, model_name) VALUES ('chat-a', 'A', 'm'), ('chat-b', 'B', 'm')")
        .execute(&pool)
        .await?;
    let owner_of = |name: &'static str| {
        let pool = pool.clone();
        async move {
            sqlx::query_scalar::<_, Option<String>>(
                "SELECT owner_conversation_id FROM documents WHERE file_name = ?",
            )
            .bind(name)
            .fetch_one(&pool)
            .await
        }
    };
    let import = |path: &Path, owner: Option<&str>| {
        harness.imports.start_files(FileImport {
            file_paths: vec![path.to_string_lossy().into_owned()],
            space_id: None,
            owner_conversation_id: owner.map(str::to_owned),
            indexing: None,
        })
    };

    // Attached to A, then to B: B owns it now.
    let shared = dir.path().join("shared.txt");
    std::fs::write(&shared, "a file two chats attach")?;
    harness
        .wait(&import(&shared, Some("chat-a")).await?)
        .await?;
    assert_eq!(owner_of("shared.txt").await?.as_deref(), Some("chat-a"));
    harness
        .wait(&import(&shared, Some("chat-b")).await?)
        .await?;
    assert_eq!(owner_of("shared.txt").await?.as_deref(), Some("chat-b"));

    // Filed in the library first: attaching it leaves it filed.
    let filed = dir.path().join("filed.txt");
    std::fs::write(&filed, "a file the user filed in the library")?;
    harness.wait(&import(&filed, None).await?).await?;
    harness.wait(&import(&filed, Some("chat-a")).await?).await?;
    assert_eq!(owner_of("filed.txt").await?, None);

    // The stamp fails after the commit; the retry finishes it.
    sqlx::query("CREATE TRIGGER fail_owner_stamp BEFORE UPDATE OF owner_conversation_id ON documents BEGIN SELECT RAISE(ABORT, 'injected owner stamp failure'); END")
        .execute(&pool)
        .await?;
    let late = dir.path().join("late.txt");
    std::fs::write(&late, "a file whose owner stamp fails once")?;
    let job_id = import(&late, Some("chat-a")).await?;
    let status = harness.wait(&job_id).await?;
    assert_eq!(status.failed_items, 1);
    assert_eq!(owner_of("late.txt").await?, None);
    sqlx::query("DROP TRIGGER fail_owner_stamp")
        .execute(&pool)
        .await?;
    let (attempt, _) = harness.imports.retry(&job_id, None, None).await?;
    let status = harness.wait(&attempt).await?;
    assert_eq!(status.completed_items, 1);
    assert_eq!(owner_of("late.txt").await?.as_deref(), Some("chat-a"));
    Ok(())
}

#[tokio::test]
async fn a_url_import_ingests_each_url_and_records_the_one_that_failed() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let harness = ImportHarness::new(dir.path()).await?;
    let urls = vec![
        "https://example.com/article1".to_string(),
        "https://example.com/broken".to_string(),
        "https://example.com/article2".to_string(),
    ];
    let job_id = harness.imports.start_urls(&urls, None).await?;
    // The items are saved, in order, before the import is handed back.
    let queued = BatchItems::new(harness.db.pool().clone())
        .list(&job_id)
        .await?;
    assert_eq!(
        queued
            .iter()
            .map(|item| item.target.clone())
            .collect::<Vec<_>>(),
        urls
    );
    let status = harness.wait(&job_id).await?;
    assert_eq!(status.job_type, "url_import");
    assert_eq!(status.status, "failed");
    assert_eq!((status.completed_items, status.failed_items), (2, 1));
    assert!(status.items[1]
        .error_message
        .as_deref()
        .is_some_and(|message| message.contains("connection refused")));
    assert!(status
        .items
        .iter()
        .filter(|item| item.status == "completed")
        .all(|item| item.document_id.is_some()));
    assert_eq!(*harness.ingestion.ingested.lock(), urls);
    assert!(
        harness
            .imports
            .retry(&job_id, Some(&status.items[1].item_id), Some("/tmp/x.txt"))
            .await
            .is_err(),
        "a URL import has no file to replace"
    );
    Ok(())
}

#[tokio::test]
async fn imports_are_checked_before_anything_is_saved() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let harness = ImportHarness::new(dir.path()).await?;
    let imports = &harness.imports;
    for url in ["ftp://example.com", "javascript:alert(1)", "", "not-a-url"] {
        assert!(
            imports.start_urls(&[url.to_string()], None).await.is_err(),
            "{url:?}"
        );
    }
    assert!(imports.start_urls(&[], None).await.is_err());
    let too_many: Vec<String> = (0..=50)
        .map(|n| format!("https://example.com/{n}"))
        .collect();
    assert!(imports.start_urls(&too_many, None).await.is_err());
    assert!(harness.files(&[], None).await.is_err());
    let too_many: Vec<PathBuf> = (0..=100)
        .map(|n| dir.path().join(format!("{n}.txt")))
        .collect();
    assert!(harness.files(&too_many, None).await.is_err());
    assert!(harness
        .files(&[PathBuf::from("../../../etc/passwd")], None)
        .await
        .is_err());
    assert!(harness
        .files(&[dir.path().join("a.txt")], Some("no-such-space"))
        .await
        .is_err());
    assert_eq!(harness.count("SELECT count(*) FROM jobs").await?, 0);
    assert_eq!(
        harness
            .count("SELECT count(*) FROM batch_import_items")
            .await?,
        0
    );
    Ok(())
}

#[tokio::test]
async fn a_running_import_can_be_neither_retried_nor_deleted() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let harness = ImportHarness::new(dir.path()).await?;
    let paths = write_files(dir.path(), &[("slow.txt", "pause here until released")]);
    let job_id = harness.files(&paths, None).await?;
    tokio::time::timeout(WAIT, harness.embedding.entered.notified()).await?;
    assert!(harness.imports.retry(&job_id, None, None).await.is_err());
    assert!(harness.imports.delete(&job_id).await.is_err());
    harness.embedding.release.notify_one();
    assert_eq!(harness.wait(&job_id).await?.status, "completed");
    harness.imports.delete(&job_id).await?;
    assert_eq!(harness.count("SELECT count(*) FROM documents").await?, 1);
    Ok(())
}
