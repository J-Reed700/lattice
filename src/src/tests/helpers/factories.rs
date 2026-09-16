#![allow(clippy::panic)]
#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
#![allow(clippy::assertions_on_constants)]
#![allow(clippy::useless_vec)]
#![allow(clippy::indexing_slicing)]
#![allow(unused_variables)]
#![allow(unused_imports)]

//! # Test Data Factories
#![allow(clippy::panic)]
#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
#![allow(clippy::assertions_on_constants)]
#![allow(clippy::useless_vec)]
#![allow(unused_variables)]
#![allow(unused_imports)]

//!
//! Factory methods for creating test data with sensible defaults.
//!
//! ## Design Pattern
//!
//! Uses the Builder pattern to allow flexible test data creation:
//! - Sensible defaults for all fields
//! - Override specific fields as needed
//! - Type-safe construction
//!
//! ## Examples
//!
//! ```rust
//! // Use defaults
//! let doc = DocumentFactory::new().build();
//!
//! // Customize specific fields
//! let doc = DocumentFactory::new()
//!     .file_name("custom.md")
//!     .content("Custom content")
//!     .build();
//! ```

use chrono::Utc;
use uuid::Uuid;
use lattice::shared::error::Result;
use lattice::infrastructure::persistence::repositories::{
    chunk_repository::ChunkRepository,
    document_repository::DocumentRepository,
    embedding_repository::EmbeddingRepository,
};
use sqlx;

use super::{TestDocument, TestChunk, TestEmbedding};

/// Factory for creating test documents.
///
/// # Examples
///
/// ```rust
/// let doc = DocumentFactory::new()
///     .file_name("test.md")
///     .file_type("markdown")
///     .content("Test content")
///     .build();
/// ```
#[derive(Debug, Clone)]
pub struct DocumentFactory {
    id: String,
    vault_id: String,
    file_path: String,
    file_name: String,
    file_type: String,
    file_size: i64,
    content: String,
    indexed_at: String,
}

impl DocumentFactory {
    pub fn new() -> Self {
        let id = Uuid::new_v4().to_string();
        let file_name = format!("test-doc-{}.md", &id[..8]);

        Self {
            id: id.clone(),
            vault_id: "test-lattice".to_string(),
            file_path: format!("/test/documents/{}", file_name),
            file_name,
            file_type: "markdown".to_string(),
            file_size: 1000,
            content: "Default test content".to_string(),
            indexed_at: Utc::now().to_rfc3339(),
        }
    }

    pub fn id(mut self, id: &str) -> Self {
        self.id = id.to_string();
        self
    }

    pub fn vault_id(mut self, vault_id: &str) -> Self {
        self.vault_id = vault_id.to_string();
        self
    }

    pub fn file_path(mut self, path: &str) -> Self {
        self.file_path = path.to_string();
        self
    }

    pub fn file_name(mut self, name: &str) -> Self {
        self.file_name = name.to_string();
        self
    }

    pub fn file_type(mut self, file_type: &str) -> Self {
        self.file_type = file_type.to_string();
        self
    }

    pub fn file_size(mut self, size: i64) -> Self {
        self.file_size = size;
        self
    }

    pub fn content(mut self, content: &str) -> Self {
        self.content = content.to_string();
        self.file_size = content.len() as i64;
        self
    }

    pub fn build(self) -> TestDocument {
        TestDocument {
            id: self.id,
            vault_id: self.vault_id,
            file_path: self.file_path,
            file_name: self.file_name,
            file_type: self.file_type,
            file_size: self.file_size,
            content: self.content,
            indexed_at: self.indexed_at,
        }
    }
}

impl Default for DocumentFactory {
    fn default() -> Self {
        Self::new()
    }
}

impl TestDocument {
    /// Save test document to database - NO-OP for now
    ///
    /// Due to domain complexity, test documents are created differently.
    /// This method is kept for interface compatibility.
    pub async fn insert_into_db(&self, _repo: &DocumentRepository) -> Result<()> {
        // NO-OP: Test documents don't use repository pattern
        // See TestContext::create_test_document for actual implementation
        Ok(())
    }
}

/// Factory for creating test chunks.
///
/// # Examples
///
/// ```rust
/// let chunk = ChunkFactory::new()
///     .document_id("doc-123")
///     .content("Chunk content")
///     .chunk_index(0)
///     .build();
/// ```
#[derive(Debug, Clone)]
pub struct ChunkFactory {
    id: String,
    document_id: String,
    content: String,
    chunk_index: i32,
    start_char: Option<i32>,
    end_char: Option<i32>,
}

impl ChunkFactory {
    pub fn new() -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            document_id: Uuid::new_v4().to_string(),
            content: "Default chunk content".to_string(),
            chunk_index: 0,
            start_char: Some(0),
            end_char: Some(100),
        }
    }

    pub fn id(mut self, id: &str) -> Self {
        self.id = id.to_string();
        self
    }

    pub fn document_id(mut self, doc_id: &str) -> Self {
        self.document_id = doc_id.to_string();
        self
    }

    pub fn content(mut self, content: &str) -> Self {
        self.content = content.to_string();
        if let Some(start) = self.start_char {
            self.end_char = Some(start + content.len() as i32);
        }
        self
    }

    pub fn chunk_index(mut self, index: i32) -> Self {
        self.chunk_index = index;
        self
    }

    pub fn start_char(mut self, start: i32) -> Self {
        self.start_char = Some(start);
        self
    }

    pub fn end_char(mut self, end: i32) -> Self {
        self.end_char = Some(end);
        self
    }

    pub fn build(self) -> TestChunk {
        TestChunk {
            id: self.id,
            document_id: self.document_id,
            content: self.content,
            chunk_index: self.chunk_index,
            start_char: self.start_char,
            end_char: self.end_char,
        }
    }
}

impl Default for ChunkFactory {
    fn default() -> Self {
        Self::new()
    }
}

impl TestChunk {
    /// Insert this test chunk into the database.
    pub async fn insert_into_db(&self, repo: &ChunkRepository) -> Result<String> {
        use lattice::application::ports::chunk_repository_port::ChunkRepositoryPort;

        let chunk = repo
            .create(
                &self.document_id,
                &self.content,
                None, // context_prefix
                None, // contextualized_content
                self.chunk_index as usize,
                self.start_char.map(|x| x as i64),
                self.end_char.map(|x| x as i64),
            )
            .await?;

        Ok(chunk.id().to_string())
    }
}

/// Factory for creating test embeddings.
///
/// # Examples
///
/// ```rust
/// let embedding = EmbeddingFactory::new()
///     .chunk_id("chunk-123")
///     .embedding(vec![0.1, 0.2, 0.3])
///     .build();
/// ```
#[derive(Debug, Clone)]
pub struct EmbeddingFactory {
    chunk_id: String,
    embedding: Vec<f32>,
}

impl EmbeddingFactory {
    pub fn new() -> Self {
        Self {
            chunk_id: Uuid::new_v4().to_string(),
            embedding: vec![0.0; 384], // Default dimension
        }
    }

    pub fn chunk_id(mut self, chunk_id: &str) -> Self {
        self.chunk_id = chunk_id.to_string();
        self
    }

    pub fn embedding(mut self, embedding: Vec<f32>) -> Self {
        self.embedding = embedding;
        self
    }

    pub fn random_embedding(mut self, dimensions: usize) -> Self {
        use rand::Rng;
        let mut rng = rand::thread_rng();
        self.embedding = (0..dimensions).map(|_| rng.gen_range(-1.0..1.0)).collect();
        self
    }

    pub fn build(self) -> TestEmbedding {
        TestEmbedding {
            chunk_id: self.chunk_id,
            embedding: self.embedding,
        }
    }
}

impl Default for EmbeddingFactory {
    fn default() -> Self {
        Self::new()
    }
}

impl TestEmbedding {
    /// Insert this test embedding into the database.
    pub async fn insert_into_db(&self, repo: &EmbeddingRepository) -> Result<()> {
        use lattice::application::ports::embedding_repository_port::EmbeddingRepositoryPort;

        repo.create(&self.chunk_id, &self.embedding, "test-model").await?;
        Ok(())
    }
}

/// Create multiple test documents with related data.
///
/// # Arguments
///
/// * `count` - Number of documents to create
///
/// # Returns
///
/// Vector of test documents
pub fn create_test_document_batch(count: usize) -> Vec<TestDocument> {
    (0..count)
        .map(|i| {
            DocumentFactory::new()
                .file_name(&format!("doc-{}.md", i))
                .content(&format!("Content for document {}", i))
                .build()
        })
        .collect()
}

/// Create test chunks for a document.
///
/// # Arguments
///
/// * `doc_id` - Document ID
/// * `count` - Number of chunks
///
/// # Returns
///
/// Vector of test chunks
pub fn create_test_chunk_batch(doc_id: &str, count: usize) -> Vec<TestChunk> {
    (0..count)
        .map(|i| {
            ChunkFactory::new()
                .document_id(doc_id)
                .content(&format!("Chunk {} content", i))
                .chunk_index(i as i32)
                .start_char((i * 100) as i32)
                .end_char(((i + 1) * 100) as i32)
                .build()
        })
        .collect()
}

/// Create embeddings for chunks.
///
/// # Arguments
///
/// * `chunk_ids` - Chunk IDs
/// * `dimensions` - Embedding dimensions
///
/// # Returns
///
/// Vector of test embeddings
pub fn create_test_embedding_batch(chunk_ids: &[String], dimensions: usize) -> Vec<TestEmbedding> {
    chunk_ids
        .iter()
        .map(|chunk_id| {
            EmbeddingFactory::new()
                .chunk_id(chunk_id)
                .random_embedding(dimensions)
                .build()
        })
        .collect()
}

/// Generate markdown content with mentions and tags.
pub fn generate_markdown_with_mentions(person: &str, topic: &str, tags: &[&str]) -> String {
    let tag_str = tags.iter().map(|t| format!("#{}", t)).collect::<Vec<_>>().join(" ");

    format!(
        r#"# Meeting Notes

Discussion with @{} about [[{}]].

## Key Points

- Important updates on the project
- Next steps defined
- Follow-up scheduled

## Action Items

- [ ] Review documentation
- [ ] Update implementation
- [ ] Schedule follow-up

Tags: {}
"#,
        person, topic, tag_str
    )
}

/// Generate code snippet content.
pub fn generate_code_snippet(language: &str, complexity: &str) -> String {
    match (language, complexity) {
        ("rust", "simple") => {
            r#"fn calculate_sum(a: i32, b: i32) -> i32 {
    a + b
}

#[test]
fn test_sum() {
    assert_eq!(calculate_sum(2, 3), 5);
}
"#
            .to_string()
        }
        ("rust", "complex") => {
            r#"use std::sync::Arc;
use tokio::sync::Mutex;

pub struct DataProcessor {
    cache: Arc<Mutex<HashMap<String, Vec<f32>>>>,
}

impl DataProcessor {
    pub async fn process(&self, key: &str, data: &[f32]) -> Result<Vec<f32>> {
        let mut cache = self.cache.lock().await;

        if let Some(cached) = cache.get(key) {
            return Ok(cached.clone());
        }

        let processed = data.iter().map(|x| x * 2.0).collect();
        cache.insert(key.to_string(), processed.clone());

        Ok(processed)
    }
}
"#
            .to_string()
        }
        _ => format!("// {} code example\n", language),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_document_factory_defaults() {
        let doc = DocumentFactory::new().build();

        assert!(!doc.id.is_empty());
        assert_eq!(doc.vault_id, "test-lattice");
        assert_eq!(doc.file_type, "markdown");
    }

    #[test]
    fn test_document_factory_customization() {
        let doc = DocumentFactory::new()
            .file_name("custom.md")
            .content("Custom content")
            .build();

        assert_eq!(doc.file_name, "custom.md");
        assert_eq!(doc.content, "Custom content");
        assert_eq!(doc.file_size, "Custom content".len() as i64);
    }

    #[test]
    fn test_chunk_factory_defaults() {
        let chunk = ChunkFactory::new().build();

        assert!(!chunk.id.is_empty());
        assert!(!chunk.document_id.is_empty());
        assert_eq!(chunk.chunk_index, 0);
    }

    #[test]
    fn test_embedding_factory() {
        let embedding = EmbeddingFactory::new()
            .embedding(vec![1.0, 2.0, 3.0])
            .build();

        assert_eq!(embedding.embedding.len(), 3);
        assert_eq!(embedding.embedding[0], 1.0);
    }

    #[test]
    fn test_batch_creation() {
        let docs = create_test_document_batch(5);
        assert_eq!(docs.len(), 5);

        let chunks = create_test_chunk_batch("doc-123", 3);
        assert_eq!(chunks.len(), 3);
        assert_eq!(chunks[0].document_id, "doc-123");
    }

    #[test]
    fn test_content_generators() {
        let markdown = generate_markdown_with_mentions("alice", "project-x", &["important", "urgent"]);
        assert!(markdown.contains("@alice"));
        assert!(markdown.contains("[[project-x]]"));
        assert!(markdown.contains("#important"));

        let code = generate_code_snippet("rust", "simple");
        assert!(code.contains("fn calculate_sum"));
    }
}
