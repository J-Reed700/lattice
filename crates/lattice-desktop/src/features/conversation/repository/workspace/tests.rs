use super::*;
use crate::features::conversation::space_repository::ConversationSpaceRepository;
use crate::infrastructure::persistence::database::{initialize_database, DatabaseConnection};

async fn repository() -> (tempfile::TempDir, ConversationRepository) {
    let directory = tempfile::tempdir().unwrap();
    let database = DatabaseConnection::new(directory.path().join("workspace.db"))
        .await
        .unwrap();
    initialize_database(database.database()).await.unwrap();
    (
        directory,
        ConversationRepository::new(database.database().clone()),
    )
}

fn journal(name: &str) -> CreateConversationJournalRequestDto {
    CreateConversationJournalRequestDto {
        name: name.into(),
        description: None,
        icon: None,
        accent_color: None,
        space_prompt: None,
        tool_preferences_json: None,
    }
}

#[tokio::test]
async fn journal_membership_is_idempotent_and_preserves_conversation_space() {
    let (_directory, repo) = repository().await;
    let conversation = repo.create("Research", None).await.unwrap();
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

/// A pin belongs to the journal entry: the same chat pinned in one journal is
/// not pinned in another, nor in the Chat sidebar, and leaving the journal
/// drops the pin.
#[tokio::test]
async fn a_journal_pin_is_kept_per_journal_entry() {
    let (_directory, repo) = repository().await;
    let conversation = repo.create("Research", None).await.unwrap();
    let id = conversation.id.to_string();
    let reading = repo.create_journal(journal("Reading")).await.unwrap();
    let writing = repo.create_journal(journal("Writing")).await.unwrap();
    for journal in [&reading, &writing] {
        repo.add_conversation_to_journal(AddConversationToJournalRequestDto {
            journal_space_id: journal.id.clone(),
            conversation_id: id.clone(),
        })
        .await
        .unwrap();
    }
    let pin = |journal_space_id: &str, pinned: bool| SetJournalEntryPinnedRequestDto {
        journal_space_id: journal_space_id.to_string(),
        conversation_id: id.clone(),
        pinned,
    };

    repo.set_journal_entry_pinned(pin(&reading.id, true))
        .await
        .unwrap();
    assert_eq!(
        repo.list_journal_entry_pins(&reading.id).await.unwrap(),
        vec![id.clone()]
    );
    assert!(repo
        .list_journal_entry_pins(&writing.id)
        .await
        .unwrap()
        .is_empty());
    let sidebar_pinned: bool =
        sqlx::query_scalar("SELECT is_pinned FROM conversations WHERE id = ?")
            .bind(&id)
            .fetch_one(&repo.pool)
            .await
            .unwrap();
    assert!(!sidebar_pinned);

    repo.set_journal_entry_pinned(pin(&reading.id, false))
        .await
        .unwrap();
    assert!(repo
        .list_journal_entry_pins(&reading.id)
        .await
        .unwrap()
        .is_empty());

    repo.set_journal_entry_pinned(pin(&writing.id, true))
        .await
        .unwrap();
    repo.remove_conversation_from_journal(RemoveConversationFromJournalRequestDto {
        journal_space_id: writing.id.clone(),
        conversation_id: id.clone(),
    })
    .await
    .unwrap();
    assert!(repo
        .list_journal_entry_pins(&writing.id)
        .await
        .unwrap()
        .is_empty());
    assert!(matches!(
        repo.set_journal_entry_pinned(pin(&writing.id, true)).await,
        Err(AppError::NotFound(_))
    ));
    assert!(matches!(
        repo.list_journal_entry_pins("journal_missing").await,
        Err(AppError::NotFound(_))
    ));
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
    let conversation = repo.create("Research", None).await.unwrap();
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
    let filed_in_destination: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM document_space_memberships WHERE space_id = ?")
            .bind(&target.id)
            .fetch_one(&repo.pool)
            .await
            .unwrap();
    assert_eq!(filed_in_destination, 0);
}

#[tokio::test]
async fn deleting_another_conversations_message_is_rejected_without_changing_totals() {
    let (_directory, repo) = repository().await;
    let first = repo.create("First", None).await.unwrap();
    let second = repo.create("Second", None).await.unwrap();
    let message = repo
        .add_message(
            &first.id.to_string(),
            lattice_core::domain::conversation::MessageRole::User,
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

async fn seed_listing(repo: &ConversationRepository) -> Vec<String> {
    let mut ids = Vec::new();
    for (index, title) in ["Alpha one", "Alpha two", "Alpha six", "Beta note"]
        .into_iter()
        .enumerate()
    {
        let conversation = repo.create(title, None).await.unwrap();
        let id = conversation.id.to_string();
        sqlx::query("UPDATE conversations SET updated_at = ? WHERE id = ?")
            .bind(format!("2026-01-0{}T00:00:00Z", index + 1))
            .bind(&id)
            .execute(&repo.pool)
            .await
            .unwrap();
        ids.push(id);
    }
    sqlx::query("UPDATE conversations SET is_archived = 1 WHERE id = ?")
        .bind(&ids[2])
        .execute(&repo.pool)
        .await
        .unwrap();
    ids
}

fn titles(response: &ListConversationsResponseDto) -> Vec<&str> {
    response
        .conversations
        .iter()
        .map(|c| c.title.as_str())
        .collect()
}

fn explorer_query(
    query: Option<&str>,
    include_archived: bool,
    limit: Option<i64>,
    offset: Option<i64>,
) -> ListConversationsExplorerQueryDto {
    ListConversationsExplorerQueryDto {
        space_id: None,
        program_id: None,
        query: query.map(str::to_string),
        saved_only: None,
        bookmarked_only: None,
        pinned_only: None,
        has_message_bookmarks: None,
        include_archived: Some(include_archived),
        limit,
        offset,
    }
}

#[tokio::test]
async fn the_explorer_listing_searches_hides_archived_and_pages() {
    let (_directory, repo) = repository().await;
    seed_listing(&repo).await;

    let found = repo
        .list_conversations_explorer(explorer_query(Some("alpha"), false, None, None))
        .await
        .unwrap();
    assert_eq!(titles(&found), ["Alpha two", "Alpha one"]);

    let with_archived = repo
        .list_conversations_explorer(explorer_query(Some("alpha"), true, None, None))
        .await
        .unwrap();
    assert_eq!(
        titles(&with_archived),
        ["Alpha six", "Alpha two", "Alpha one"]
    );

    let all = repo
        .list_conversations_explorer(explorer_query(None, false, None, None))
        .await
        .unwrap();
    assert_eq!(titles(&all), ["Beta note", "Alpha two", "Alpha one"]);

    let second = repo
        .list_conversations_explorer(explorer_query(Some("alpha"), false, Some(1), Some(1)))
        .await
        .unwrap();
    assert_eq!(titles(&second), ["Alpha one"]);
    let past_end = repo
        .list_conversations_explorer(explorer_query(Some("alpha"), false, Some(1), Some(2)))
        .await
        .unwrap();
    assert!(past_end.conversations.is_empty());
}

fn journal_query(
    journal_space_id: &str,
    query: Option<&str>,
    include_archived: bool,
    limit: Option<i64>,
    offset: Option<i64>,
) -> ListJournalConversationsQueryDto {
    ListJournalConversationsQueryDto {
        journal_space_id: journal_space_id.to_string(),
        query: query.map(str::to_string),
        include_archived: Some(include_archived),
        limit,
        offset,
    }
}

#[tokio::test]
async fn the_journal_listing_searches_hides_archived_pages_and_stays_in_its_journal() {
    let (_directory, repo) = repository().await;
    let ids = seed_listing(&repo).await;
    let reading = repo.create_journal(journal("Reading")).await.unwrap();
    for id in &ids[..3] {
        repo.add_conversation_to_journal(AddConversationToJournalRequestDto {
            journal_space_id: reading.id.clone(),
            conversation_id: id.clone(),
        })
        .await
        .unwrap();
    }

    let found = repo
        .list_journal_conversations(journal_query(&reading.id, Some("alpha"), false, None, None))
        .await
        .unwrap();
    assert_eq!(titles(&found), ["Alpha two", "Alpha one"]);

    let with_archived = repo
        .list_journal_conversations(journal_query(&reading.id, Some("alpha"), true, None, None))
        .await
        .unwrap();
    assert_eq!(
        titles(&with_archived),
        ["Alpha six", "Alpha two", "Alpha one"]
    );

    let beta = repo
        .list_journal_conversations(journal_query(&reading.id, Some("beta"), true, None, None))
        .await
        .unwrap();
    assert!(beta.conversations.is_empty());

    let second = repo
        .list_journal_conversations(journal_query(
            &reading.id,
            Some("alpha"),
            false,
            Some(1),
            Some(1),
        ))
        .await
        .unwrap();
    assert_eq!(titles(&second), ["Alpha one"]);

    assert!(matches!(
        repo.list_journal_conversations(journal_query("journal_missing", None, false, None, None))
            .await,
        Err(AppError::NotFound(_))
    ));
    assert!(matches!(
        repo.list_journal_conversations(journal_query(" ", None, false, None, None))
            .await,
        Err(AppError::InvalidInput(_))
    ));
}
