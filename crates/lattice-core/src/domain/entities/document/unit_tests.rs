//! Unit tests for the document entity and its classifications.

#![cfg(test)]

use super::*;
use std::path::PathBuf;
use std::str::FromStr;

fn temp_path(file_name: &str) -> PathBuf {
    std::env::temp_dir().join(file_name)
}

fn create_test_document() -> Document {
    let path =
        ValidatedFilePath::new(temp_path("test.txt")).expect("Failed to create validated path");
    let checksum = Checksum::new("a".repeat(64)).expect("valid checksum");

    Document::new(
        path,
        "test.txt".to_string(),
        "text/plain".to_string(),
        1024,
        checksum,
    )
}

fn create_test_chunk(content: &str, chunk_number: usize) -> Chunk {
    Chunk::new(DocumentId::new(), content.to_string(), chunk_number)
}

mod display_fromstr_tests {
    use super::*;

    #[test]
    fn test_document_status_display_all_variants() {
        assert_eq!(DocumentStatus::Pending.to_string(), "pending");
        assert_eq!(DocumentStatus::Processing.to_string(), "processing");
        assert_eq!(DocumentStatus::Indexed.to_string(), "indexed");
        assert_eq!(DocumentStatus::Failed.to_string(), "failed");
    }

    #[test]
    fn test_document_status_from_str_valid() {
        assert_eq!(
            DocumentStatus::from_str("pending").unwrap(),
            DocumentStatus::Pending
        );
        assert_eq!(
            DocumentStatus::from_str("processing").unwrap(),
            DocumentStatus::Processing
        );
        assert_eq!(
            DocumentStatus::from_str("indexed").unwrap(),
            DocumentStatus::Indexed
        );
        assert_eq!(
            DocumentStatus::from_str("failed").unwrap(),
            DocumentStatus::Failed
        );
    }

    #[test]
    fn test_document_status_from_str_invalid() {
        assert!(DocumentStatus::from_str("invalid").is_err());
        assert!(DocumentStatus::from_str("not_a_status").is_err());
        assert!(DocumentStatus::from_str("").is_err());
    }

    #[test]
    fn test_language_display_all_variants() {
        assert_eq!(Language::English.to_string(), "english");
        assert_eq!(Language::Spanish.to_string(), "spanish");
        assert_eq!(Language::French.to_string(), "french");
        assert_eq!(Language::German.to_string(), "german");
        assert_eq!(Language::Chinese.to_string(), "chinese");
        assert_eq!(Language::Japanese.to_string(), "japanese");
        assert_eq!(Language::Python.to_string(), "python");
        assert_eq!(Language::Rust.to_string(), "rust");
        assert_eq!(Language::JavaScript.to_string(), "javascript");
        assert_eq!(Language::TypeScript.to_string(), "typescript");
        assert_eq!(Language::Go.to_string(), "go");
        assert_eq!(Language::Java.to_string(), "java");
        assert_eq!(Language::Cpp.to_string(), "cpp");
        assert_eq!(Language::CSharp.to_string(), "csharp");
        assert_eq!(Language::Unknown.to_string(), "unknown");
    }

    #[test]
    fn test_language_from_str_valid() {
        assert_eq!(Language::from_str("english").unwrap(), Language::English);
        assert_eq!(Language::from_str("spanish").unwrap(), Language::Spanish);
        assert_eq!(Language::from_str("python").unwrap(), Language::Python);
        assert_eq!(Language::from_str("rust").unwrap(), Language::Rust);
        assert_eq!(Language::from_str("unknown").unwrap(), Language::Unknown);
    }

    #[test]
    fn test_language_from_str_invalid_defaults_unknown() {
        // Language::from_str uses to_lowercase(), so it accepts case-insensitive input
        // "ENGLISH" -> "english" -> matches Language::English
        assert_eq!(Language::from_str("ENGLISH").unwrap(), Language::English);

        // These truly invalid inputs default to Unknown
        assert_eq!(
            Language::from_str("invalid_language").unwrap(),
            Language::Unknown
        );
        assert_eq!(
            Language::from_str("not_a_language").unwrap(),
            Language::Unknown
        );
    }

    #[test]
    fn test_category_display_all_variants() {
        assert_eq!(Category::Code.to_string(), "code");
        assert_eq!(Category::Documentation.to_string(), "documentation");
        assert_eq!(Category::Research.to_string(), "research");
        assert_eq!(Category::Notes.to_string(), "notes");
        assert_eq!(Category::Reference.to_string(), "reference");
        assert_eq!(Category::Tutorial.to_string(), "tutorial");
        assert_eq!(Category::Uncategorized.to_string(), "uncategorized");
    }

    #[test]
    fn test_category_from_str_valid() {
        assert_eq!(Category::from_str("code").unwrap(), Category::Code);
        assert_eq!(
            Category::from_str("documentation").unwrap(),
            Category::Documentation
        );
        assert_eq!(Category::from_str("research").unwrap(), Category::Research);
        assert_eq!(Category::from_str("notes").unwrap(), Category::Notes);
        assert_eq!(
            Category::from_str("uncategorized").unwrap(),
            Category::Uncategorized
        );
    }

    #[test]
    fn test_category_from_str_invalid_defaults_uncategorized() {
        assert_eq!(Category::from_str("CODE").unwrap(), Category::Code);

        // These truly invalid inputs default to Uncategorized
        assert_eq!(
            Category::from_str("invalid_category").unwrap(),
            Category::Uncategorized
        );
        assert_eq!(
            Category::from_str("not_a_category").unwrap(),
            Category::Uncategorized
        );
    }
}

mod chunk_manipulation_tests {
    use super::*;

    #[test]
    fn test_set_chunk_token_count_valid_index() {
        let mut doc = create_test_document();
        let chunk = create_test_chunk("Test content", 0);
        doc.chunks.push(chunk);

        doc.set_chunk_token_count(0, 42);

        assert_eq!(doc.chunks()[0].token_count(), 42);
    }

    #[test]
    fn test_set_chunk_token_count_invalid_index() {
        let mut doc = create_test_document();

        doc.set_chunk_token_count(0, 42);

        // Document should remain valid
        assert_eq!(doc.chunks().len(), 0);
    }

    #[test]
    fn test_set_chunk_word_count_valid_index() {
        let mut doc = create_test_document();
        let chunk = create_test_chunk("Test content with multiple words", 0);
        doc.chunks.push(chunk);

        doc.set_chunk_word_count(0, 5);

        assert_eq!(doc.chunks()[0].word_count(), 5);
    }

    #[test]
    fn test_set_chunk_word_count_invalid_index() {
        let mut doc = create_test_document();

        doc.set_chunk_word_count(0, 5);

        // Document should remain valid
        assert_eq!(doc.chunks().len(), 0);
    }

    #[test]
    fn test_set_chunk_has_code_valid_index() {
        let mut doc = create_test_document();
        let chunk = create_test_chunk("fn main() { println!(\"Hello\"); }", 0);
        doc.chunks.push(chunk);

        doc.set_chunk_has_code(0, true);

        assert!(doc.chunks()[0].has_code());
    }

    #[test]
    fn test_set_chunk_has_code_invalid_index() {
        let mut doc = create_test_document();

        doc.set_chunk_has_code(0, true);

        // Document should remain valid
        assert_eq!(doc.chunks().len(), 0);
    }

    #[test]
    fn test_set_chunk_section_valid_index() {
        let mut doc = create_test_document();
        let chunk = create_test_chunk("Section content", 0);
        doc.chunks.push(chunk);

        doc.set_chunk_section(0, Some("Introduction".to_string()));

        assert_eq!(doc.chunks()[0].section(), Some("Introduction"));
    }

    #[test]
    fn test_set_chunk_section_invalid_index() {
        let mut doc = create_test_document();

        doc.set_chunk_section(0, Some("Introduction".to_string()));

        // Document should remain valid
        assert_eq!(doc.chunks().len(), 0);
    }

    #[test]
    fn test_chunk_mutation_maintains_aggregate_root() {
        let mut doc = create_test_document();
        let chunk1 = create_test_chunk("First chunk", 0);
        let chunk2 = create_test_chunk("Second chunk", 1);
        doc.chunks.push(chunk1);
        doc.chunks.push(chunk2);

        // Mutate first chunk
        doc.set_chunk_token_count(0, 10);
        doc.set_chunk_has_code(0, true);

        assert_eq!(doc.chunks()[0].token_count(), 10);
        assert!(doc.chunks()[0].has_code());

        assert_eq!(doc.chunks()[1].token_count(), 0);
        assert!(!doc.chunks()[1].has_code());
    }
}

mod metadata_operations_tests {
    use super::*;

    #[test]
    fn test_set_language_updates_field() {
        let mut doc = create_test_document();

        doc.set_language(Language::Rust);
        assert_eq!(doc.language(), &Language::Rust);

        doc.set_language(Language::Python);
        assert_eq!(doc.language(), &Language::Python);
    }

    #[test]
    fn test_set_category_updates_field() {
        let mut doc = create_test_document();

        doc.set_category(Category::Code);
        assert_eq!(doc.category(), &Category::Code);

        doc.set_category(Category::Documentation);
        assert_eq!(doc.category(), &Category::Documentation);
    }

    #[test]
    fn test_set_quality_score_valid_range() {
        let mut doc = create_test_document();

        doc.set_quality_score(0.75);
        assert_eq!(doc.quality_score(), 0.75);

        doc.set_quality_score(0.0);
        assert_eq!(doc.quality_score(), 0.0);

        doc.set_quality_score(1.0);
        assert_eq!(doc.quality_score(), 1.0);
    }

    #[test]
    fn test_set_quality_score_clamps_negative() {
        let mut doc = create_test_document();

        doc.set_quality_score(-0.5);
        assert_eq!(doc.quality_score(), 0.0);

        doc.set_quality_score(-10.0);
        assert_eq!(doc.quality_score(), 0.0);
    }

    #[test]
    fn test_set_quality_score_clamps_above_one() {
        let mut doc = create_test_document();

        doc.set_quality_score(1.5);
        assert_eq!(doc.quality_score(), 1.0);

        doc.set_quality_score(10.0);
        assert_eq!(doc.quality_score(), 1.0);
    }

    #[test]
    fn test_increment_access_updates_count_and_timestamp() {
        let mut doc = create_test_document();

        let initial_count = doc.access_count();
        assert!(doc.last_accessed_at().is_none());

        doc.increment_access();

        assert_eq!(doc.access_count(), initial_count + 1);
        assert!(doc.last_accessed_at().is_some());

        let first_timestamp = *doc.last_accessed_at().unwrap();

        // Sleep briefly to ensure timestamp difference
        std::thread::sleep(std::time::Duration::from_millis(10));

        doc.increment_access();

        assert_eq!(doc.access_count(), initial_count + 2);
        assert!(doc.last_accessed_at().unwrap() >= &first_timestamp);
    }

    #[test]
    fn test_set_word_count_clamps_negative() {
        let mut doc = create_test_document();

        doc.set_word_count(-10);
        assert_eq!(doc.word_count(), 0);

        doc.set_word_count(-1);
        assert_eq!(doc.word_count(), 0);

        doc.set_word_count(0);
        assert_eq!(doc.word_count(), 0);

        doc.set_word_count(100);
        assert_eq!(doc.word_count(), 100);
    }
}

mod factory_tests {
    use super::*;
    use crate::domain::value_objects::{ChunkingStrategy, FileMetadata};

    /// Helper to create test FileMetadata
    fn create_test_metadata() -> FileMetadata {
        FileMetadata::new(
            "test_document.txt".to_string(),
            "text/plain".to_string(),
            1024,
            Utc::now(),
        )
        .expect("Failed to create test metadata")
    }

    #[test]
    fn test_from_file_happy_path() {
        let file_path =
            ValidatedFilePath::new(temp_path("test.txt")).expect("Failed to create validated path");
        let metadata = create_test_metadata();
        let checksum = Checksum::new("a".repeat(64)).expect("valid checksum");
        let content = "This is test content for the document.".to_string();
        let strategy = ChunkingStrategy::FixedSize { size: 512 };

        let result = Document::from_file(file_path, metadata, checksum, content.clone(), strategy);

        assert!(result.is_ok());
        let doc = result.unwrap();
        assert_eq!(doc.status(), DocumentStatus::Pending);
        assert_eq!(doc.file_name(), "test_document.txt");
        assert_eq!(doc.mime_type(), "text/plain");
        assert_eq!(doc.size_bytes(), 1024);
        assert_eq!(doc.content(), content.as_str());
    }

    #[test]
    fn test_from_file_computes_word_count() {
        let file_path =
            ValidatedFilePath::new(temp_path("test.txt")).expect("Failed to create validated path");
        let metadata = create_test_metadata();
        let checksum = Checksum::new("a".repeat(64)).expect("valid checksum");
        let content = "One two three four five words here.".to_string();
        let strategy = ChunkingStrategy::default();

        let doc = Document::from_file(file_path, metadata, checksum, content, strategy)
            .expect("Failed to create document");

        assert_eq!(doc.word_count(), 7);
    }

    #[test]
    fn test_from_file_sets_defaults() {
        let file_path =
            ValidatedFilePath::new(temp_path("test.txt")).expect("Failed to create validated path");
        let metadata = create_test_metadata();
        let checksum = Checksum::new("a".repeat(64)).expect("valid checksum");
        let content = "Default test content".to_string();
        let strategy = ChunkingStrategy::default();

        let doc = Document::from_file(file_path, metadata, checksum, content, strategy)
            .expect("Failed to create document");

        assert_eq!(doc.status(), DocumentStatus::Pending);
        assert_eq!(doc.quality_score(), 0.5);
        assert_eq!(doc.access_count(), 0);
        assert!(doc.last_accessed_at().is_none());
        assert_eq!(doc.language(), &Language::Unknown);
        assert_eq!(doc.category(), &Category::Uncategorized);
        assert!(doc.error_message().is_none());
    }

    #[test]
    fn test_from_file_preserves_metadata() {
        let file_path = ValidatedFilePath::new(temp_path("example.md"))
            .expect("Failed to create validated path");
        let modified_at = Utc::now();
        let metadata = FileMetadata::new(
            "example.md".to_string(),
            "text/markdown".to_string(),
            2048,
            modified_at,
        )
        .expect("Failed to create metadata");
        let checksum = Checksum::new("e".repeat(64)).expect("valid checksum");
        let content = "# Markdown Document\n\nContent here.".to_string();
        let strategy = ChunkingStrategy::default();

        let doc = Document::from_file(
            file_path,
            metadata,
            checksum.clone(),
            content.clone(),
            strategy,
        )
        .expect("Failed to create document");

        assert_eq!(doc.file_name(), "example.md");
        assert_eq!(doc.mime_type(), "text/markdown");
        assert_eq!(doc.size_bytes(), 2048);
        assert_eq!(doc.modified_at(), &modified_at);
        assert_eq!(doc.checksum(), &checksum);
        assert_eq!(doc.content(), content.as_str());
        assert_eq!(doc.file_type(), Some("md"));
    }

    #[test]
    fn test_from_file_empty_content_word_count_zero() {
        let file_path = ValidatedFilePath::new(temp_path("empty.txt"))
            .expect("Failed to create validated path");
        let metadata = create_test_metadata();
        let checksum = Checksum::new("b".repeat(64)).expect("valid checksum");
        let content = String::new();
        let strategy = ChunkingStrategy::default();

        let doc = Document::from_file(file_path, metadata, checksum, content, strategy)
            .expect("Failed to create document");

        assert_eq!(doc.word_count(), 0);
        assert_eq!(doc.content(), "");
        assert_eq!(doc.status(), DocumentStatus::Pending);
    }

    #[test]
    fn test_from_file_generates_chunks() {
        let file_path =
            ValidatedFilePath::new(temp_path("test.txt")).expect("Failed to create validated path");
        let metadata = create_test_metadata();
        let checksum = Checksum::new("a".repeat(64)).expect("valid checksum");
        let content = "This is a test document with some content.".to_string();
        let strategy = ChunkingStrategy::FixedSize { size: 10 };

        let document =
            Document::from_file(file_path, metadata, checksum, content.clone(), strategy)
                .expect("from_file should succeed");

        assert!(!document.chunks().is_empty(), "Document must have chunks");

        for chunk in document.chunks() {
            assert_eq!(chunk.document_id(), document.id());
        }

        let reconstructed: String = document.chunks().iter().map(|c| c.content()).collect();
        assert_eq!(reconstructed, content);
    }

    #[test]
    fn test_from_file_empty_content_no_chunks() {
        let file_path = ValidatedFilePath::new(temp_path("empty.txt"))
            .expect("Failed to create validated path");
        let metadata = create_test_metadata();
        let checksum = Checksum::new("b".repeat(64)).expect("valid checksum");
        let content = "".to_string();
        let strategy = ChunkingStrategy::default();

        let document = Document::from_file(file_path, metadata, checksum, content, strategy)
            .expect("from_file should succeed for empty content");

        assert_eq!(
            document.chunks().len(),
            0,
            "Empty content should have no chunks"
        );
    }

    #[test]
    fn test_from_file_whitespace_only_no_chunks() {
        let file_path = ValidatedFilePath::new(temp_path("whitespace.txt"))
            .expect("Failed to create validated path");
        let metadata = create_test_metadata();
        let checksum = Checksum::new("f".repeat(64)).expect("valid checksum");
        let content = "   \n\t   ".to_string();
        let strategy = ChunkingStrategy::default();

        let document = Document::from_file(file_path, metadata, checksum, content, strategy)
            .expect("from_file should succeed for whitespace");

        assert_eq!(
            document.chunks().len(),
            0,
            "Whitespace-only should have no chunks"
        );
    }
}
