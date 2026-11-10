//! Tests for preparing, committing and publishing a single imported file.

use super::*;
use async_trait::async_trait;
use lattice_core::application::ports::ImportedBlob;
use lattice_core::application::ports::{ContentAddressedStoragePort, EmbeddingRepositoryPort};
use lattice_core::domain::entities::Document;
use lattice_core::domain::entities::Embedding;
use sqlx::SqlitePool;
use std::path::Path;

/// Library that indexes files where they already are: every source path
/// is its own "blob", so tests exercise the pipeline without copying.
struct MockContentStorage;

#[async_trait]
impl ContentAddressedStoragePort for MockContentStorage {
    async fn import_file(&self, source_path: &Path) -> Result<ImportedBlob> {
        let hash = "a".repeat(64);
        Ok(ImportedBlob {
            path: source_path.to_path_buf(),
            lease: lattice_core::application::ports::BlobLease::detached(&hash),
            hash,
            created: true,
        })
    }

    async fn exists_by_hash(&self, _hash: &str) -> Result<bool> {
        Ok(false)
    }

    async fn get_path_by_hash(&self, _hash: &str) -> Result<Option<std::path::PathBuf>> {
        Ok(None)
    }

    fn owns(&self, _path: &Path) -> bool {
        false
    }

    async fn list_hashes(&self) -> Result<Vec<String>> {
        Ok(Vec::new())
    }

    async fn remove_if_unreferenced(
        &self,
        _hash: &str,
        _refs: &dyn lattice_core::application::ports::BlobReferenceCheck,
    ) -> Result<lattice_core::application::ports::BlobRemoval> {
        Ok(lattice_core::application::ports::BlobRemoval::Retained(
            lattice_core::application::ports::RetainReason::Missing,
        ))
    }
}

struct MockFileStorage;

#[async_trait]
impl FileStoragePort for MockFileStorage {
    async fn read_file(&self, _path: &Path) -> Result<String> {
        Ok("This is test file content for indexing and chunking.".to_string())
    }

    async fn write_file(&self, _path: &Path, _content: &str) -> Result<()> {
        Ok(())
    }

    async fn delete_file(&self, _path: &Path) -> Result<()> {
        Ok(())
    }

    async fn read_file_bytes(&self, _path: &Path) -> Result<Vec<u8>> {
        Ok(b"Test content".to_vec())
    }

    async fn write_file_bytes(&self, _path: &Path, _content: &[u8]) -> Result<()> {
        Ok(())
    }

    async fn compute_hash(&self, _path: &Path) -> Result<String> {
        Ok("a".repeat(64))
    }

    async fn exists(&self, _path: &Path) -> bool {
        true
    }

    async fn metadata(
        &self,
        _path: &Path,
    ) -> Result<lattice_core::application::ports::file_storage_port::FileMetadata> {
        use chrono::Utc;
        use lattice_core::application::ports::file_storage_port::FileMetadata;
        Ok(FileMetadata {
            size: 100,
            modified_at: Utc::now().timestamp(),
            is_file: true,
            is_directory: false,
        })
    }
}

struct MockContentExtractor;

#[async_trait]
impl ContentExtractionPort for MockContentExtractor {
    async fn extract_content(
        &self,
        path: &Path,
    ) -> Result<lattice_core::application::ports::ExtractedContentData> {
        use lattice_core::application::ports::ExtractedContentData;
        let text = std::fs::read_to_string(path)
            .unwrap_or_else(|_| "This is test file content for indexing and chunking.".to_string());
        let char_count = text.chars().count();
        let word_count = text.split_whitespace().count();

        Ok(ExtractedContentData {
            text,
            mime_type: "text/plain".to_string(),
            page_count: None,
            page_ranges: Vec::new(),
            ocr_pages: Vec::new(),
            word_count,
            char_count,
        })
    }

    fn is_supported(&self, _path: &Path) -> bool {
        true
    }
}

struct MockEmbedder;

#[async_trait]
impl EmbeddingPort for MockEmbedder {
    async fn embed_single(&self, _text: &str) -> Result<Vec<f32>> {
        Ok(vec![0.1, 0.2, 0.3])
    }

    async fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        Ok(texts.iter().map(|_| vec![0.1, 0.2, 0.3]).collect())
    }

    fn dimension(&self) -> usize {
        3
    }

    async fn is_ready(&self) -> Result<bool> {
        Ok(true)
    }
}

/// Embedder with a real input budget: it splits a passage into fixed-width
/// windows. `embedding_input::prepare_structured` rebuilds the chunk list
/// from `EmbeddingPort::split_text`, so this is what actually decides how
/// many chunks an indexed document ends up with.
struct MockSplittingEmbedder {
    window: usize,
}

#[async_trait]
impl EmbeddingPort for MockSplittingEmbedder {
    async fn embed_single(&self, _text: &str) -> Result<Vec<f32>> {
        Ok(vec![0.1, 0.2, 0.3])
    }

    async fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        Ok(texts.iter().map(|_| vec![0.1, 0.2, 0.3]).collect())
    }

    fn split_text(
        &self,
        text: &str,
        _prefix: &str,
    ) -> Result<Vec<lattice_core::application::ports::embedding_port::EmbeddingTextChunk>> {
        use lattice_core::application::ports::embedding_port::EmbeddingTextChunk;

        let mut parts = Vec::new();
        let mut start = 0usize;
        while start < text.len() {
            let mut end = (start + self.window).min(text.len());
            while !text.is_char_boundary(end) {
                end -= 1;
            }
            parts.push(EmbeddingTextChunk {
                text: text[start..end].to_string(),
                start,
                end,
                // Keeps the context-prefix budget loop in `prepare_structured`
                // from shrinking the prefix.
                token_count: 0,
            });
            start = end;
        }
        Ok(parts)
    }

    fn dimension(&self) -> usize {
        3
    }

    async fn is_ready(&self) -> Result<bool> {
        Ok(true)
    }
}

struct MockEmbeddingRepo;

#[async_trait]
impl EmbeddingRepositoryPort for MockEmbeddingRepo {
    async fn vectors_by_document_ids(
        &self,
        _document_ids: &[String],
    ) -> Result<lattice_core::application::ports::embedding_repository_port::DocumentVectors> {
        Ok(Default::default())
    }

    async fn create(&self, _chunk_id: &str, _vector: &[f32], _model: &str) -> Result<String> {
        unimplemented!()
    }

    async fn find_by_chunk(&self, _chunk_id: &str) -> Result<Option<Embedding>> {
        unimplemented!()
    }

    async fn save(&self, _entity: &Embedding, _vector: Vec<f32>) -> Result<()> {
        Ok(())
    }

    async fn save_batch(&self, _entries: Vec<(Embedding, Vec<f32>)>) -> Result<()> {
        Ok(())
    }

    async fn find_by_chunk_id(&self, _chunk_id: &str) -> Result<Option<(Embedding, Vec<f32>)>> {
        Ok(None)
    }

    async fn find_by_document_id(&self, _document_id: &str) -> Result<Vec<(Embedding, Vec<f32>)>> {
        Ok(vec![])
    }

    async fn delete_by_chunk_id(&self, _chunk_id: &str) -> Result<()> {
        Ok(())
    }

    async fn delete_by_document_id(&self, _document_id: &str) -> Result<()> {
        Ok(())
    }

    async fn count(&self) -> Result<i64> {
        Ok(0)
    }

    async fn create_batch(&self, _entries: Vec<(Embedding, Vec<f32>)>) -> Result<Vec<String>> {
        Ok(vec![])
    }
}

struct MockDocumentRepo;

#[async_trait]
impl DocumentRepositoryPort for MockDocumentRepo {
    async fn find_by_checksum(
        &self,
        _checksum: &lattice_core::domain::value_objects::Checksum,
    ) -> Result<Option<Document>> {
        Ok(None)
    }

    async fn count_documents(&self) -> Result<i64> {
        Ok(0)
    }

    async fn count_chunks(&self) -> Result<i64> {
        Ok(0)
    }

    async fn find_file_path_by_id(&self, _document_id: &str) -> Result<String> {
        Ok(String::new())
    }

    async fn find_id_by_path(&self, _file_path: &str) -> Result<Option<String>> {
        Ok(None)
    }

    async fn document_exists(&self, _document_id: &str) -> Result<bool> {
        Ok(false)
    }

    async fn delete(&self, _document_id: &str) -> Result<()> {
        Ok(())
    }

    async fn delete_and_release_blob(&self, document_id: &str) -> Result<()> {
        <Self as DocumentRepositoryPort>::delete(self, document_id).await
    }

    async fn find_all_paginated(&self, _limit: usize) -> Result<Vec<Document>> {
        Ok(vec![])
    }
}

#[async_trait]
impl lattice_core::application::ports::RepositoryPort<Document> for MockDocumentRepo {
    async fn save(&self, _entity: &Document) -> Result<()> {
        Ok(())
    }

    async fn find_by_id(&self, _id: &str) -> Result<Option<Document>> {
        Ok(None)
    }

    async fn find_all(&self) -> Result<Vec<Document>> {
        Ok(vec![])
    }

    async fn exists(&self, _id: &str) -> Result<bool> {
        Ok(false)
    }

    async fn find_by_filter(
        &self,
        _filter: &dyn lattice_core::application::ports::repository_port::Filter,
    ) -> Result<Vec<Document>> {
        Ok(vec![])
    }

    async fn save_batch(&self, _entities: &[Document]) -> Result<()> {
        Ok(())
    }

    async fn delete(&self, _id: &str) -> Result<()> {
        Ok(())
    }

    async fn delete_batch(&self, _ids: &[&str]) -> Result<()> {
        Ok(())
    }

    async fn count(&self) -> Result<usize> {
        Ok(0)
    }
}

async fn test_pool() -> SqlitePool {
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    sqlx::migrate!("./migrations").run(&pool).await.unwrap();
    pool
}

async fn count(pool: &SqlitePool, table: &str) -> i64 {
    sqlx::query_scalar(&format!("SELECT COUNT(*) FROM {table}"))
        .fetch_one(pool)
        .await
        .unwrap()
}

#[tokio::test]
async fn test_index_file_execution() {
    use std::fs;
    use tempfile::TempDir;

    let temp_dir = TempDir::new().unwrap();
    let file_path = temp_dir.path().join("test.txt");
    fs::write(&file_path, "Test content").unwrap();

    let pool = test_pool().await;
    let use_case = IndexFileUseCase::new(
        Arc::new(MockContentStorage),
        Arc::new(MockFileStorage),
        Arc::new(MockContentExtractor),
        Arc::new(MockEmbedder),
        Arc::new(MockDocumentRepo),
        Arc::new(MockEmbeddingRepo),
        Database::from(pool.clone()),
    );

    let request = IndexFileRequestDto {
        path: file_path.to_str().unwrap().to_string(),
        chunking_strategy: crate::features::indexing::dto::ChunkingStrategyDto::FixedSize {
            size: 512,
        },
        tags: None,
        metadata: None,
        space_id: None,
    };

    let response = use_case.execute(request).await.unwrap();

    assert!(!response.document_id.is_empty());
    // `prepare_structured` discards the strategy-produced chunks and rebuilds
    // them from `EmbeddingPort::split_text`. `MockEmbedder` keeps the port's
    // default, which returns the whole passage as a single part, and this
    // short unstructured file is one structure span, so exactly one chunk.
    assert_eq!(response.chunks_created, 1);
    assert_eq!(response.status, "imported");
    assert!(response.error.is_none());
    assert!(!response.file_path.is_empty());
    assert_eq!(count(&pool, "documents").await, 1);
    assert_eq!(count(&pool, "text_chunks").await, 1);
    assert_eq!(count(&pool, "text_embeddings").await, 1);
}

#[tokio::test]
async fn test_index_file_with_chunking() {
    use std::fs;
    use tempfile::TempDir;

    let temp_dir = TempDir::new().unwrap();
    let file_path = temp_dir.path().join("long.txt");
    fs::write(&file_path, "A".repeat(1000)).unwrap();

    let pool = test_pool().await;
    let use_case = IndexFileUseCase::new(
        Arc::new(MockContentStorage),
        Arc::new(MockFileStorage),
        Arc::new(MockContentExtractor),
        Arc::new(MockSplittingEmbedder { window: 100 }),
        Arc::new(MockDocumentRepo),
        Arc::new(MockEmbeddingRepo),
        Database::from(pool.clone()),
    );

    let request = IndexFileRequestDto {
        path: file_path.to_str().unwrap().to_string(),
        chunking_strategy: crate::features::indexing::dto::ChunkingStrategyDto::FixedSize {
            size: 100,
        },
        tags: None,
        metadata: None,
        space_id: None,
    };

    let response = use_case.execute(request).await.unwrap();

    // The 1000-byte file has no headings, so it is one structure span that the
    // embedder splits into 1000 / 100 windows.
    assert_eq!(response.chunks_created, 10);
    assert_eq!(count(&pool, "text_chunks").await, 10);
    assert_eq!(count(&pool, "text_embeddings").await, 10);
}

#[tokio::test]
async fn prepare_for_indexing_records_library_blob_hash_when_source_mutates_during_copy() {
    use std::fs;
    use tempfile::TempDir;

    struct MutatingContentStorage;

    #[async_trait]
    impl ContentAddressedStoragePort for MutatingContentStorage {
        async fn import_file(&self, source_path: &Path) -> Result<ImportedBlob> {
            let hash = "b".repeat(64);
            Ok(ImportedBlob {
                path: source_path.to_path_buf(),
                lease: lattice_core::application::ports::BlobLease::detached(&hash),
                hash,
                created: true,
            })
        }

        async fn exists_by_hash(&self, _hash: &str) -> Result<bool> {
            Ok(false)
        }

        async fn get_path_by_hash(&self, _hash: &str) -> Result<Option<std::path::PathBuf>> {
            Ok(None)
        }

        fn owns(&self, _path: &Path) -> bool {
            false
        }

        async fn list_hashes(&self) -> Result<Vec<String>> {
            Ok(Vec::new())
        }

        async fn remove_if_unreferenced(
            &self,
            _hash: &str,
            _refs: &dyn lattice_core::application::ports::BlobReferenceCheck,
        ) -> Result<lattice_core::application::ports::BlobRemoval> {
            Ok(lattice_core::application::ports::BlobRemoval::Retained(
                lattice_core::application::ports::RetainReason::Missing,
            ))
        }
    }

    let temp_dir = TempDir::new().unwrap();
    let file_path = temp_dir.path().join("watched.txt");
    fs::write(&file_path, "Watched content").unwrap();

    let use_case = IndexFileUseCase::new(
        Arc::new(MutatingContentStorage),
        Arc::new(MockFileStorage),
        Arc::new(MockContentExtractor),
        Arc::new(MockEmbedder),
        Arc::new(MockDocumentRepo),
        Arc::new(MockEmbeddingRepo),
        Database::from(test_pool().await),
    );

    let outcome = use_case
        .prepare_for_indexing(IndexFileRequestDto {
            path: file_path.to_str().unwrap().to_string(),
            chunking_strategy: crate::features::indexing::dto::ChunkingStrategyDto::FixedSize {
                size: 512,
            },
            tags: None,
            metadata: None,
            space_id: None,
        })
        .await
        .unwrap();

    let PrepareForIndexingOutcome::Prepared(prepared) = outcome else {
        panic!("expected Prepared outcome");
    };
    assert_eq!(prepared.document.checksum().as_str(), &"b".repeat(64));
}

/// Files a document under one source group, then imports the same file under
/// another after an editor saved it between the import's two hashes. Returns
/// what is wrong with the library once that import has committed and a
/// startup sweep has run.
async fn reimport_of_a_file_edited_during_the_copy(edited_content_indexed: bool) -> Vec<String> {
    use crate::features::indexing::library_gc::test_support::file_intents;
    use crate::infrastructure::persistence::repositories::mocks::MockDocumentRepository;
    use crate::infrastructure::storage::ContentAddressedStorage;
    use crate::infrastructure::storage::{FileStore, IntentKind};
    use lattice_core::application::ports::RepositoryPort;
    use lattice_core::domain::value_objects::source_context::{
        SourceContext, SourceGroup, StructureMode,
    };
    use lattice_core::domain::value_objects::{Checksum, ChunkingStrategy, FileMetadata};
    use lattice_core::shared::fs::SweepGate;
    use lattice_core::shared::types::ValidatedFilePath;
    use std::fs;
    use tempfile::TempDir;

    fn group(id: &str, title: &str) -> SourceContext {
        SourceContext {
            group: SourceGroup {
                id: id.to_string(),
                title: title.to_string(),
                edition: None,
                description: None,
                ordered: false,
                structure: StructureMode::default(),
            },
            position: 0,
        }
    }

    fn indexed(blob: &Path, hash: &str, text: &str, context: Option<SourceContext>) -> Document {
        let metadata = FileMetadata::new(
            "watched.txt".to_string(),
            "text/plain".to_string(),
            text.len() as i64,
            chrono::Utc::now(),
        )
        .unwrap();
        let mut document = Document::from_file(
            ValidatedFilePath::new(blob.to_path_buf()).unwrap(),
            metadata,
            Checksum::new(hash.to_string()).unwrap(),
            text.to_string(),
            ChunkingStrategy::FixedSize { size: 64 },
        )
        .unwrap();
        document.set_source_context(context);
        document.mark_indexed();
        document
    }

    fn blob_folder(document: &Document) -> Option<&str> {
        document.file_path().parent()?.file_name()?.to_str()
    }

    let temp = TempDir::new().unwrap();
    let library_root = temp.path().join("lib");
    let storage = Arc::new(ContentAddressedStorage::with_root(library_root.clone()));
    let repo = Arc::new(MockDocumentRepository::new());
    let first_group = group("11111111-1111-4111-8111-111111111111", "First group");
    let second_group = group("22222222-2222-4222-8222-222222222222", "Second group");

    // `MockFileStorage` hashes every file to this, as the first import of the
    // file's content did, so that import's blob sits in this folder.
    let first_hash = "a".repeat(64);
    let first_text = "The first version of the file.";
    let first_blob = library_root.join(&first_hash).join("watched.txt");
    fs::create_dir_all(first_blob.parent().unwrap()).unwrap();
    fs::write(&first_blob, first_text).unwrap();
    repo.add_document(indexed(
        &first_blob,
        &first_hash,
        first_text,
        Some(first_group.clone()),
    ));

    let edited_text = "The second version, saved while the import ran.";
    let source = temp.path().join("watched.txt");
    fs::write(&source, edited_text).unwrap();
    let edited_hash = crate::infrastructure::file_system::compute_file_sha256_async(&source)
        .await
        .unwrap();
    if edited_content_indexed {
        let blob = storage.import_file(&source).await.unwrap();
        repo.add_document(indexed(&blob.path, &blob.hash, edited_text, None));
    }

    let use_case = IndexFileUseCase::new(
        storage.clone(),
        Arc::new(MockFileStorage),
        Arc::new(MockContentExtractor),
        Arc::new(MockEmbedder),
        repo.clone(),
        Arc::new(MockEmbeddingRepo),
        Database::from(test_pool().await),
    );
    let request = IndexFileRequestDto {
        path: source.to_str().unwrap().to_string(),
        chunking_strategy: crate::features::indexing::dto::ChunkingStrategyDto::FixedSize {
            size: 512,
        },
        tags: None,
        metadata: Some(std::collections::HashMap::from([(
            "source_context".to_string(),
            serde_json::to_string(&second_group).unwrap(),
        )])),
        space_id: None,
    };
    let PrepareForIndexingOutcome::Prepared(prepared) =
        use_case.prepare_for_indexing(request).await.unwrap()
    else {
        panic!("a file filed under another group is prepared, not a duplicate");
    };

    // The commit's only write is this save of the prepared document.
    let PreparedDocument {
        document, lease, ..
    } = *prepared;
    repo.save(&document).await.unwrap();
    drop(lease);
    let gate = SweepGate::default();
    gate.open();
    let (_pool, intents) = file_intents().await;
    for hash in [&first_hash, &edited_hash] {
        intents
            .record(FileStore::Library, IntentKind::Release, hash)
            .await
            .unwrap();
    }
    LibraryGc::new(storage, repo.clone(), Arc::new(gate))
        .with_intents(intents.clone())
        .sweep()
        .await
        .unwrap();

    let rows = repo.find_all().await.unwrap();
    let mut problems = Vec::new();
    if !intents
        .pending(FileStore::Library)
        .await
        .unwrap()
        .is_empty()
    {
        problems
            .push("a release record is still pending, though every blob is referenced".to_string());
    }
    for row in &rows {
        if blob_folder(row) != Some(row.checksum().as_str()) {
            problems.push(format!(
                "{} has checksum {} but its file is {}",
                row.id().as_str(),
                row.checksum(),
                row.file_path().display()
            ));
        }
        if !row.file_path().exists() {
            problems.push(format!(
                "{} points at {}, which is gone after the startup sweep",
                row.id().as_str(),
                row.file_path().display()
            ));
        }
    }
    match rows
        .iter()
        .find(|row| row.checksum().as_str() == first_hash)
    {
        Some(row) if row.source_context() == Some(&first_group) => {}
        Some(_) => problems.push("the first document was filed under another group".to_string()),
        None => problems.push(format!(
            "no document references {first_hash}, the blob of the first import"
        )),
    }
    let edited: Vec<_> = rows
        .iter()
        .filter(|row| row.checksum().as_str() != first_hash)
        .collect();
    match edited.as_slice() {
        [row] if row.source_context() == Some(&second_group) => {}
        _ => problems.push(format!(
            "{} documents hold the edited content; expected one, filed under the second group",
            edited.len()
        )),
    }
    problems
}

#[tokio::test]
async fn reimport_of_a_source_edited_during_the_copy_keeps_the_existing_document_on_its_blob() {
    let problems = reimport_of_a_file_edited_during_the_copy(false).await;

    assert!(problems.is_empty(), "{problems:#?}");
}

#[tokio::test]
async fn reimport_of_a_source_edited_during_the_copy_reuses_the_document_of_the_edited_content() {
    let problems = reimport_of_a_file_edited_during_the_copy(true).await;

    assert!(problems.is_empty(), "{problems:#?}");
}

#[tokio::test]
async fn prepare_for_indexing_preserves_tokenizer_counts_code_flags_and_avoids_fake_code_sections()
{
    use std::fs;
    use tempfile::TempDir;

    struct TokenizerCountEmbedder {
        token_count: usize,
    }

    #[async_trait]
    impl EmbeddingPort for TokenizerCountEmbedder {
        async fn embed_single(&self, _text: &str) -> Result<Vec<f32>> {
            Ok(vec![0.1, 0.2, 0.3])
        }

        async fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
            Ok(texts.iter().map(|_| vec![0.1, 0.2, 0.3]).collect())
        }

        fn split_text(
            &self,
            passage: &str,
            _prefix: &str,
        ) -> Result<Vec<lattice_core::application::ports::embedding_port::EmbeddingTextChunk>>
        {
            Ok(vec![
                lattice_core::application::ports::embedding_port::EmbeddingTextChunk {
                    start: 0,
                    end: passage.len(),
                    text: passage.to_string(),
                    token_count: self.token_count,
                },
            ])
        }

        fn dimension(&self) -> usize {
            3
        }

        async fn is_ready(&self) -> Result<bool> {
            Ok(true)
        }
    }

    let temp_dir = TempDir::new().unwrap();
    let use_case = IndexFileUseCase::new(
        Arc::new(MockContentStorage),
        Arc::new(MockFileStorage),
        Arc::new(MockContentExtractor),
        Arc::new(TokenizerCountEmbedder { token_count: 14 }),
        Arc::new(MockDocumentRepo),
        Arc::new(MockEmbeddingRepo),
        Database::from(test_pool().await),
    );

    // 1. C code with #include must not get a fake section label "include <stdio.h>"
    let c_path = temp_dir.path().join("main.c");
    fs::write(
        &c_path,
        "#include <stdio.h>\nint main(void) { return 0; }\n",
    )
    .unwrap();
    let PrepareForIndexingOutcome::Prepared(c_prepared) = use_case
        .prepare_for_indexing(IndexFileRequestDto {
            path: c_path.to_str().unwrap().to_string(),
            chunking_strategy: crate::features::indexing::dto::ChunkingStrategyDto::FixedSize {
                size: 512,
            },
            tags: None,
            metadata: None,
            space_id: None,
        })
        .await
        .unwrap()
    else {
        panic!("expected Prepared outcome for C file");
    };
    assert_eq!(c_prepared.document.chunks()[0].section(), None);
    assert!(c_prepared.document.chunks()[0].has_code());

    // 2. YAML file keeps has_code == true and canonical mime_type
    let yaml_path = temp_dir.path().join("config.yaml");
    fs::write(&yaml_path, "server:\n  host: localhost\n  port: 8080\n").unwrap();
    let PrepareForIndexingOutcome::Prepared(yaml_prepared) = use_case
        .prepare_for_indexing(IndexFileRequestDto {
            path: yaml_path.to_str().unwrap().to_string(),
            chunking_strategy: crate::features::indexing::dto::ChunkingStrategyDto::FixedSize {
                size: 512,
            },
            tags: None,
            metadata: None,
            space_id: None,
        })
        .await
        .unwrap()
    else {
        panic!("expected Prepared outcome for YAML file");
    };
    assert_eq!(yaml_prepared.document.mime_type(), "text/x-yaml");
    assert!(yaml_prepared.document.chunks()[0].has_code());

    // 3. CJK text with no spaces keeps the tokenizer's token_count (14, not 1)
    let cjk_path = temp_dir.path().join("notes.txt");
    fs::write(&cjk_path, "人工智能正在改变软件工程的未来").unwrap();
    let PrepareForIndexingOutcome::Prepared(cjk_prepared) = use_case
        .prepare_for_indexing(IndexFileRequestDto {
            path: cjk_path.to_str().unwrap().to_string(),
            chunking_strategy: crate::features::indexing::dto::ChunkingStrategyDto::FixedSize {
                size: 512,
            },
            tags: None,
            metadata: None,
            space_id: None,
        })
        .await
        .unwrap()
    else {
        panic!("expected Prepared outcome for CJK file");
    };
    assert_eq!(cjk_prepared.document.chunks()[0].token_count(), 14);
}
