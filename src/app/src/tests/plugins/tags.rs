//! Smoke tests for Tags plugin DTOs

use vault::application::dtos::tag_dto::*;

#[test]
fn test_create_tag_request_dto() {
    let request = CreateTagRequestDto {
        name: "test-tag".to_string(),
        color: Some("#FF5733".to_string()),
        description: Some("Test tag".to_string()),
    };

    assert_eq!(request.name, "test-tag");
    assert_eq!(request.color.unwrap(), "#FF5733");
}

#[test]
fn test_create_tag_request_minimal() {
    let request = CreateTagRequestDto {
        name: "minimal".to_string(),
        color: None,
        description: None,
    };

    assert_eq!(request.name, "minimal");
    assert!(request.color.is_none());
    assert!(request.description.is_none());
}

#[test]
fn test_update_tag_request_dto() {
    let request = UpdateTagRequestDto {
        id: "tag-123".to_string(),
        name: Some("updated-name".to_string()),
        color: Some("#00FF00".to_string()),
        description: Some("Updated".to_string()),
    };

    assert_eq!(request.id, "tag-123");
    assert_eq!(request.name.unwrap(), "updated-name");
}

#[test]
fn test_delete_tag_request_dto() {
    let request = DeleteTagRequestDto {
        tag_id: "tag-456".to_string(),
    };

    assert_eq!(request.tag_id, "tag-456");
}

#[test]
fn test_apply_tags_request_dto() {
    let request = ApplyTagsRequestDto {
        document_id: "doc-789".to_string(),
        tag_names: vec!["tag1".to_string(), "tag2".to_string()],
    };

    assert_eq!(request.document_id, "doc-789");
    assert_eq!(request.tag_names.len(), 2);
}

#[test]
fn test_remove_tag_request_dto() {
    let request = RemoveTagRequestDto {
        document_id: "doc-123".to_string(),
        tag_id: "tag-456".to_string(),
    };

    assert_eq!(request.document_id, "doc-123");
    assert_eq!(request.tag_id, "tag-456");
}

#[test]
fn test_search_by_tag_request_dto() {
    let request = SearchByTagRequestDto {
        tag_name: "research".to_string(),
    };

    assert_eq!(request.tag_name, "research");
}

#[test]
fn test_generate_tags_request_dto() {
    let request = GenerateTagsRequestDto {
        document_id: "doc-999".to_string(),
        max_tags: 5,
    };

    assert_eq!(request.document_id, "doc-999");
    assert_eq!(request.max_tags, 5);
}

#[test]
fn test_auto_tag_request_dto() {
    let request = AutoTagRequestDto { max_documents: 50 };

    assert_eq!(request.max_documents, 50);
}
