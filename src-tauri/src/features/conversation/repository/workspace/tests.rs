use super::*;
use crate::features::conversation::space_repository::ConversationSpaceRepository;
use crate::infrastructure::persistence::database::{initialize_database, DatabaseConnection};

async fn repository() -> (tempfile::TempDir, ConversationRepository) {
    let directory = tempfile::tempdir().unwrap();
    let database = DatabaseConnection::new(directory.path().join("workspace.db"))
        .await
        .unwrap();
    initialize_database(database.pool()).await.unwrap();
    (
        directory,
        ConversationRepository::new(database.pool().clone()),
    )
}

fn journal(name: &str) -> CreateConversationJournalRequestDto {
    CreateConversationJournalRequestDto {
        name: name.into(),
        description: None,
        icon: None,
        accent_color: None,
        space_prompt: None,
        default_model_name: None,
        tool_preferences_json: None,
    }
}

#[tokio::test]
async fn journal_membership_is_idempotent_and_preserves_conversation_space() {
    let (_directory, repo) = repository().await;
    let conversation = repo.create("Research", "test-model", None).await.unwrap();
    let journal = repo.create_journal(journal("Reading")).await.unwrap();
    for _ in 0..2 {
        repo.add_conversation_to_journal(AddConversationToJournalRequestDto {
            journal_space_id: journal.id.clone(),
            conversation_id: conversation.id.to_string(),
        })
        .await
        .unwrap();
    }
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM journal_conversation_entries WHERE journal_space_id = ?",
    )
    .bind(&journal.id)
    .fetch_one(&repo.pool)
    .await
    .unwrap();
    assert_eq!(count, 1);
    assert_eq!(
        repo.project_conversation(&conversation)
            .await
            .unwrap()
            .space_id
            .as_deref(),
        Some("space_general")
    );
    repo.remove_conversation_from_journal(RemoveConversationFromJournalRequestDto {
        journal_space_id: journal.id,
        conversation_id: conversation.id.to_string(),
    })
    .await
    .unwrap();
    assert!(repo
        .find_by_id(&conversation.id.to_string())
        .await
        .unwrap()
        .is_some());
}

#[tokio::test]
async fn invalid_membership_does_not_write_and_default_space_cannot_be_archived() {
    let (_directory, repo) = repository().await;
    let journal = repo.create_journal(journal("Reading")).await.unwrap();
    let result = repo
        .add_conversation_to_journal(AddConversationToJournalRequestDto {
            journal_space_id: journal.id,
            conversation_id: uuid::Uuid::new_v4().to_string(),
        })
        .await;
    assert!(matches!(result, Err(AppError::NotFound(_))));
    assert!(repo
        .archive_conversation_space(ArchiveConversationSpaceRequestDto {
            space_id: "space_general".into(),
            archived: true,
        })
        .await
        .is_err());
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM journal_conversation_entries")
        .fetch_one(&repo.pool)
        .await
        .unwrap();
    assert_eq!(count, 0);
}

/// Moving a chat used to file every document it had cited into the destination
/// space, where all the other chats then searched them.
#[tokio::test]
async fn moving_a_conversation_does_not_file_its_documents_into_the_destination() {
    let (_directory, repo) = repository().await;
    let conversation = repo.create("Research", "test-model", None).await.unwrap();
    sqlx::query(
        "INSERT INTO documents (id, file_path, file_name, size_bytes, modified_at, checksum) \
         VALUES ('doc-cited', '/vault/cited.pdf', 'cited.pdf', 1, '2026-09-01T10:00:00Z', 'sum')",
    )
    .execute(&repo.pool)
    .await
    .unwrap();
    repo.add_document_reference(&conversation.id.to_string(), "doc-cited", None, Some(0.9))
        .await
        .unwrap();
    let target = ConversationSpaceRepository::new(repo.pool.clone())
        .create(CreateConversationSpaceRequestDto {
            name: "Destination".into(),
            description: None,
            icon: None,
            accent_color: None,
            space_prompt: None,
            default_model_name: None,
            tool_preferences_json: None,
        })
        .await
        .unwrap();

    repo.move_conversation_to_space(MoveConversationToSpaceRequestDto {
        conversation_id: conversation.id.to_string(),
        space_id: target.id.clone(),
    })
    .await
    .unwrap();

    assert_eq!(
        repo.project_conversation(&conversation)
            .await
            .unwrap()
            .space_id
            .as_deref(),
        Some(target.id.as_str())
    );
    let filed_in_destination: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM document_space_memberships WHERE space_id = ?",
    )
    .bind(&target.id)
    .fetch_one(&repo.pool)
    .await
    .unwrap();
    assert_eq!(filed_in_destination, 0);
}

#[tokio::test]
async fn deleting_another_conversations_message_is_rejected_without_changing_totals() {
    let (_directory, repo) = repository().await;
    let first = repo.create("First", "test-model", None).await.unwrap();
    let second = repo.create("Second", "test-model", None).await.unwrap();
    let message = repo
        .add_message(
            &first.id.to_string(),
            crate::domain::conversation::MessageRole::User,
            "Keep this",
            7,
            None,
        )
        .await
        .unwrap();
    assert!(repo
        .delete_conversation_message(DeleteConversationMessageRequestDto {
            conversation_id: second.id.to_string(),
            message_id: message.id.to_string(),
        })
        .await
        .is_err());
    assert_eq!(
        repo.get_messages(&first.id.to_string())
            .await
            .unwrap()
            .len(),
        1
    );
    repo.delete_conversation_message(DeleteConversationMessageRequestDto {
        conversation_id: first.id.to_string(),
        message_id: message.id.to_string(),
    })
    .await
    .unwrap();
    let stored = repo
        .find_by_id(&first.id.to_string())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(stored.message_count, 0);
    assert_eq!(stored.total_tokens, 0);
}
