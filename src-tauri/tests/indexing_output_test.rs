#![allow(clippy::panic)]
#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
#![allow(clippy::assertions_on_constants)]
#![allow(clippy::useless_vec)]
#![allow(clippy::indexing_slicing)]
#![allow(unused_variables)]
#![allow(unused_imports)]

use insta::assert_json_snapshot;
use lattice::features::indexing::engine::chunker::TextChunk;
use lattice::features::indexing::engine::events::IndexingEvent;
use serde_json::json;
#[test]
fn test_text_chunk_output() {
    let chunks = vec![
        TextChunk {
            text: "This is the first chunk of text from the document.".to_string(),
            start_idx: 0,
            end_idx: 51,
            token_count: 10,
            document_path: String::new(),
            chunk_index: 0,
        },
        TextChunk {
            text: "This is the second chunk with some overlap.".to_string(),
            start_idx: 45,
            end_idx: 89,
            token_count: 9,
            document_path: String::new(),
            chunk_index: 1,
        },
        TextChunk {
            text: "Final chunk of the document.".to_string(),
            start_idx: 85,
            end_idx: 113,
            token_count: 6,
            document_path: String::new(),
            chunk_index: 2,
        },
    ];

    assert_json_snapshot!("text_chunks_basic", chunks);
}

#[test]
fn test_text_chunk_with_overlap() {
    let chunks = vec![
        TextChunk {
            text: "Machine learning is a subset of artificial intelligence.".to_string(),
            start_idx: 0,
            end_idx: 57,
            token_count: 12,
            document_path: String::new(),
            chunk_index: 0,
        },
        TextChunk {
            text: "artificial intelligence. It focuses on algorithms".to_string(),
            start_idx: 33,
            end_idx: 83,
            token_count: 10,
            document_path: String::new(),
            chunk_index: 1,
        },
    ];

    assert_json_snapshot!("text_chunks_with_overlap", chunks);
}

#[test]
fn test_empty_chunking_output() {
    let chunks: Vec<TextChunk> = vec![];
    assert_json_snapshot!("empty_chunks", chunks);
}

#[test]
fn test_single_chunk() {
    let chunk = TextChunk {
        text: "Short document that fits in one chunk.".to_string(),
        start_idx: 0,
        end_idx: 39,
        token_count: 8,
        document_path: String::new(),
        chunk_index: 0,
    };

    assert_json_snapshot!("single_chunk", chunk);
}

#[test]
fn test_indexing_started_event() {
    let event = IndexingEvent::started(150);
    assert_json_snapshot!("indexing_started", event);
}

#[test]
fn test_indexing_file_started_event() {
    let event = IndexingEvent::file_started("/documents/research/paper.pdf".to_string(), 1, 150);

    assert_json_snapshot!("indexing_file_started", event, {
        ".path" => "[file_path]"
    });
}

#[test]
fn test_indexing_file_completed_event() {
    let event = IndexingEvent::file_completed(
        "/documents/notes/meeting_notes.txt".to_string(),
        12,
        1250,
        25,
        150,
    );

    assert_json_snapshot!("indexing_file_completed", event, {
        ".path" => "[file_path]"
    });
}

#[test]
fn test_indexing_file_error_event() {
    let event = IndexingEvent::file_error(
        "/documents/corrupted/bad_file.pdf".to_string(),
        "Failed to extract text: invalid PDF structure".to_string(),
        42,
        150,
    );

    assert_json_snapshot!("indexing_file_error", event, {
        ".path" => "[file_path]",
        ".error" => insta::dynamic_redaction(|value, _path| {
            value.as_str().unwrap().to_string()
        })
    });
}

#[test]
fn test_indexing_completed_event() {
    let event = IndexingEvent::completed(150, 1823, 125000);
    assert_json_snapshot!("indexing_completed", event);
}

#[test]
fn test_indexing_cancelled_event() {
    let event = IndexingEvent::cancelled();
    assert_json_snapshot!("indexing_cancelled", event);
}

#[test]
fn test_indexing_event_sequence() {
    let events = vec![
        IndexingEvent::started(3),
        IndexingEvent::file_started("file1.txt".to_string(), 1, 3),
        IndexingEvent::file_completed("file1.txt".to_string(), 5, 500, 1, 3),
        IndexingEvent::file_started("file2.pdf".to_string(), 2, 3),
        IndexingEvent::file_completed("file2.pdf".to_string(), 8, 1200, 2, 3),
        IndexingEvent::file_started("file3.md".to_string(), 3, 3),
        IndexingEvent::file_error("file3.md".to_string(), "File not found".to_string(), 3, 3),
        IndexingEvent::completed(2, 13, 1700),
    ];

    assert_json_snapshot!("indexing_event_sequence", events, {
        ".**.path" => insta::sorted_redaction()
    });
}

#[test]
fn test_chunking_output_large_document() {
    let chunks: Vec<TextChunk> = (0..50)
        .map(|i| TextChunk {
            text: format!("Chunk {} content with some sample text", i),
            start_idx: i * 100,
            end_idx: (i + 1) * 100,
            token_count: 50 - i,
            document_path: String::new(),
            chunk_index: i,
        })
        .collect();

    assert_json_snapshot!("chunking_large_document", &chunks[0..5]);
}

#[test]
fn test_chunk_with_special_characters() {
    let chunk = TextChunk {
        text: r#"Special chars: "quotes", 'apostrophes', & symbols, <tags>, @mentions"#.to_string(),
        start_idx: 0,
        end_idx: 69,
        token_count: 15,
        document_path: String::new(),
        chunk_index: 0,
    };

    assert_json_snapshot!("chunk_special_characters", chunk);
}

#[test]
fn test_chunk_with_unicode() {
    let chunk = TextChunk {
        text: "Unicode: 日本語 中文 한글 العربية Ελληνικά".to_string(),
        start_idx: 0,
        end_idx: 50,
        token_count: 10,
        document_path: String::new(),
        chunk_index: 0,
    };

    assert_json_snapshot!("chunk_unicode", chunk);
}

#[test]
fn test_embedding_storage_format() {
    let embedding_data = json!({
        "chunk_id": "chunk_abc123",
        "document_id": "doc_xyz789",
        "embedding": vec![0.1, 0.2, 0.3, 0.4, 0.5],
        "dimensions": 5,
        "model": "all-MiniLM-L6-v2",
        "version": "1.0"
    });

    assert_json_snapshot!("embedding_storage", embedding_data, {
        ".chunk_id" => "[chunk_id]",
        ".document_id" => "[doc_id]",
        ".embedding" => insta::sorted_redaction()
    });
}

#[test]
fn test_batch_embedding_storage() {
    let batch_data = json!({
        "batch_id": "batch_20240115_001",
        "embeddings": vec![
            json!({
                "chunk_id": "chunk1",
                "embedding": vec![0.1, 0.2, 0.3]
            }),
            json!({
                "chunk_id": "chunk2",
                "embedding": vec![0.4, 0.5, 0.6]
            }),
            json!({
                "chunk_id": "chunk3",
                "embedding": vec![0.7, 0.8, 0.9]
            })
        ],
        "total_count": 3,
        "processing_time_ms": 450
    });

    assert_json_snapshot!("batch_embedding_storage", batch_data, {
        ".batch_id" => "[batch_id]",
        ".embeddings.**.chunk_id" => insta::sorted_redaction()
    });
}

#[test]
fn test_indexing_progress_events() {
    let progress_events = vec![
        json!({
            "type": "progress",
            "current": 10,
            "total": 100,
            "percentage": 10.0,
            "chunks_processed": 50
        }),
        json!({
            "type": "progress",
            "current": 50,
            "total": 100,
            "percentage": 50.0,
            "chunks_processed": 250
        }),
        json!({
            "type": "progress",
            "current": 100,
            "total": 100,
            "percentage": 100.0,
            "chunks_processed": 500
        }),
    ];

    assert_json_snapshot!("indexing_progress_events", progress_events);
}

#[test]
fn test_chunk_metadata() {
    let chunk_with_metadata = json!({
        "chunk": {
            "text": "Sample text chunk",
            "start_idx": 0,
            "end_idx": 17,
            "token_count": 4
        },
        "metadata": {
            "document_title": "Research Paper",
            "section": "Introduction",
            "page": 1,
            "created_at": "2024-01-15T10:00:00Z",
            "language": "en"
        }
    });

    assert_json_snapshot!("chunk_with_metadata", chunk_with_metadata, {
        ".metadata.created_at" => "[timestamp]"
    });
}

#[test]
fn test_indexing_error_collection() {
    let errors = vec![
        json!({
            "file": "document1.pdf",
            "error": "Unsupported format",
            "timestamp": "2024-01-15T10:00:00Z"
        }),
        json!({
            "file": "document2.docx",
            "error": "File corrupted",
            "timestamp": "2024-01-15T10:05:00Z"
        }),
    ];

    assert_json_snapshot!("indexing_errors", errors, {
        ".**.timestamp" => "[timestamp]"
    });
}
