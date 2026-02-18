//! Smoke tests for Extraction plugin DTOs

use vault::application::dtos::extraction_dto::*;

#[test]
fn test_parse_wikilinks_request_dto() {
    let request = ParseWikilinksRequestDto {
        text: "This is [[a link]] and [[another|display text]].".to_string(),
        source_path: Some("/path/to/doc.md".to_string()),
    };

    assert!(request.text.contains("[[a link]]"));
    assert_eq!(request.source_path.unwrap(), "/path/to/doc.md");
}

#[test]
fn test_parse_wikilinks_request_no_source() {
    let request = ParseWikilinksRequestDto {
        text: "Content with [[link]]".to_string(),
        source_path: None,
    };

    assert!(request.source_path.is_none());
}

#[test]
fn test_extract_title_request_dto() {
    let request = ExtractTitleRequestDto {
        content: "# Document Title\n\nContent here.".to_string(),
    };

    assert!(request.content.contains("# Document Title"));
}

#[test]
fn test_resolve_wikilink_request_dto() {
    let doc_ref = DocumentRefDto {
        document_id: "doc-123".to_string(),
        file_path: "notes/test.md".to_string(),
        title: Some("Test Note".to_string()),
    };

    let request = ResolveWikilinkRequestDto {
        link_target: "test".to_string(),
        source_document_id: Some("doc-456".to_string()),
        available_documents: vec![doc_ref],
    };

    assert_eq!(request.link_target, "test");
    assert_eq!(request.available_documents.len(), 1);
}

#[test]
fn test_document_ref_dto_creation() {
    let doc_ref = DocumentRefDto {
        document_id: "doc-789".to_string(),
        file_path: "projects/project.md".to_string(),
        title: Some("Project Notes".to_string()),
    };

    assert_eq!(doc_ref.document_id, "doc-789");
    assert_eq!(doc_ref.file_path, "projects/project.md");
    assert_eq!(doc_ref.title.as_ref().unwrap(), "Project Notes");
}

#[test]
fn test_extract_and_resolve_request_dto() {
    let request = ExtractAndResolveRequestDto {
        document_id: "doc-999".to_string(),
        content: "Content with [[links]]".to_string(),
    };

    assert_eq!(request.document_id, "doc-999");
    assert!(request.content.contains("[[links]]"));
}
