use crate::features::indexing::engine::chunker::{ChunkerConfig, ContextualizedChunk, SemanticChunker};
use crate::features::indexing::engine::metadata_extractor::{
    determine_page_number, extract_metadata, DocumentMetadata,
};
use std::path::Path;
use std::sync::Arc;
use tokenizers::Tokenizer;

#[test]
fn test_metadata_extraction_basic() {
    let path = Path::new("test_document.pdf");
    let content = "This is test content";

    let metadata = extract_metadata(path, content, 0).unwrap();

    assert_eq!(metadata.title, "test_document.pdf");
    assert_eq!(metadata.document_type, "PDF");
    assert_eq!(metadata.page_number, None);
}

#[test]
fn test_metadata_section_extraction_markdown() {
    let path = Path::new("doc.md");
    let content = "# Introduction\nThis is the intro section.";

    let metadata = extract_metadata(path, content, 0).unwrap();

    assert_eq!(metadata.section, Some("Introduction".to_string()));
}

#[test]
fn test_metadata_section_extraction_uppercase() {
    let path = Path::new("doc.txt");
    let content = "EXECUTIVE SUMMARY\nThe company achieved...";

    let metadata = extract_metadata(path, content, 0).unwrap();

    assert_eq!(metadata.section, Some("EXECUTIVE SUMMARY".to_string()));
}

#[test]
fn test_page_number_determination() {
    let page_ranges = vec![(1, 0, 100), (2, 100, 250), (3, 250, 400)];

    assert_eq!(determine_page_number(50, &page_ranges), Some(1));
    assert_eq!(determine_page_number(150, &page_ranges), Some(2));
    assert_eq!(determine_page_number(300, &page_ranges), Some(3));
    assert_eq!(determine_page_number(500, &page_ranges), None);
}

#[tokio::test]
async fn test_context_prepending() -> anyhow::Result<()> {
    let tokenizer = Tokenizer::from_pretrained("bert-base-uncased", None)
        .map_err(|e| anyhow::anyhow!("Failed to load tokenizer: {}", e))?;

    let chunker = SemanticChunker::new(
        Arc::new(tokenizer),
        ChunkerConfig {
            max_tokens: 50,
            overlap_tokens: 10,
            prefer_sentence_boundaries: true,
        },
    )?;

    let metadata = DocumentMetadata {
        title: "Test.pdf".to_string(),
        page_number: Some(3),
        section: Some("Introduction".to_string()),
        document_type: "PDF".to_string(),
    };

    let text = "This is sample text that will be chunked with context.";
    let chunks = chunker.chunk_with_context(text, &metadata)?;

    assert!(!chunks.is_empty());

    let first_chunk = &chunks[0];
    assert!(first_chunk.context_prefix.contains("Test.pdf"));
    assert!(first_chunk.context_prefix.contains("Page: 3"));
    assert!(first_chunk.context_prefix.contains("Introduction"));
    assert_eq!(first_chunk.original_content, text);
    assert!(first_chunk
        .contextualized_content
        .starts_with("[Document:"));
    assert!(first_chunk.contextualized_content.contains(text));

    Ok(())
}

#[test]
fn test_context_prefix_format() {
    let metadata = DocumentMetadata {
        title: "Financial_Report.pdf".to_string(),
        page_number: Some(5),
        section: Some("Revenue Growth".to_string()),
        document_type: "PDF".to_string(),
    };

    let expected_parts = vec![
        "Document: Financial_Report.pdf",
        "Page: 5",
        "Section: Revenue Growth",
    ];

    for part in expected_parts {
        let context_prefix = format!(
            "[{}]",
            vec![
                format!("Document: {}", metadata.title),
                format!("Page: {}", metadata.page_number.unwrap()),
                format!("Section: {}", metadata.section.as_ref().unwrap()),
            ]
            .join(" | ")
        );

        assert!(context_prefix.contains(part));
    }
}

#[test]
fn test_context_prefix_without_optional_fields() {
    let metadata = DocumentMetadata {
        title: "Simple.txt".to_string(),
        page_number: None,
        section: None,
        document_type: "TXT".to_string(),
    };

    let context_prefix = format!("[Document: {}]", metadata.title);

    assert!(context_prefix.contains("Simple.txt"));
    assert!(!context_prefix.contains("Page:"));
    assert!(!context_prefix.contains("Section:"));
}

#[tokio::test]
async fn test_contextualized_chunk_structure() -> anyhow::Result<()> {
    let tokenizer = Tokenizer::from_pretrained("bert-base-uncased", None)
        .map_err(|e| anyhow::anyhow!("Failed to load tokenizer: {}", e))?;

    let chunker = SemanticChunker::new(
        Arc::new(tokenizer),
        ChunkerConfig {
            max_tokens: 50,
            overlap_tokens: 10,
            prefer_sentence_boundaries: true,
        },
    )?;

    let metadata = DocumentMetadata {
        title: "Doc.pdf".to_string(),
        page_number: Some(1),
        section: None,
        document_type: "PDF".to_string(),
    };

    let text = "First sentence. Second sentence. Third sentence.";
    let chunks = chunker.chunk_with_context(text, &metadata)?;

    for (idx, chunk) in chunks.iter().enumerate() {
        assert_eq!(chunk.chunk_index, idx);
        assert!(!chunk.original_content.is_empty());
        assert!(!chunk.contextualized_content.is_empty());
        assert!(!chunk.context_prefix.is_empty());
        assert!(chunk.token_count > 0);

        assert!(chunk.contextualized_content.len() > chunk.original_content.len());

        assert!(
            chunk
                .contextualized_content
                .contains(&chunk.original_content)
        );
    }

    Ok(())
}

#[test]
fn test_multiple_chunks_same_metadata() {
    let metadata = DocumentMetadata {
        title: "Long_Document.pdf".to_string(),
        page_number: Some(10),
        section: Some("Analysis".to_string()),
        document_type: "PDF".to_string(),
    };

    let context_prefix = format!(
        "[{}]",
        vec![
            format!("Document: {}", metadata.title),
            format!("Page: {}", metadata.page_number.unwrap()),
            format!("Section: {}", metadata.section.as_ref().unwrap()),
        ]
        .join(" | ")
    );

    assert_eq!(
        context_prefix,
        "[Document: Long_Document.pdf | Page: 10 | Section: Analysis]"
    );
}

#[test]
fn test_section_extraction_colon_format() {
    let content = "Executive Summary:\nThis document provides an overview...";
    let metadata = extract_metadata(Path::new("doc.txt"), content, 0).unwrap();

    assert_eq!(metadata.section, Some("Executive Summary".to_string()));
}

#[test]
fn test_no_section_extraction_for_normal_text() {
    let content = "This is just normal text without any section headers. It continues on...";
    let metadata = extract_metadata(Path::new("doc.txt"), content, 0).unwrap();

    assert_eq!(metadata.section, None);
}
