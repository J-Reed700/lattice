use super::*;
use crate::features::conversation::space_repository::ConversationSpaceRepository;
use crate::infrastructure::persistence::database::initialize_database;
use crate::tests::sqlite::file_options;
use lattice_core::shared::persistence::Database;
use sqlx::sqlite::SqlitePoolOptions;
use sqlx::SqlitePool;

async fn repository() -> (tempfile::TempDir, SqlitePool, ConversationRepository) {
    let directory = tempfile::tempdir().unwrap();
    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(file_options(directory.path().join("workspace.db")))
        .await
        .unwrap();
    let db = Database::new(pool.clone());
    initialize_database(&db).await.unwrap();
    (directory, pool, ConversationRepository::new(db))
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
    let (_directory, pool, repo) = repository().await;
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
    .fetch_one(&pool)
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
    let (_directory, pool, repo) = repository().await;
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
            .fetch_one(&pool)
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
    let (_directory, pool, repo) = repository().await;
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
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 0);
}

/// Moving a chat used to file every document it had cited into the destination
/// space, where all the other chats then searched them.
#[tokio::test]
async fn moving_a_conversation_does_not_file_its_documents_into_the_destination() {
    let (_directory, pool, repo) = repository().await;
    let conversation = repo.create("Research", None).await.unwrap();
    sqlx::query(
        "INSERT INTO documents (id, file_path, file_name, size_bytes, modified_at, checksum) \
         VALUES ('doc-cited', '/vault/cited.pdf', 'cited.pdf', 1, '2026-09-01T10:00:00Z', 'sum')",
    )
    .execute(&pool)
    .await
    .unwrap();
    repo.add_document_reference(&conversation.id.to_string(), "doc-cited", None, Some(0.9))
        .await
        .unwrap();
    let target = ConversationSpaceRepository::new(repo.db.clone())
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
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(filed_in_destination, 0);
}

#[tokio::test]
async fn deleting_another_conversations_message_is_rejected_without_changing_totals() {
    let (_directory, _, repo) = repository().await;
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

async fn seed_listing(pool: &SqlitePool, repo: &ConversationRepository) -> Vec<String> {
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
            .execute(pool)
            .await
            .unwrap();
        ids.push(id);
    }
    sqlx::query("UPDATE conversations SET is_archived = 1 WHERE id = ?")
        .bind(&ids[2])
        .execute(pool)
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
    let (_directory, pool, repo) = repository().await;
    seed_listing(&pool, &repo).await;

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
    let (_directory, pool, repo) = repository().await;
    let ids = seed_listing(&pool, &repo).await;
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

async fn bookmarked_message(
    pool: &SqlitePool,
    repo: &ConversationRepository,
    conversation_id: &str,
    content: &str,
    created_at: &str,
) -> String {
    let message = repo
        .add_message(
            conversation_id,
            lattice_core::domain::conversation::MessageRole::User,
            content,
            1,
            None,
        )
        .await
        .unwrap();
    let message_id = message.id.to_string();
    repo.bookmark_conversation_message(BookmarkConversationMessageRequestDto {
        conversation_id: conversation_id.to_string(),
        message_id: message_id.clone(),
        title: None,
        note: None,
    })
    .await
    .unwrap();
    sqlx::query("UPDATE conversation_message_bookmarks SET created_at = ? WHERE message_id = ?")
        .bind(created_at)
        .bind(&message_id)
        .execute(pool)
        .await
        .unwrap();
    message_id
}

fn bookmark_query(query: Option<&str>) -> ListMessageBookmarksQueryDto {
    ListMessageBookmarksQueryDto {
        conversation_id: None,
        query: query.map(str::to_string),
        limit: None,
        offset: None,
    }
}

#[tokio::test]
async fn bookmark_search_ranks_each_bookmark_by_its_own_match() {
    let (_directory, pool, repo) = repository().await;
    let field_log = repo.create("Field log", None).await.unwrap().id.to_string();
    let shopping = repo.create("Shopping", None).await.unwrap().id.to_string();
    let strong = bookmarked_message(
        &pool,
        &repo,
        &field_log,
        "kestrel kestrel kestrel",
        "2026-01-01T00:00:00Z",
    )
    .await;
    let weak = bookmarked_message(
        &pool,
        &repo,
        &field_log,
        "a kestrel flew over the quiet harbor today",
        "2026-01-02T00:00:00Z",
    )
    .await;
    let unrelated = bookmarked_message(
        &pool,
        &repo,
        &shopping,
        "bread and milk",
        "2026-01-03T00:00:00Z",
    )
    .await;
    let message_ids = |response: ListMessageBookmarksResponseDto| {
        response
            .bookmarks
            .into_iter()
            .map(|bookmark| bookmark.message_id)
            .collect::<Vec<_>>()
    };

    let found = repo
        .list_message_bookmarks(bookmark_query(Some("kestrel")))
        .await
        .unwrap();
    assert_eq!(message_ids(found), [strong.clone(), weak.clone()]);

    let none = repo
        .list_message_bookmarks(bookmark_query(Some("osprey")))
        .await
        .unwrap();
    assert!(none.bookmarks.is_empty());

    let all = repo
        .list_message_bookmarks(bookmark_query(None))
        .await
        .unwrap();
    assert_eq!(message_ids(all), [unrelated, weak, strong]);
}

#[tokio::test]
async fn a_better_match_lists_before_a_newer_weaker_one() {
    let (_directory, pool, repo) = repository().await;
    let older = repo
        .create("Kestrel kestrel kestrel", None)
        .await
        .unwrap()
        .id
        .to_string();
    let newer = repo
        .create("Harbor notes", None)
        .await
        .unwrap()
        .id
        .to_string();
    repo.add_message(
        &newer,
        lattice_core::domain::conversation::MessageRole::User,
        "a kestrel flew over the quiet harbor today",
        1,
        None,
    )
    .await
    .unwrap();
    for filler in ["Bread", "Milk", "Cheese", "Apples", "Tea", "Rice"] {
        repo.create(filler, None).await.unwrap();
    }
    for (id, updated_at) in [
        (&older, "2026-01-01T00:00:00Z"),
        (&newer, "2026-01-02T00:00:00Z"),
    ] {
        sqlx::query("UPDATE conversations SET updated_at = ? WHERE id = ?")
            .bind(updated_at)
            .bind(id)
            .execute(&pool)
            .await
            .unwrap();
    }

    let found = repo
        .list_conversations_explorer(explorer_query(Some("kestrel"), false, None, None))
        .await
        .unwrap();

    assert_eq!(titles(&found), ["Kestrel kestrel kestrel", "Harbor notes"]);
}

#[tokio::test]
async fn each_explorer_scope_filter_lists_only_its_own_conversations() {
    let (_directory, pool, repo) = repository().await;
    let space = ConversationSpaceRepository::new(repo.db.clone())
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
    sqlx::query(
        "INSERT INTO learning_programs
            (id, title, goal, status, revision, prior_knowledge, minutes_per_session, model_name, created_at)
         VALUES ('program-1', 'Course', 'Learn', 'active', 1, 'none', 30, 'model', 0)",
    )
    .execute(&pool)
    .await
    .unwrap();
    let mut ids = std::collections::HashMap::new();
    for title in [
        "In space",
        "In program",
        "Saved",
        "Bookmarked",
        "Pinned",
        "Message bookmark",
        "Plain",
    ] {
        let id = repo.create(title, None).await.unwrap().id.to_string();
        ids.insert(title, id);
    }
    for (title, column, value) in [
        ("In space", "space_id", space.id.as_str()),
        ("In program", "program_id", "program-1"),
    ] {
        sqlx::query(&format!(
            "UPDATE conversations SET {column} = ? WHERE id = ?"
        ))
        .bind(value)
        .bind(&ids[title])
        .execute(&pool)
        .await
        .unwrap();
    }
    for (title, column) in [
        ("Saved", "is_saved"),
        ("Bookmarked", "is_bookmarked"),
        ("Pinned", "is_pinned"),
    ] {
        sqlx::query(&format!(
            "UPDATE conversations SET {column} = 1 WHERE id = ?"
        ))
        .bind(&ids[title])
        .execute(&pool)
        .await
        .unwrap();
    }
    bookmarked_message(
        &pool,
        &repo,
        &ids["Message bookmark"],
        "remember this",
        "2026-01-01T00:00:00Z",
    )
    .await;
    let base = || explorer_query(None, false, None, None);
    let list = |query: ListConversationsExplorerQueryDto| {
        let repo = &repo;
        async move {
            let mut found = titles(&repo.list_conversations_explorer(query).await.unwrap())
                .into_iter()
                .map(str::to_string)
                .collect::<Vec<_>>();
            found.sort();
            found
        }
    };

    assert_eq!(
        list(ListConversationsExplorerQueryDto {
            space_id: Some(space.id.clone()),
            ..base()
        })
        .await,
        ["In space"]
    );
    assert_eq!(
        list(ListConversationsExplorerQueryDto {
            program_id: Some("program-1".into()),
            ..base()
        })
        .await,
        ["In program"]
    );
    assert_eq!(
        list(ListConversationsExplorerQueryDto {
            saved_only: Some(true),
            ..base()
        })
        .await,
        ["Saved"]
    );
    assert_eq!(
        list(ListConversationsExplorerQueryDto {
            bookmarked_only: Some(true),
            ..base()
        })
        .await,
        ["Bookmarked"]
    );
    assert_eq!(
        list(ListConversationsExplorerQueryDto {
            pinned_only: Some(true),
            ..base()
        })
        .await,
        ["Pinned"]
    );
    assert_eq!(
        list(ListConversationsExplorerQueryDto {
            has_message_bookmarks: Some(true),
            ..base()
        })
        .await,
        ["Message bookmark"]
    );
}

async fn many_conversations(repo: &ConversationRepository, count: usize) -> Vec<String> {
    let mut ids = Vec::new();
    for index in 0..count {
        let conversation = repo.create(&format!("Chat {index}"), None).await.unwrap();
        ids.push(conversation.id.to_string());
    }
    ids
}

#[tokio::test]
async fn the_explorer_listing_without_a_limit_returns_every_conversation() {
    let (_directory, repo) = repository().await;
    many_conversations(&repo, 101).await;

    let all = repo
        .list_conversations_explorer(explorer_query(None, false, None, None))
        .await
        .unwrap();
    assert_eq!(all.conversations.len(), 101);

    let past_the_first_hundred = repo
        .list_conversations_explorer(explorer_query(None, false, None, Some(100)))
        .await
        .unwrap();
    assert_eq!(past_the_first_hundred.conversations.len(), 1);
}

#[tokio::test]
async fn an_explicit_listing_limit_returns_at_most_that_many_and_offset_pages() {
    let (_directory, repo) = repository().await;
    many_conversations(&repo, 5).await;

    let first = repo
        .list_conversations_explorer(explorer_query(None, false, Some(2), None))
        .await
        .unwrap();
    let second = repo
        .list_conversations_explorer(explorer_query(None, false, Some(2), Some(2)))
        .await
        .unwrap();
    let rest = repo
        .list_conversations_explorer(explorer_query(None, false, None, Some(2)))
        .await
        .unwrap();
    assert_eq!(
        (first.conversations.len(), second.conversations.len()),
        (2, 2)
    );
    assert_eq!(rest.conversations.len(), 3);
    assert_ne!(first.conversations[0].id, second.conversations[0].id);
}

#[tokio::test]
async fn a_listing_limit_below_one_or_a_negative_offset_is_invalid_input() {
    let (_directory, repo) = repository().await;
    let journal_id = repo.create_journal(journal("Reading")).await.unwrap().id;

    for (limit, offset) in [(Some(0), None), (Some(-5), None), (None, Some(-1))] {
        assert!(matches!(
            repo.list_conversations_explorer(explorer_query(None, false, limit, offset))
                .await,
            Err(AppError::InvalidInput(_))
        ));
        assert!(matches!(
            repo.list_journal_conversations(journal_query(&journal_id, None, false, limit, offset))
                .await,
            Err(AppError::InvalidInput(_))
        ));
        let mut bookmarks = bookmark_query(None);
        bookmarks.limit = limit;
        bookmarks.offset = offset;
        assert!(matches!(
            repo.list_message_bookmarks(bookmarks).await,
            Err(AppError::InvalidInput(_))
        ));
    }
}

#[tokio::test]
async fn the_journal_listing_without_a_limit_returns_every_entry() {
    let (_directory, repo) = repository().await;
    let reading = repo.create_journal(journal("Reading")).await.unwrap();
    for id in many_conversations(&repo, 121).await {
        repo.add_conversation_to_journal(AddConversationToJournalRequestDto {
            journal_space_id: reading.id.clone(),
            conversation_id: id,
        })
        .await
        .unwrap();
    }

    let all = repo
        .list_journal_conversations(journal_query(&reading.id, None, false, None, None))
        .await
        .unwrap();
    assert_eq!(all.conversations.len(), 121);
}

#[tokio::test]
async fn the_bookmark_listing_returns_every_bookmark_without_a_limit_and_honours_a_large_one() {
    let (_directory, repo) = repository().await;
    let conversation = repo.create("Notes", None).await.unwrap().id.to_string();
    for index in 0..201 {
        bookmarked_message(
            &repo,
            &conversation,
            &format!("note {index}"),
            "2026-01-01T00:00:00Z",
        )
        .await;
    }

    let all = repo
        .list_message_bookmarks(bookmark_query(None))
        .await
        .unwrap();
    assert_eq!(all.bookmarks.len(), 201);

    let mut large = bookmark_query(None);
    large.limit = Some(250);
    let listed = repo.list_message_bookmarks(large).await.unwrap();
    assert_eq!(listed.bookmarks.len(), 201);

    let mut tail = bookmark_query(None);
    tail.limit = Some(2);
    tail.offset = Some(200);
    assert_eq!(
        repo.list_message_bookmarks(tail)
            .await
            .unwrap()
            .bookmarks
            .len(),
        1
    );
}
