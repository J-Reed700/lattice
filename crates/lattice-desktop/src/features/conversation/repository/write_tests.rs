//! A write that reads first decides on what it reads once the write lock is
//! its own, and a write of several statements lands whole or not at all.
use super::*;
use crate::features::conversation::message_bookmark_dto::*;
use crate::features::conversation::space_dto::*;
use crate::infrastructure::persistence::database::{initialize_database, DatabaseConnection};
use lattice_core::domain::conversation::MessageRole;
use lattice_core::shared::error::AppError;
use lattice_core::shared::persistence::Transactions;
use std::time::Duration;

const HELD: Duration = Duration::from_millis(300);

/// Opens a migrated database with two connections already open, so a read never waits for a connection to open behind another writer.
async fn open() -> (tempfile::TempDir, ConversationRepository) {
    let directory = tempfile::tempdir().unwrap();
    let database = DatabaseConnection::new(directory.path().join("writes.db"))
        .await
        .unwrap();
    initialize_database(database.pool()).await.unwrap();
    let first = database.pool().acquire().await.unwrap();
    let second = database.pool().acquire().await.unwrap();
    drop(first);
    drop(second);
    (
        directory,
        ConversationRepository::new(database.pool().clone()),
    )
}

/// Another connection takes the write lock now, runs `sql`, and commits after `HELD`.
async fn another_writer(pool: &SqlitePool, sql: &str) -> tokio::task::JoinHandle<()> {
    let mut tx = pool.begin_write().await.unwrap();
    sqlx::query(sql).execute(&mut *tx).await.unwrap();
    tokio::spawn(async move {
        tokio::time::sleep(HELD).await;
        tx.commit().await.unwrap();
    })
}

async fn run(pool: &SqlitePool, sql: &str) {
    sqlx::query(sql).execute(pool).await.unwrap();
}

async fn seed_document(pool: &SqlitePool, id: &str) {
    sqlx::query(
        "INSERT INTO documents (id, file_path, file_name, size_bytes, modified_at, checksum) VALUES (?, ?, ?, 1, '2026-09-01T10:00:00Z', ?)",
    )
    .bind(id)
    .bind(format!("/fixtures/{id}.md"))
    .bind(format!("{id}.md"))
    .bind(format!("checksum-{id}"))
    .execute(pool)
    .await
    .unwrap();
}

async fn count(pool: &SqlitePool, sql: &str) -> i64 {
    sqlx::query_scalar(sql).fetch_one(pool).await.unwrap()
}

#[tokio::test]
async fn a_message_status_is_not_kept_when_the_conversation_timestamp_is_refused() {
    let (_directory, repo) = open().await;
    let conversation = repo.create("Chat", None).await.unwrap();
    let message = repo
        .add_message(
            &conversation.id.to_string(),
            MessageRole::User,
            "hi",
            1,
            None,
        )
        .await
        .unwrap();
    run(
        &repo.pool,
        "CREATE TRIGGER refuse_for_test BEFORE UPDATE OF updated_at ON conversations BEGIN SELECT RAISE(ABORT, 'refused for the test'); END",
    )
    .await;

    let outcome = repo.update_message_status(&message.id, "failed").await;

    assert!(outcome.is_err());
    let status: String =
        sqlx::query_scalar("SELECT status FROM conversation_messages WHERE id = ?")
            .bind(&message.id)
            .fetch_one(&repo.pool)
            .await
            .unwrap();
    assert_eq!(status, "completed");
}

#[tokio::test]
async fn bookmarking_a_message_deleted_meanwhile_finds_it_gone() {
    let (_directory, repo) = open().await;
    let conversation = repo.create("Chat", None).await.unwrap();
    let message = repo
        .add_message(
            &conversation.id.to_string(),
            MessageRole::User,
            "hi",
            1,
            None,
        )
        .await
        .unwrap();
    let other = another_writer(
        &repo.pool,
        &format!(
            "DELETE FROM conversation_messages WHERE id = '{}'",
            message.id
        ),
    )
    .await;

    let outcome = repo
        .bookmark_conversation_message(BookmarkConversationMessageRequestDto {
            conversation_id: conversation.id.to_string(),
            message_id: message.id.clone(),
            title: None,
            note: None,
        })
        .await;

    other.await.unwrap();
    assert!(matches!(outcome, Err(AppError::NotFound(_))), "{outcome:?}");
}

#[tokio::test]
async fn adding_a_conversation_deleted_meanwhile_to_a_journal_finds_it_gone() {
    let (_directory, repo) = open().await;
    let conversation = repo.create("Chat", None).await.unwrap();
    run(
        &repo.pool,
        "INSERT INTO journals (id, name) VALUES ('journal-1', 'Reading')",
    )
    .await;
    let other = another_writer(
        &repo.pool,
        &format!("DELETE FROM conversations WHERE id = '{}'", conversation.id),
    )
    .await;

    let outcome = repo
        .add_conversation_to_journal(AddConversationToJournalRequestDto {
            journal_space_id: "journal-1".into(),
            conversation_id: conversation.id.to_string(),
        })
        .await;

    other.await.unwrap();
    assert!(matches!(outcome, Err(AppError::NotFound(_))), "{outcome:?}");
}

#[tokio::test]
async fn removing_an_owner_while_the_other_owner_is_removed_keeps_one() {
    let (_directory, repo) = open().await;
    run(
        &repo.pool,
        "INSERT INTO conversation_spaces (id, name) VALUES ('space-1', 'Team')",
    )
    .await;
    for member in ["owner-a", "owner-b"] {
        run(
            &repo.pool,
            &format!("INSERT INTO collaborator_profiles (id, display_name) VALUES ('{member}', '{member}')"),
        )
        .await;
        run(
            &repo.pool,
            &format!("INSERT INTO conversation_space_members (space_id, member_id, role) VALUES ('space-1', '{member}', 'owner')"),
        )
        .await;
    }
    let other = another_writer(
        &repo.pool,
        "DELETE FROM conversation_space_members WHERE space_id = 'space-1' AND member_id = 'owner-b'",
    )
    .await;

    let outcome = repo
        .remove_conversation_space_member(RemoveConversationSpaceMemberRequestDto {
            space_id: "space-1".into(),
            member_id: "owner-a".into(),
        })
        .await;

    other.await.unwrap();
    assert!(
        matches!(outcome, Err(AppError::InvalidInput(_))),
        "{outcome:?}"
    );
    assert_eq!(
        count(
            &repo.pool,
            "SELECT COUNT(*) FROM conversation_space_members"
        )
        .await,
        1
    );
}

#[tokio::test]
async fn assigning_a_document_deleted_meanwhile_to_a_space_finds_it_gone() {
    let (_directory, repo) = open().await;
    run(
        &repo.pool,
        "INSERT INTO conversation_spaces (id, name) VALUES ('space-1', 'Team')",
    )
    .await;
    seed_document(&repo.pool, "doc-1").await;
    let other = another_writer(&repo.pool, "DELETE FROM documents WHERE id = 'doc-1'").await;

    let outcome = repo
        .set_document_space_membership("doc-1".into(), "space-1".into(), true)
        .await;

    other.await.unwrap();
    assert!(matches!(outcome, Err(AppError::NotFound(_))), "{outcome:?}");
}

#[tokio::test]
async fn assigning_documents_to_a_space_lands_whole_or_not_at_all() {
    let (_directory, repo) = open().await;
    run(
        &repo.pool,
        "INSERT INTO conversation_spaces (id, name) VALUES ('space-1', 'Team')",
    )
    .await;
    let ids: Vec<String> = (0..=250).map(|n| format!("doc-{n:03}")).collect();
    for id in &ids {
        seed_document(&repo.pool, id).await;
    }
    run(
        &repo.pool,
        "CREATE TRIGGER refuse_for_test BEFORE INSERT ON document_space_memberships WHEN NEW.document_id = 'doc-250' BEGIN SELECT RAISE(ABORT, 'refused for the test'); END",
    )
    .await;

    let outcome = repo
        .set_documents_space_membership(ids, "space-1".into(), true)
        .await;

    assert!(outcome.is_err());
    assert_eq!(
        count(
            &repo.pool,
            "SELECT COUNT(*) FROM document_space_memberships"
        )
        .await,
        0,
        "the first batch of 250 must not stay when the second is refused"
    );
}

#[tokio::test]
async fn assigning_documents_one_of_which_is_deleted_meanwhile_finds_it_gone() {
    let (_directory, repo) = open().await;
    run(
        &repo.pool,
        "INSERT INTO conversation_spaces (id, name) VALUES ('space-1', 'Team')",
    )
    .await;
    seed_document(&repo.pool, "doc-a").await;
    seed_document(&repo.pool, "doc-b").await;
    let other = another_writer(&repo.pool, "DELETE FROM documents WHERE id = 'doc-b'").await;

    let outcome = repo
        .set_documents_space_membership(
            vec!["doc-a".into(), "doc-b".into()],
            "space-1".into(),
            true,
        )
        .await;

    other.await.unwrap();
    assert!(matches!(outcome, Err(AppError::NotFound(_))), "{outcome:?}");
}

#[tokio::test]
async fn a_title_is_not_kept_when_the_system_prompt_change_is_refused() {
    let (_directory, repo) = open().await;
    let conversation = repo.create("Chat", None).await.unwrap();
    run(
        &repo.pool,
        "CREATE TRIGGER refuse_for_test BEFORE UPDATE OF system_prompt ON conversations BEGIN SELECT RAISE(ABORT, 'refused for the test'); END",
    )
    .await;

    let outcome = repo
        .update(
            &conversation.id.to_string(),
            Some("Renamed"),
            Some(Some("Be brief")),
            None,
            None,
        )
        .await;

    assert!(outcome.is_err());
    let title: String = sqlx::query_scalar("SELECT title FROM conversations WHERE id = ?")
        .bind(conversation.id.to_string())
        .fetch_one(&repo.pool)
        .await
        .unwrap();
    assert_eq!(title, "Chat");
}

fn journal_not_found<T>(outcome: &Result<T, AppError>) -> bool {
    matches!(outcome, Err(AppError::NotFound(message)) if message.starts_with("Journal not found"))
}

#[tokio::test]
async fn removing_a_conversation_from_a_journal_deleted_meanwhile_finds_the_journal_gone() {
    let (_directory, repo) = open().await;
    let conversation = repo.create("Chat", None).await.unwrap();
    run(
        &repo.pool,
        "INSERT INTO journals (id, name) VALUES ('journal-1', 'Reading')",
    )
    .await;
    let other = another_writer(&repo.pool, "DELETE FROM journals WHERE id = 'journal-1'").await;

    let outcome = repo
        .remove_conversation_from_journal(RemoveConversationFromJournalRequestDto {
            journal_space_id: "journal-1".into(),
            conversation_id: conversation.id.to_string(),
        })
        .await;

    other.await.unwrap();
    assert!(journal_not_found(&outcome), "{outcome:?}");
}

#[tokio::test]
async fn pinning_a_journal_entry_in_a_journal_deleted_meanwhile_finds_the_journal_gone() {
    let (_directory, repo) = open().await;
    let conversation = repo.create("Chat", None).await.unwrap();
    run(
        &repo.pool,
        "INSERT INTO journals (id, name) VALUES ('journal-1', 'Reading')",
    )
    .await;
    let other = another_writer(&repo.pool, "DELETE FROM journals WHERE id = 'journal-1'").await;

    let outcome = repo
        .set_journal_entry_pinned(SetJournalEntryPinnedRequestDto {
            journal_space_id: "journal-1".into(),
            conversation_id: conversation.id.to_string(),
            pinned: true,
        })
        .await;

    other.await.unwrap();
    assert!(journal_not_found(&outcome), "{outcome:?}");
}

#[tokio::test]
async fn adding_a_member_to_a_space_deleted_meanwhile_finds_it_gone() {
    let (_directory, repo) = open().await;
    run(
        &repo.pool,
        "INSERT INTO conversation_spaces (id, name) VALUES ('space-1', 'Team')",
    )
    .await;
    let other = another_writer(
        &repo.pool,
        "DELETE FROM conversation_spaces WHERE id = 'space-1'",
    )
    .await;

    let outcome = repo
        .upsert_conversation_space_member(UpsertConversationSpaceMemberRequestDto {
            space_id: "space-1".into(),
            member_id: "member-1".into(),
            display_name: None,
            email: None,
            avatar_url: None,
            role: "editor".into(),
        })
        .await;

    other.await.unwrap();
    assert!(matches!(outcome, Err(AppError::NotFound(_))), "{outcome:?}");
}

#[tokio::test]
async fn moving_a_conversation_to_a_space_deleted_meanwhile_finds_it_gone() {
    let (_directory, repo) = open().await;
    let conversation = repo.create("Chat", None).await.unwrap();
    run(
        &repo.pool,
        "INSERT INTO conversation_spaces (id, name) VALUES ('space-1', 'Team')",
    )
    .await;
    let other = another_writer(
        &repo.pool,
        "DELETE FROM conversation_spaces WHERE id = 'space-1'",
    )
    .await;

    let outcome = repo
        .move_conversation_to_space(MoveConversationToSpaceRequestDto {
            conversation_id: conversation.id.to_string(),
            space_id: "space-1".into(),
        })
        .await;

    other.await.unwrap();
    assert!(matches!(outcome, Err(AppError::NotFound(_))), "{outcome:?}");
}
