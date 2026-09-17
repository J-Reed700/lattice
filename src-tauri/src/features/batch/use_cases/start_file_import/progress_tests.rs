#![cfg(test)]

#[cfg(test)]
use super::*;
use crate::application::ports::EmbeddingPort;
use crate::infrastructure::{
    adapters::content_extraction_adapter::ContentExtractionAdapter,
    file_system::SecureFileStorage,
    persistence::{
        database::{initialize_database, DatabaseConnection},
        repositories::{
            unit_of_work::SqliteUnitOfWorkFactory, BatchJobRepository, DocumentRepositoryImpl,
            EmbeddingRepository,
        },
    },
    storage::ContentAddressedStorage,
};
use async_trait::async_trait;
use tokio::sync::Notify;

struct GatedEmbedding {
    entered: Notify,
    release: Notify,
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

#[tokio::test]
async fn batch_progress_is_committed_before_next_file_and_survives_later_failure(
) -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let db = DatabaseConnection::new(dir.path().join("test.db")).await?;
    initialize_database(db.pool()).await?;
    let repo = Arc::new(BatchJobRepository::new(db.pool().clone()));
    let uow = Arc::new(SqliteUnitOfWorkFactory::new(db.pool().clone()));
    let embedding = Arc::new(GatedEmbedding {
        entered: Notify::new(),
        release: Notify::new(),
    });
    let indexer = Arc::new(IndexFileUseCase::new(
        Arc::new(ContentAddressedStorage::with_root(
            dir.path().join("library"),
        )),
        Arc::new(SecureFileStorage::new()),
        Arc::new(ContentExtractionAdapter::new()),
        embedding.clone(),
        Arc::new(DocumentRepositoryImpl::new(db.pool().clone())),
        Arc::new(EmbeddingRepository::new(db.pool().clone())),
        uow.clone(),
    ));
    let worker =
        StartBatchFileImportUseCase::new(repo.clone(), indexer, uow).with_document_scope(Arc::new(
            crate::infrastructure::document_scope::SqliteDocumentScope::new(db.pool().clone()),
        ));
    sqlx::query("INSERT OR IGNORE INTO conversation_spaces (id, name) VALUES ('retry-space', 'Patent Training')").execute(db.pool()).await?;
    let paths = ["first.txt", "second.txt", "fail.txt"].map(|name| dir.path().join(name));
    for (path, text) in paths.iter().zip([
        "first document to keep",
        "pause here while preparing second document",
        "third document must roll back",
    ]) {
        std::fs::write(path, text)?;
    }
    // Fail after the document is inserted, while saving its embeddings.
    sqlx::query("CREATE TRIGGER fail_third_embedding BEFORE INSERT ON text_embeddings WHEN EXISTS (SELECT 1 FROM text_chunks c JOIN documents d ON c.document_id = d.id WHERE c.id = NEW.chunk_id AND d.file_name = 'fail.txt') BEGIN SELECT RAISE(ABORT, 'injected embedding save failure'); END").execute(db.pool()).await?;
    let mut files: Vec<_> = paths
        .iter()
        .map(|path| path.to_string_lossy().into_owned())
        .collect();
    files.push(files[0].clone());
    let job = worker
        .execute(StartBatchFileImportRequestDto {
            indexing: None,
            file_paths: files,
            space_id: Some("retry-space".into()),
        })
        .await?;
    tokio::time::timeout(
        std::time::Duration::from_secs(10),
        embedding.entered.notified(),
    )
    .await?;
    let status = repo.get_batch_job(&job.job_id).await?;
    assert_eq!(status.completed_items, 1);
    assert_eq!(status.progress, 0.25);
    assert_eq!(
        status
            .items
            .iter()
            .filter(|item| item.status == "processing")
            .count(),
        1
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM documents")
            .fetch_one(db.pool())
            .await?,
        1
    );
    assert!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM text_embeddings")
            .fetch_one(db.pool())
            .await?
            > 0
    );
    embedding.release.notify_one();
    let status = wait_for_job(repo.as_ref(), &job.job_id).await?;
    assert_eq!(status.status, "failed");
    assert_eq!(
        (status.completed_items, status.failed_items, status.progress),
        (3, 1, 1.0)
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM documents")
            .fetch_one(db.pool())
            .await?,
        2
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM documents WHERE status = 'indexed'")
            .fetch_one(db.pool())
            .await?,
        2
    );
    let completed: Vec<_> = status
        .items
        .iter()
        .filter(|item| item.url.ends_with("first.txt"))
        .collect();
    assert_eq!(completed[0].document_id, completed[1].document_id);
    assert!(status.items.iter().any(|item| item
        .error_message
        .as_deref()
        .is_some_and(|message| message.contains("injected embedding save failure"))));

    // Model an interrupted job: first item committed, counters stale, second
    // item running. Startup must keep the first and retry only unfinished work.
    let recovered = Uuid::new_v4().to_string();
    repo.create_batch_job(&recovered, "file_import", 2, None)
        .await?;
    repo.create_batch_items(
        &recovered,
        vec![
            paths[0].to_string_lossy().into_owned(),
            paths[1].to_string_lossy().into_owned(),
        ],
    )
    .await?;
    let items = repo.get_pending_items(&recovered).await?;
    repo.update_item_status(
        &items[0].id,
        "completed",
        completed[0].document_id.as_deref(),
        None,
    )
    .await?;
    repo.update_item_status(&items[1].id, "processing", None, None)
        .await?;
    repo.update_job_status(&recovered, "running", None, None)
        .await?;
    worker.resume_interrupted(recovered.clone()).await?;
    let status = wait_for_job(repo.as_ref(), &recovered).await?;
    assert_eq!((status.completed_items, status.failed_items), (2, 0));
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM documents")
            .fetch_one(db.pool())
            .await?,
        2
    );
    // Retry really uses the file worker and stays on the same durable record.
    // A repeat failure remains visible, with the original successes untouched.
    assert_eq!(worker.retry_failed(&job.job_id, None, None).await?, 1);
    let failed_again = wait_for_job(repo.as_ref(), &job.job_id).await?;
    assert_eq!(
        (failed_again.completed_items, failed_again.failed_items),
        (3, 1)
    );
    let failed_item = failed_again
        .items
        .iter()
        .find(|item| item.status == "failed")
        .unwrap();
    let replacement = dir.path().join("corrected.txt");
    std::fs::write(&replacement, "A corrected replacement document")?;
    // Invalid item IDs roll back the job claim and preserve the previous error.
    assert!(repo
        .requeue_failed_files(&job.job_id, Some("missing-item"), None)
        .await
        .is_err());
    assert_eq!(repo.get_batch_job(&job.job_id).await?.status, "failed");
    // Only one concurrent retry can claim a job, even before the worker starts.
    let replacement = replacement.to_string_lossy();
    let (first, second) = tokio::join!(
        repo.requeue_failed_files(&job.job_id, Some(&failed_item.id), Some(&replacement)),
        repo.requeue_failed_files(&job.job_id, Some(&failed_item.id), Some(&replacement)),
    );
    assert_ne!(first.is_ok(), second.is_ok());
    // Simulate restart after the retry is saved but before processing begins.
    worker.resume_interrupted(job.job_id.clone()).await?;
    let resolved = wait_for_job(repo.as_ref(), &job.job_id).await?;
    assert_eq!(resolved.status, "completed");
    assert_eq!((resolved.completed_items, resolved.failed_items), (4, 0));
    assert_eq!(resolved.items.len(), failed_again.items.len());
    assert!(resolved.items.iter().all(|item| failed_again
        .items
        .iter()
        .any(|original| original.id == item.id)));
    // Neither retry nor replacement adds a history block or another file row.
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM batch_jobs")
            .fetch_one(db.pool())
            .await?,
        2
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM batch_job_items")
            .fetch_one(db.pool())
            .await?,
        6
    );
    assert!(resolved
        .items
        .iter()
        .all(|item| item.error_message.is_none()));
    let replaced = resolved
        .items
        .iter()
        .find(|item| item.id == failed_item.id)
        .unwrap();
    assert_eq!(replaced.url, replacement);
    assert!(replaced.document_id.is_some());
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM document_space_memberships WHERE space_id = 'retry-space'"
        )
        .fetch_one(db.pool())
        .await?,
        3
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM documents")
            .fetch_one(db.pool())
            .await?,
        3
    );
    Ok(())
}

#[tokio::test]
async fn history_reads_job_and_file_statuses_from_one_snapshot() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let db = DatabaseConnection::new(dir.path().join("history.db")).await?;
    initialize_database(db.pool()).await?;
    let repo = BatchJobRepository::new(db.pool().clone());
    repo.create_batch_job("history", "file_import", 1, None)
        .await?;
    repo.create_batch_items("history", vec!["chapter.pdf".into()])
        .await?;
    let alternate =
        crate::infrastructure::persistence::repositories::batch_job::SqliteBatchJobRepository::new(
            db.pool().clone(),
        );
    // Repeated atomic transitions model a retry racing status reads. Neither
    // repository may combine a heading from one state with rows from another.
    let writer = async {
        for n in 0..250 {
            let status = if n % 2 == 0 { "completed" } else { "pending" };
            let count = i64::from(status == "completed");
            let mut tx = db.pool().begin().await?;
            sqlx::query(
                "UPDATE batch_jobs SET status = ?, completed_items = ? WHERE id = 'history'",
            )
            .bind(status)
            .bind(count)
            .execute(&mut *tx)
            .await?;
            sqlx::query("UPDATE batch_job_items SET status = ? WHERE job_id = 'history'")
                .bind(status)
                .execute(&mut *tx)
                .await?;
            tx.commit().await?;
        }
        Ok::<_, anyhow::Error>(())
    };
    let reader = async {
        for _ in 0..250 {
            for repository in [&repo as &dyn BatchJobRepositoryPort, &alternate] {
                let snapshot = repository.get_batch_job("history").await?;
                assert_eq!(snapshot.items[0].status, snapshot.status);
                assert_eq!(
                    snapshot.completed_items,
                    i64::from(snapshot.status == "completed")
                );
            }
        }
        Ok::<_, anyhow::Error>(())
    };
    let (written, read) = tokio::join!(writer, reader);
    written?;
    read?;
    Ok(())
}

async fn wait_for_job(
    repo: &dyn BatchJobRepositoryPort,
    id: &str,
) -> anyhow::Result<crate::application::ports::batch_job_repository_port::BatchJobStatus> {
    Ok(
        tokio::time::timeout(std::time::Duration::from_secs(10), async {
            loop {
                let status = repo.get_batch_job(id).await?;
                if matches!(status.status.as_str(), "completed" | "failed") {
                    return Ok::<_, AppError>(status);
                }
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
        })
        .await??,
    )
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
    let db = DatabaseConnection::new(dir.path().join("publication.db")).await?;
    initialize_database(db.pool()).await?;
    let repo = Arc::new(BatchJobRepository::new(db.pool().clone()));
    let uow = Arc::new(SqliteUnitOfWorkFactory::new(db.pool().clone()));
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
    let indexer = Arc::new(
        IndexFileUseCase::new(
            Arc::new(ContentAddressedStorage::with_root(
                dir.path().join("library"),
            )),
            Arc::new(SecureFileStorage::new()),
            Arc::new(ContentExtractionAdapter::new()),
            Arc::new(GatedEmbedding {
                entered: Notify::new(),
                release: Notify::new(),
            }),
            Arc::new(DocumentRepositoryImpl::new(db.pool().clone())),
            Arc::new(EmbeddingRepository::new(db.pool().clone())),
            uow.clone(),
        )
        .with_vector_search(search.clone()),
    );
    let worker = StartBatchFileImportUseCase::new(repo.clone(), indexer.clone(), uow);
    let path = dir.path().join("publication.txt");
    std::fs::write(
        &path,
        "This source must be searchable before its import is complete. ".repeat(100),
    )?;
    let job = worker
        .execute(StartBatchFileImportRequestDto {
            indexing: None,
            file_paths: vec![path.to_string_lossy().into_owned()],
            space_id: None,
        })
        .await?;
    let failed = wait_for_job(repo.as_ref(), &job.job_id).await?;
    assert_eq!(failed.status, "failed");
    assert_eq!((failed.completed_items, failed.failed_items), (0, 1));
    assert!(failed.items[0]
        .error_message
        .as_ref()
        .unwrap()
        .contains("search indexing failed"));
    let chunks: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM text_chunks")
        .fetch_one(db.pool())
        .await?;
    assert!(chunks > 0);
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM text_embeddings")
            .fetch_one(db.pool())
            .await?,
        chunks
    );
    worker.retry_failed(&job.job_id, None, None).await?;
    tokio::time::timeout(std::time::Duration::from_secs(2), search.entered.notified()).await?;
    // Single async worker: publication must yield it so chat can still write.
    let conversation_id = uuid::Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO conversations (id, title, model_name) VALUES (?, 'During import', 'test')",
    )
    .bind(&conversation_id)
    .execute(db.pool())
    .await?;
    let conversations =
        crate::features::conversation::repository::ConversationRepository::new(db.pool().clone());
    tokio::time::timeout(
        std::time::Duration::from_secs(1),
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
    let running = repo.get_batch_job(&job.job_id).await?;
    assert_eq!(running.completed_items, 0);
    assert_eq!(running.items[0].status, "processing");
    release.send(())?;
    let finished = wait_for_job(repo.as_ref(), &job.job_id).await?;
    assert_eq!((finished.completed_items, finished.failed_items), (1, 0));
    assert_eq!(search.count(), chunks as usize);
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM documents")
            .fetch_one(db.pool())
            .await?,
        1
    );
    // The standalone entry point uses the same prepare/commit/batch path.
    let second = dir.path().join("standalone.txt");
    std::fs::write(&second, "Standalone imports also publish after committing")?;
    release.send(())?;
    let result = indexer
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
    use crate::features::batch::dto::FileIndexingOptionsDto;
    let dir = tempfile::tempdir()?;
    let db = DatabaseConnection::new(dir.path().join("related.db")).await?;
    initialize_database(db.pool()).await?;
    let repo = Arc::new(BatchJobRepository::new(db.pool().clone()));
    let docs = Arc::new(DocumentRepositoryImpl::new(db.pool().clone()));
    let uow = Arc::new(SqliteUnitOfWorkFactory::new(db.pool().clone()));
    let embedding = Arc::new(GatedEmbedding {
        entered: Notify::new(),
        release: Notify::new(),
    });
    let indexer = Arc::new(IndexFileUseCase::new(
        Arc::new(ContentAddressedStorage::with_root(
            dir.path().join("library"),
        )),
        Arc::new(SecureFileStorage::new()),
        Arc::new(ContentExtractionAdapter::new()),
        embedding,
        docs.clone(),
        Arc::new(EmbeddingRepository::new(db.pool().clone())),
        uow.clone(),
    ));
    let worker = StartBatchFileImportUseCase::new(repo.clone(), indexer, uow);
    let paths: Vec<_> = ["chapter10.md", "chapter2.md"]
        .iter()
        .map(|p| dir.path().join(p).to_string_lossy().into_owned())
        .collect();
    std::fs::write(
        &paths[0],
        "# Chapter 10\nAdvanced topics.\n## 1001 Exceptions\nThese are exceptions.",
    )?;
    std::fs::write(
        &paths[1],
        "# Chapter 2\nFoundations.\n## 201 Introduction\nStart learning here.",
    )?;
    let first = worker
        .execute(StartBatchFileImportRequestDto {
            file_paths: paths.clone(),
            space_id: None,
            indexing: None,
        })
        .await?;
    let first = wait_for_job(repo.as_ref(), &first.job_id).await?;
    assert_eq!(first.failed_items, 0);
    let ids: Vec<String> = first
        .items
        .iter()
        .map(|i| i.document_id.clone().unwrap())
        .collect();
    let original_chunks: Vec<String> =
        sqlx::query_scalar("SELECT id FROM text_chunks WHERE document_id = ? ORDER BY chunk_index")
            .bind(&ids[1])
            .fetch_all(db.pool())
            .await?;
    sqlx::raw_sql("CREATE TRIGGER fail_context_rebuild BEFORE INSERT ON text_embeddings WHEN EXISTS (SELECT 1 FROM text_chunks c JOIN documents d ON d.id=c.document_id WHERE c.id=NEW.chunk_id AND d.file_name='chapter2.md') BEGIN SELECT RAISE(ABORT,'injected'); END").execute(db.pool()).await?;
    let group = SourceGroup {
        id: uuid::Uuid::new_v4().to_string(),
        title: "Zirconium Handbook".into(),
        edition: Some("Second edition".into()),
        description: Some("One ordered book".into()),
        ordered: true,
        structure: StructureMode::Sections,
    };
    let second = worker
        .execute(StartBatchFileImportRequestDto {
            file_paths: vec![paths[1].clone(), paths[0].clone()],
            space_id: None,
            indexing: Some(FileIndexingOptionsDto {
                source_group: Some(group.clone()),
            }),
        })
        .await?;
    let status = wait_for_job(repo.as_ref(), &second.job_id).await?;
    assert_eq!((status.completed_items, status.failed_items), (1, 1));
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM documents")
            .fetch_one(db.pool())
            .await?,
        2
    );
    assert_eq!(
        original_chunks,
        sqlx::query_scalar::<_, String>(
            "SELECT id FROM text_chunks WHERE document_id = ? ORDER BY chunk_index"
        )
        .bind(&ids[1])
        .fetch_all(db.pool())
        .await?
    );
    sqlx::raw_sql("DROP TRIGGER fail_context_rebuild")
        .execute(db.pool())
        .await?;
    let failed = status.items.iter().find(|i| i.status == "failed").unwrap();
    let replacement = dir.path().join("replacement.md");
    std::fs::copy(&paths[1], &replacement)?;
    worker
        .retry_failed(&second.job_id, Some(&failed.id), replacement.to_str())
        .await?;
    let status = wait_for_job(repo.as_ref(), &second.job_id).await?;
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
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM chunks_fts WHERE chunks_fts MATCH 'Zirconium'"
        )
        .fetch_one(db.pool())
        .await?
            > 0
    );
    let repo =
        crate::features::conversation::repository::ConversationRepository::new(db.pool().clone());
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
