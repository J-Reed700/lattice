//! Smoke tests for Conversation plugin DTOs

use vault::application::dtos::conversation_dto::*;

#[test]
fn test_create_conversation_request_dto() {
    let request = CreateConversationRequestDto {
        title: "Research Discussion".to_string(),
        model_name: "gpt-4".to_string(),
        system_prompt: Some("You are a helpful assistant.".to_string()),
    };

    assert_eq!(request.title, "Research Discussion");
    assert_eq!(request.model_name, "gpt-4");
    assert!(request.system_prompt.is_some());
}

#[test]
fn test_create_conversation_request_minimal() {
    let request = CreateConversationRequestDto {
        title: "Quick Chat".to_string(),
        model_name: "gpt-3.5-turbo".to_string(),
        system_prompt: None,
    };

    assert_eq!(request.title, "Quick Chat");
    assert!(request.system_prompt.is_none());
}

#[test]
fn test_list_conversations_query() {
    let query = ListConversationsQuery {
        limit: Some(20),
        offset: Some(0),
    };

    assert_eq!(query.limit.unwrap(), 20);
    assert_eq!(query.offset.unwrap(), 0);
}

#[test]
fn test_list_conversations_query_defaults() {
    let query = ListConversationsQuery {
        limit: None,
        offset: None,
    };

    assert!(query.limit.is_none());
    assert!(query.offset.is_none());
}

#[test]
fn test_get_conversation_request_dto() {
    let request = GetConversationRequestDto {
        conversation_id: "conv-123".to_string(),
    };

    assert_eq!(request.conversation_id, "conv-123");
}

#[test]
fn test_rename_conversation_request_dto() {
    let request = RenameConversationRequestDto {
        conversation_id: "conv-456".to_string(),
        new_title: "Updated Title".to_string(),
    };

    assert_eq!(request.conversation_id, "conv-456");
    assert_eq!(request.new_title, "Updated Title");
}

#[test]
fn test_delete_conversation_request_dto() {
    let request = DeleteConversationRequestDto {
        conversation_id: "conv-789".to_string(),
    };

    assert_eq!(request.conversation_id, "conv-789");
}

#[test]
fn test_conversation_dto_complete() {
    let dto = ConversationDto {
        id: "conv-111".to_string(),
        title: "Test Conversation".to_string(),
        model_name: "gpt-4".to_string(),
        system_prompt: Some("You are helpful".to_string()),
        created_at: "2024-01-01T00:00:00Z".to_string(),
        updated_at: "2024-01-01T00:00:00Z".to_string(),
        message_count: 10,
        total_tokens: 500,
        space_id: Some("space_general".to_string()),
        is_saved: Some(false),
        is_bookmarked: Some(false),
        is_pinned: Some(false),
        is_archived: Some(false),
        saved_at: None,
        bookmarked_at: None,
        pinned_at: None,
        archived_at: None,
        last_message_preview: Some("Most recent content".to_string()),
    };

    assert_eq!(dto.id, "conv-111");
    assert_eq!(dto.message_count, 10);
}

#[test]
fn test_message_dto_creation() {
    let dto = MessageDto {
        id: "msg-222".to_string(),
        conversation_id: "conv-111".to_string(),
        role: "user".to_string(),
        content: "Hello".to_string(),
        status: "completed".to_string(),
        tokens: 5,
        created_at: "2024-01-01T00:00:00Z".to_string(),
        metadata: None,
    };

    assert_eq!(dto.role, "user");
    assert_eq!(dto.tokens, 5);
}
