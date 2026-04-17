//! # Chunk Entity
//!
//! Text chunk entity within the domain layer.
//!
//! This is a pure domain entity with NO infrastructure concerns:
//! - No embeddings (handled by infrastructure layer)
//! - No database IDs (handled by repositories)
//! - Just the core business data and logic

use crate::domain::entities::document::Language;
use crate::shared::domain_types::{ChunkId, DocumentId};
use serde::{Deserialize, Serialize};

/// Parameters for creating a chunk with an existing ID.
#[derive(Debug, Clone)]
pub struct ChunkParams {
    pub id: ChunkId,
    pub document_id: DocumentId,
    pub content: String,
    pub index: usize,
    pub language: Language,
    pub token_count: i32,
    pub word_count: usize,
    pub has_code: bool,
    pub section: Option<String>,
}

/// Chunk entity (owned by document aggregate).
///
/// Represents a text chunk with its position in the document.
///
/// ## Pure Domain Model
///
/// This entity contains only business logic. Infrastructure concerns like
/// embeddings, database persistence, and search indexes are handled by
/// the infrastructure layer.
///
/// ## Example
///
/// ```rust,no_run
/// use vault_desktop::domain::entities::chunk::Chunk;
/// use vault_desktop::domain_types::DocumentId;
///
/// let doc_id = DocumentId::new();
/// let chunk = Chunk::new(doc_id, "This is chunk content".to_string(), 0);
///
/// assert_eq!(chunk.content(), "This is chunk content");
/// assert_eq!(chunk.index(), 0);
/// ```
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Chunk {
    id: ChunkId,
    document_id: DocumentId,
    content: String,
    index: usize,
    // Rich metadata fields (Phase 2)
    language: Language,
    token_count: i32,
    // NEW: Phase 1 metadata
    word_count: usize,
    has_code: bool,
    section: Option<String>,
}

impl Chunk {
    /// Create new chunk.
    ///
    /// # Arguments
    ///
    /// * `document_id` - ID of the parent document
    /// * `content` - Text content of the chunk
    /// * `index` - Position index within the document (0-based)
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use vault_desktop::domain::entities::chunk::Chunk;
    /// use vault_desktop::domain_types::DocumentId;
    ///
    /// let doc_id = DocumentId::new();
    /// let chunk = Chunk::new(doc_id.clone(), "Content here".to_string(), 0);
    /// assert_eq!(chunk.document_id(), &doc_id);
    /// ```
    pub fn new(document_id: DocumentId, content: String, index: usize) -> Self {
        Self {
            id: ChunkId::new(),
            document_id,
            content,
            index,
            language: Language::default(),
            token_count: 0,
            word_count: 0,
            has_code: false,
            section: None,
        }
    }

    /// Create chunk with existing ID (for reconstruction from storage).
    ///
    /// This is typically used by repositories when loading chunks from storage.
    pub fn with_id(params: ChunkParams) -> Self {
        Self {
            id: params.id,
            document_id: params.document_id,
            content: params.content,
            index: params.index,
            language: params.language,
            token_count: params.token_count,
            word_count: params.word_count,
            has_code: params.has_code,
            section: params.section,
        }
    }

    /// Get chunk ID.
    pub fn id(&self) -> &ChunkId {
        &self.id
    }

    /// Get document ID.
    pub fn document_id(&self) -> &DocumentId {
        &self.document_id
    }

    /// Get chunk content.
    pub fn content(&self) -> &str {
        &self.content
    }

    /// Get chunk index.
    ///
    /// The index represents the position of this chunk within its parent document.
    /// Chunks are ordered by index, starting from 0.
    pub fn index(&self) -> usize {
        self.index
    }

    /// Get chunk index (alias for index()).
    pub fn chunk_index(&self) -> usize {
        self.index
    }

    /// Get content length in characters.
    pub fn content_length(&self) -> usize {
        self.content.len()
    }

    /// Check if chunk content is empty.
    pub fn is_empty(&self) -> bool {
        self.content.is_empty()
    }

    /// Check if chunk content contains a given substring.
    ///
    /// This is a simple text search, case-sensitive.
    pub fn contains(&self, query: &str) -> bool {
        self.content.contains(query)
    }

    /// Check if chunk content contains a given substring (case-insensitive).
    pub fn contains_ignore_case(&self, query: &str) -> bool {
        self.content.to_lowercase().contains(&query.to_lowercase())
    }

    // ========================================================================
    // Rich Metadata Methods (Phase 2)
    // ========================================================================

    /// Get chunk language.
    pub fn language(&self) -> &Language {
        &self.language
    }

    /// Set chunk language.
    pub fn set_language(&mut self, language: Language) {
        self.language = language;
    }

    /// Get token count.
    pub fn token_count(&self) -> i32 {
        self.token_count
    }

    /// Set token count.
    pub fn set_token_count(&mut self, count: i32) {
        self.token_count = count.max(0);
    }

    /// Get chunk word count.
    pub fn word_count(&self) -> usize {
        self.word_count
    }

    /// Check if chunk contains code.
    pub fn has_code(&self) -> bool {
        self.has_code
    }

    /// Get chunk section/heading.
    pub fn section(&self) -> Option<&str> {
        self.section.as_deref()
    }

    /// Set chunk word count.
    pub fn set_word_count(&mut self, count: usize) {
        self.word_count = count;
    }

    /// Set whether chunk contains code.
    pub fn set_has_code(&mut self, has_code: bool) {
        self.has_code = has_code;
    }

    /// Set chunk section/heading.
    pub fn set_section(&mut self, section: Option<String>) {
        self.section = section;
    }

    /// Get chunk ID as string slice (for tests).
    pub fn as_str(&self) -> &str {
        self.id.as_str()
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chunk_creation() {
        let doc_id = DocumentId::new();
        let content = "Test chunk content".to_string();
        let chunk = Chunk::new(doc_id.clone(), content.clone(), 0);

        assert_eq!(chunk.document_id(), &doc_id);
        assert_eq!(chunk.content(), content);
        assert_eq!(chunk.index(), 0);
        assert!(!chunk.id().as_str().is_empty());
    }

    #[test]
    fn test_chunk_with_id() {
        let chunk_id = ChunkId::new();
        let doc_id = DocumentId::new();
        let chunk = Chunk::with_id(ChunkParams {
            id: chunk_id.clone(),
            document_id: doc_id.clone(),
            content: "Content".to_string(),
            index: 5,
            language: Language::default(),
            token_count: 0,
            word_count: 0,
            has_code: false,
            section: None,
        });

        assert_eq!(chunk.id(), &chunk_id);
        assert_eq!(chunk.document_id(), &doc_id);
        assert_eq!(chunk.index(), 5);
    }

    #[test]
    fn test_chunk_content_length() {
        let chunk = Chunk::new(DocumentId::new(), "Hello".to_string(), 0);
        assert_eq!(chunk.content_length(), 5);
    }

    #[test]
    fn test_chunk_is_empty() {
        let empty_chunk = Chunk::new(DocumentId::new(), "".to_string(), 0);
        assert!(empty_chunk.is_empty());

        let non_empty_chunk = Chunk::new(DocumentId::new(), "Content".to_string(), 0);
        assert!(!non_empty_chunk.is_empty());
    }

    #[test]
    fn test_chunk_contains() {
        let chunk = Chunk::new(DocumentId::new(), "The quick brown fox".to_string(), 0);

        assert!(chunk.contains("quick"));
        assert!(chunk.contains("fox"));
        assert!(!chunk.contains("dog"));
    }

    #[test]
    fn test_chunk_contains_ignore_case() {
        let chunk = Chunk::new(DocumentId::new(), "The Quick Brown Fox".to_string(), 0);

        assert!(chunk.contains_ignore_case("quick"));
        assert!(chunk.contains_ignore_case("QUICK"));
        assert!(chunk.contains_ignore_case("QuIcK"));
        assert!(!chunk.contains_ignore_case("dog"));
    }

    #[test]
    fn test_chunk_equality() {
        let id = ChunkId::new();
        let doc_id = DocumentId::new();

        let chunk1 = Chunk::with_id(ChunkParams {
            id: id.clone(),
            document_id: doc_id.clone(),
            content: "Same content".to_string(),
            index: 0,
            language: Language::default(),
            token_count: 0,
            word_count: 0,
            has_code: false,
            section: None,
        });
        let chunk2 = Chunk::with_id(ChunkParams {
            id: id.clone(),
            document_id: doc_id.clone(),
            content: "Same content".to_string(),
            index: 0,
            language: Language::default(),
            token_count: 0,
            word_count: 0,
            has_code: false,
            section: None,
        });

        assert_eq!(chunk1, chunk2);
    }

    #[test]
    fn test_chunk_serialization() {
        let chunk = Chunk::new(DocumentId::new(), "Test content".to_string(), 3);

        // Serialize
        let json = serde_json::to_string(&chunk).unwrap();

        // Deserialize
        let deserialized: Chunk = serde_json::from_str(&json).unwrap();

        assert_eq!(chunk, deserialized);
        assert_eq!(deserialized.content(), "Test content");
        assert_eq!(deserialized.index(), 3);
    }
}

// ============================================================================
// Property Tests
// ============================================================================

#[cfg(test)]
mod property_tests {
    use super::*;
    use proptest::prelude::*;

    // ========================================================================
    // Strategy Helpers
    // ========================================================================

    /// Strategy: Valid chunk content (10-1000 characters)
    fn valid_chunk_content() -> impl Strategy<Value = String> {
        prop::string::string_regex("[a-zA-Z0-9 \\n\\r\\t.,;:!?()\\[\\]{}\"']{10,1000}").unwrap()
    }

    /// Strategy: Non-empty string for metadata fields
    fn non_empty_string() -> impl Strategy<Value = String> {
        prop::string::string_regex("[a-zA-Z0-9_\\-\\.]{1,100}").unwrap()
    }

    /// Strategy: Valid token count (0-10000)
    fn valid_token_count() -> impl Strategy<Value = i32> {
        0i32..10_000i32
    }

    /// Strategy: Valid word count (0-10000)
    fn valid_word_count() -> impl Strategy<Value = usize> {
        0usize..10_000usize
    }

    /// Strategy: Arbitrary Chunk with all fields
    fn arbitrary_chunk() -> impl Strategy<Value = Chunk> {
        (
            valid_chunk_content(),
            0usize..1000usize, // index
            valid_token_count(),
            valid_word_count(),
            any::<bool>(),                        // has_code
            prop::option::of(non_empty_string()), // section
        )
            .prop_map(
                |(content, index, token_count, word_count, has_code, section)| {
                    let mut chunk = Chunk::new(DocumentId::new(), content, index);
                    chunk.set_token_count(token_count);
                    chunk.set_word_count(word_count);
                    chunk.set_has_code(has_code);
                    chunk.set_section(section);
                    chunk
                },
            )
    }

    // ========================================================================
    // Category A: Creation & Identity (4 tests)
    // ========================================================================

    proptest! {
        #[test]
        fn prop_chunk_has_unique_id(_dummy in 0..10u32) {
            let chunk1 = Chunk::new(
                DocumentId::new(),
                "test content 1".to_string(),
                0,
            );
            let chunk2 = Chunk::new(
                DocumentId::new(),
                "test content 2".to_string(),
                0,
            );

            prop_assert_ne!(chunk1.id(), chunk2.id());
        }

        #[test]
        fn prop_chunk_id_stable_across_clones(chunk in arbitrary_chunk()) {
            let cloned = chunk.clone();
            prop_assert_eq!(chunk.id(), cloned.id());
        }

        #[test]
        fn prop_chunk_document_id_preserved(chunk in arbitrary_chunk()) {
            let doc_id = chunk.document_id();
            let cloned = chunk.clone();
            prop_assert_eq!(doc_id, cloned.document_id());
        }

        #[test]
        fn prop_chunk_metadata_preserved_on_clone(chunk in arbitrary_chunk()) {
            let cloned = chunk.clone();
            prop_assert_eq!(chunk.token_count(), cloned.token_count());
            prop_assert_eq!(chunk.word_count(), cloned.word_count());
            prop_assert_eq!(chunk.has_code(), cloned.has_code());
            prop_assert_eq!(chunk.section(), cloned.section());
        }
    }

    // ========================================================================
    // Category B: Content Operations (4 tests)
    // ========================================================================

    proptest! {
        #[test]
        fn prop_chunk_content_non_empty(chunk in arbitrary_chunk()) {
            prop_assert!(!chunk.content().is_empty());
        }

        #[test]
        fn prop_chunk_content_length_consistent(content in valid_chunk_content()) {
            let expected_length = content.len();
            let chunk = Chunk::new(DocumentId::new(), content.clone(), 0);

            prop_assert_eq!(chunk.content_length(), expected_length);
            prop_assert_eq!(chunk.content().len(), expected_length);
        }

        #[test]
        fn prop_chunk_index_preserved(index in 0usize..1000usize) {
            let chunk = Chunk::new(
                DocumentId::new(),
                "test content".to_string(),
                index,
            );

            prop_assert_eq!(chunk.index(), index);
            prop_assert_eq!(chunk.chunk_index(), index); // Alias method
        }

        #[test]
        fn prop_chunk_contains_substring(content in valid_chunk_content()) {
            let chunk = Chunk::new(DocumentId::new(), content.clone(), 0);

            // Extract a substring from the content
            if content.len() >= 3 {
                let start = content.len() / 2;
                let end = start + 3.min(content.len() - start);
                let substring = &content[start..end];
                prop_assert!(chunk.contains(substring));
            }
        }
    }

    // ========================================================================
    // Category C: Rich Metadata (5 tests)
    // ========================================================================

    proptest! {
        #[test]
        fn prop_chunk_token_count_can_be_set(mut chunk in arbitrary_chunk(), count in valid_token_count()) {
            chunk.set_token_count(count);
            prop_assert_eq!(chunk.token_count(), count);
        }

        #[test]
        fn prop_chunk_token_count_never_negative(mut chunk in arbitrary_chunk()) {
            chunk.set_token_count(-100);
            prop_assert!(chunk.token_count() >= 0);
        }

        #[test]
        fn prop_chunk_word_count_can_be_set(mut chunk in arbitrary_chunk(), count in valid_word_count()) {
            chunk.set_word_count(count);
            prop_assert_eq!(chunk.word_count(), count);
        }

        #[test]
        fn prop_chunk_has_code_can_be_toggled(mut chunk in arbitrary_chunk(), has_code in any::<bool>()) {
            chunk.set_has_code(has_code);
            prop_assert_eq!(chunk.has_code(), has_code);
        }

        #[test]
        fn prop_chunk_section_can_be_set(mut chunk in arbitrary_chunk(), section in prop::option::of(non_empty_string())) {
            chunk.set_section(section.clone());
            prop_assert_eq!(chunk.section(), section.as_deref());
        }
    }

    // ========================================================================
    // Category D: Serialization & Equality (2 tests)
    // ========================================================================

    proptest! {
        #[test]
        fn prop_chunk_serialization_roundtrip(chunk in arbitrary_chunk()) {
            let json = serde_json::to_string(&chunk).unwrap();
            let deserialized: Chunk = serde_json::from_str(&json).unwrap();

            prop_assert_eq!(chunk.id(), deserialized.id());
            prop_assert_eq!(chunk.document_id(), deserialized.document_id());
            prop_assert_eq!(chunk.content(), deserialized.content());
            prop_assert_eq!(chunk.index(), deserialized.index());
            prop_assert_eq!(chunk.token_count(), deserialized.token_count());
            prop_assert_eq!(chunk.word_count(), deserialized.word_count());
            prop_assert_eq!(chunk.has_code(), deserialized.has_code());
            prop_assert_eq!(chunk.section(), deserialized.section());
        }

        #[test]
        fn prop_chunk_clone_creates_equal(chunk in arbitrary_chunk()) {
            let cloned = chunk.clone();

            prop_assert_eq!(chunk.id(), cloned.id());
            prop_assert_eq!(chunk.document_id(), cloned.document_id());
            prop_assert_eq!(chunk.content(), cloned.content());
            prop_assert_eq!(chunk.index(), cloned.index());
            prop_assert_eq!(chunk.token_count(), cloned.token_count());
            prop_assert_eq!(chunk.word_count(), cloned.word_count());
            prop_assert_eq!(chunk.has_code(), cloned.has_code());
            prop_assert_eq!(chunk.section(), cloned.section());
        }
    }
}
