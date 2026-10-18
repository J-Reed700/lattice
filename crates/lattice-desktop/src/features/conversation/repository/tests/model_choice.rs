//! A chat's own model choice: stored, read back, carried by its branches,
//! and the migration that introduced it.

use super::*;
use crate::features::conversation::repository::ConversationRepository;
use crate::features::conversation::space_dto::{
    CreateConversationJournalRequestDto, CreateConversationSpaceRequestDto,
    ListConversationsExplorerQueryDto, UpdateConversationSpaceRequestDto,
};
use crate::features::conversation::space_repository::ConversationSpaceRepository;
use crate::features::conversation::tangent_dto::CreateTangentRequestDto;
use lattice_core::domain::conversation::ChatModelChoice;
use lattice_core::domain::models::model_ref::{CloudProvider, ModelRef};
use lattice_core::shared::error::AppError;

fn chosen() -> ChatModelChoice {
    ChatModelChoice::Explicit {
        model: ModelRef::Cloud {
            provider: CloudProvider::Anthropic,
            model: "claude-x".into(),
        },
    }
}

async fn thread() -> (SqlitePool, ConversationRepository, String) {
    let pool = create_test_pool().await;
    setup_schema(&pool).await;
    let id = seed_thread(&pool).await;
    (pool.clone(), ConversationRepository::new(pool), id)
}

async fn stored(pool: &SqlitePool, id: &str) -> Option<String> {
    sqlx::query_scalar("SELECT model_choice FROM conversations WHERE id = ?")
        .bind(id)
        .fetch_one(pool)
        .await
        .unwrap()
}

#[tokio::test]
async fn a_new_chat_follows_the_settings_default() {
    let (_, repo, _) = thread().await;
    let created = repo.create("New chat", None).await.unwrap();
    let id = created.id.to_string();

    assert_eq!(created.model_choice, ChatModelChoice::Default);
    assert_eq!(
        repo.model_choice(&id).await.unwrap(),
        ChatModelChoice::Default
    );
    assert_eq!(
        repo.find_by_id(&id).await.unwrap().unwrap().model_choice,
        ChatModelChoice::Default
    );
}

#[tokio::test]
async fn a_choice_is_stored_as_its_model_and_default_as_null() {
    let (pool, repo, id) = thread().await;

    repo.set_model_choice(&id, &chosen()).await.unwrap();
    assert_eq!(repo.model_choice(&id).await.unwrap(), chosen());
    assert_eq!(
        repo.find_by_id(&id).await.unwrap().unwrap().model_choice,
        chosen()
    );
    assert_eq!(
        stored(&pool, &id).await.as_deref(),
        Some(r#"{"kind":"cloud","provider":"anthropic","model":"claude-x"}"#)
    );

    repo.set_model_choice(&id, &ChatModelChoice::Default)
        .await
        .unwrap();
    assert_eq!(stored(&pool, &id).await, None);
}

#[tokio::test]
async fn choosing_a_model_bumps_the_chat_and_an_unknown_chat_is_not_found() {
    let (_, repo, id) = thread().await;
    let before = repo.find_by_id(&id).await.unwrap().unwrap().updated_at;

    repo.set_model_choice(&id, &chosen()).await.unwrap();

    let after = repo.find_by_id(&id).await.unwrap().unwrap().updated_at;
    assert!(after > before);
    assert!(matches!(
        repo.set_model_choice("missing", &chosen()).await,
        Err(AppError::NotFound(_))
    ));
    assert!(matches!(
        repo.model_choice("missing").await,
        Err(AppError::NotFound(_))
    ));
}

#[tokio::test]
async fn an_unreadable_stored_choice_is_an_error_naming_the_chat_never_the_default() {
    let (pool, repo, id) = thread().await;
    sqlx::query("UPDATE conversations SET model_choice = 'local:ornith' WHERE id = ?")
        .bind(&id)
        .execute(&pool)
        .await
        .unwrap();

    for error in [
        repo.model_choice(&id).await.unwrap_err(),
        repo.find_by_id(&id).await.unwrap_err(),
    ] {
        assert!(
            matches!(&error, AppError::Database(message)
                if message.starts_with(&format!("conversation {id} has an unreadable model_choice"))),
            "{error:?}"
        );
    }
}

#[tokio::test]
async fn forks_tangents_and_continuations_keep_the_chats_choice() {
    let (_, repo, id) = thread().await;
    repo.set_model_choice(&id, &chosen()).await.unwrap();

    let (fork, _) = repo
        .fork(&id, Some("m2"), "conv-fork", "Thread · branch")
        .await
        .unwrap();
    let tangent = repo
        .create_tangent(CreateTangentRequestDto {
            conversation_id: id.clone(),
            message_id: "m2".into(),
            selected_text: "a passage".into(),
        })
        .await
        .unwrap();
    repo.create_continuation(&id, "conv-next", "Thread · continued", "Summary", "{}")
        .await
        .unwrap();

    for branch in [fork, tangent.conversation_id, "conv-next".to_string()] {
        assert_eq!(
            repo.model_choice(&branch).await.unwrap(),
            chosen(),
            "{branch}"
        );
    }
}

/// Every migration before the model-choice one, so a database can be filled
/// the way the previous release left it.
async fn migrated_before_model_choice(pool: &SqlitePool) -> tempfile::TempDir {
    const MODEL_CHOICE: &str = "20261016120000";
    let dir = tempfile::tempdir().unwrap();
    for entry in std::fs::read_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/migrations")).unwrap() {
        let path = entry.unwrap().path();
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        if name.as_str() < MODEL_CHOICE {
            std::fs::copy(&path, dir.path().join(&name)).unwrap();
        }
    }
    sqlx::migrate::Migrator::new(dir.path())
        .await
        .unwrap()
        .run(pool)
        .await
        .unwrap();
    dir
}

async fn columns(pool: &SqlitePool, table: &str) -> Vec<String> {
    sqlx::query_scalar(&format!("SELECT name FROM pragma_table_info('{table}')"))
        .fetch_all(pool)
        .await
        .unwrap()
}

#[tokio::test]
async fn the_model_choice_migration_keeps_every_chat_and_drops_the_dead_model_columns() {
    let pool = create_test_pool().await;
    let _earlier = migrated_before_model_choice(&pool).await;
    for statement in [
        "INSERT INTO conversation_spaces (id, name, default_model_name) VALUES ('space-a', 'Research', 'qwen3:8b')",
        "INSERT INTO journals (id, name, default_model_name) VALUES ('journal-a', 'Notes', 'qwen3:8b')",
        "INSERT INTO conversations (id, title, model_name, space_id, created_at, updated_at, next_message_sequence) \
         VALUES ('conv-old', 'Lithium recycling', 'qwen3:8b', 'space-a', \
                 '2026-09-01T10:00:00Z', '2026-09-01T10:00:00Z', 2)",
        "INSERT INTO conversation_messages (id, conversation_id, role, content, tokens, created_at, status, sequence) \
         VALUES ('m-old', 'conv-old', 'user', 'How are packs shredded?', 5, '2026-09-01T10:00:00Z', 'completed', 1)",
    ] {
        sqlx::query(statement).execute(&pool).await.unwrap();
    }

    sqlx::migrate!("./migrations").run(&pool).await.unwrap();

    let conversation_columns = columns(&pool, "conversations").await;
    assert!(conversation_columns.contains(&"model_choice".to_string()));
    assert!(!conversation_columns.contains(&"model_name".to_string()));
    for table in ["conversation_spaces", "journals"] {
        assert!(
            !columns(&pool, table)
                .await
                .contains(&"default_model_name".to_string()),
            "{table}"
        );
    }

    // The chat and its message survive, and the chat follows the default.
    let repo = ConversationRepository::new(pool.clone());
    let migrated = repo.find_by_id("conv-old").await.unwrap().unwrap();
    assert_eq!(migrated.title, "Lithium recycling");
    assert_eq!(migrated.model_choice, ChatModelChoice::Default);
    assert_eq!(repo.get_messages("conv-old").await.unwrap().len(), 1);
    let listed = repo
        .list_conversations_explorer(ListConversationsExplorerQueryDto {
            space_id: Some("space-a".into()),
            query: None,
            saved_only: None,
            bookmarked_only: None,
            pinned_only: None,
            has_message_bookmarks: None,
            include_archived: None,
            limit: None,
            offset: None,
        })
        .await
        .unwrap();
    assert_eq!(listed.conversations.len(), 1);
    assert_eq!(
        listed.conversations[0].model_choice,
        ChatModelChoice::Default
    );

    // Search still indexes: the old message, and a message and title written
    // after the migration.
    repo.add_message(
        "conv-old",
        lattice_core::domain::conversation::MessageRole::Assistant,
        "Hydrometallurgy leaches the black mass.",
        7,
        None,
    )
    .await
    .unwrap();
    repo.update_title("conv-old", "Battery recovery")
        .await
        .unwrap();
    let hits = |term: &'static str| {
        let pool = pool.clone();
        async move {
            sqlx::query_scalar::<_, i64>(
                "SELECT count(*) FROM conversation_search_fts WHERE conversation_search_fts MATCH ?",
            )
            .bind(term)
            .fetch_one(&pool)
            .await
            .unwrap()
        }
    };
    assert_eq!(hits("shredded").await, 1);
    assert_eq!(hits("hydrometallurgy").await, 1);
    assert_eq!(hits("recovery").await, 1);

    // Spaces and journals round-trip without the dropped tier.
    let spaces = ConversationSpaceRepository::new(pool.clone());
    let created = spaces
        .create(CreateConversationSpaceRequestDto {
            name: "Fresh".into(),
            description: Some("New space".into()),
            icon: None,
            accent_color: None,
            space_prompt: Some("Answer briefly.".into()),
            tool_preferences_json: None,
        })
        .await
        .unwrap();
    let updated = repo
        .update_conversation_space(UpdateConversationSpaceRequestDto {
            space_id: created.id.clone(),
            name: Some("Renamed".into()),
            description: None,
            icon: None,
            accent_color: None,
            space_prompt: None,
            tool_preferences_json: None,
            is_archived: None,
            sort_order: None,
        })
        .await
        .unwrap();
    assert_eq!(updated.name, "Renamed");
    assert_eq!(updated.space_prompt.as_deref(), Some("Answer briefly."));
    assert!(repo
        .list_conversation_spaces()
        .await
        .unwrap()
        .iter()
        .any(|space| space.id == "space-a"));
    let journal = repo
        .create_journal(CreateConversationJournalRequestDto {
            name: "Fresh journal".into(),
            description: None,
            icon: None,
            accent_color: None,
            space_prompt: None,
            tool_preferences_json: None,
        })
        .await
        .unwrap();
    assert!(repo
        .list_journals()
        .await
        .unwrap()
        .iter()
        .any(|listed| listed.id == journal.id));
    assert!(repo
        .list_journals()
        .await
        .unwrap()
        .iter()
        .any(|listed| listed.id == "journal-a"));
}
