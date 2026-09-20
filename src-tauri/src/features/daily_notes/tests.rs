//! Repository + capture-rule tests for daily notes.
//!
//! The repository runs against a real in-memory SQLite database with the real
//! migrations applied (the `features::references::tests` pattern); the capture
//! rules are exercised through the pure helpers, which need neither a
//! `Container` nor a pool.

#![cfg(test)]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]

use sqlx::SqlitePool;

use super::commands::{appended_capture, capture_snippet};
use super::repository::{DailyNotesRepository, WorkspaceNoteRecord};
use crate::shared::error::AppError;

async fn fresh_pool() -> SqlitePool {
    let pool = SqlitePool::connect(":memory:").await.unwrap();
    sqlx::migrate!("./migrations").run(&pool).await.unwrap();
    // The app turns this on at connection time; the cascade from a deleted
    // journal to its pages is silently a no-op without it.
    sqlx::query("PRAGMA foreign_keys = ON")
        .execute(&pool)
        .await
        .unwrap();
    pool
}

async fn fresh_repository() -> DailyNotesRepository {
    DailyNotesRepository::new(fresh_pool().await)
}

async fn insert_journal(pool: &SqlitePool, id: &str, name: &str) {
    sqlx::query("INSERT INTO journals (id, name) VALUES (?, ?)")
        .bind(id)
        .bind(name)
        .execute(pool)
        .await
        .unwrap();
}

fn record(id: &str, title: &str, content: &str, updated_at: &str) -> WorkspaceNoteRecord {
    WorkspaceNoteRecord {
        id: id.to_string(),
        title: title.to_string(),
        journal_id: None,
        content: content.to_string(),
        linked_document_ids: "[]".to_string(),
        linked_conversation_ids: "[]".to_string(),
        highlights_json: "[]".to_string(),
        sticky_notes_json: "[]".to_string(),
        conversation_snapshots_json: "[]".to_string(),
        created_at: updated_at.to_string(),
        updated_at: updated_at.to_string(),
    }
}

#[tokio::test]
async fn empty_content_is_rejected_before_any_page_is_touched() {
    for input in ["", "   ", "\n\t  \n"] {
        match capture_snippet(input) {
            Err(AppError::InvalidInput(message)) => {
                assert_eq!(message, "Nothing to capture");
            }
            other => panic!("expected InvalidInput for {input:?}, got {other:?}"),
        }
    }
}

#[test]
fn capture_snippet_trims_what_it_keeps() {
    assert_eq!(capture_snippet("  keep this  ").unwrap(), "keep this");
}

#[test]
fn appending_to_an_empty_page_writes_the_snippet_alone() {
    assert_eq!(appended_capture("", "first thought"), "first thought");
    assert_eq!(appended_capture("   \n ", "first thought"), "first thought");
}

#[test]
fn appending_to_a_written_page_keeps_one_blank_line() {
    assert_eq!(appended_capture("earlier", "later"), "earlier\n\nlater");
}

#[tokio::test]
async fn capture_creates_todays_page_when_it_does_not_exist() {
    let repository = fresh_repository().await;

    // The create branch: nothing with today's title is stored yet.
    assert!(repository
        .find_by_title("Daily Notes · Saturday, September 6", None)
        .await
        .unwrap()
        .is_none());

    repository
        .insert(&record(
            "note_today",
            "Daily Notes · Saturday, September 6",
            "",
            "2026-09-06T09:00:00.000Z",
        ))
        .await
        .unwrap();

    let found = repository
        .find_by_title("Daily Notes · Saturday, September 6", None)
        .await
        .unwrap()
        .expect("today's page");
    assert_eq!(found.id, "note_today");
}

#[tokio::test]
async fn capture_appends_to_todays_page_and_ignores_a_newer_unrelated_page() {
    let repository = fresh_repository().await;

    repository
        .insert(&record(
            "note_today",
            "Daily Notes · Saturday, September 6",
            "earlier",
            "2026-09-06T09:00:00.000Z",
        ))
        .await
        .unwrap();
    // A week synthesis written after today's page. The old rule ("append to the
    // most recent page") would have put the capture here.
    repository
        .insert(&record(
            "note_week",
            "Week of Sep 1",
            "## Journal Synthesis",
            "2026-09-06T18:00:00.000Z",
        ))
        .await
        .unwrap();

    assert_eq!(
        repository.most_recent().await.unwrap().unwrap().id,
        "note_week"
    );

    let target = repository
        .find_by_title("Daily Notes · Saturday, September 6", None)
        .await
        .unwrap()
        .expect("today's page");
    assert_eq!(target.id, "note_today");

    let mut appended = target;
    appended.content = appended_capture(&appended.content, "later");
    repository.update(&appended).await.unwrap();

    let stored = repository.get("note_today").await.unwrap();
    assert_eq!(stored.content, "earlier\n\nlater");
    // The unrelated page is untouched.
    assert_eq!(
        repository.get("note_week").await.unwrap().content,
        "## Journal Synthesis"
    );
}

#[tokio::test]
async fn find_by_title_matches_exactly_and_lists_newest_first() {
    let repository = fresh_repository().await;

    repository
        .insert(&record(
            "a",
            "Week of Sep 1",
            "old",
            "2026-09-01T10:00:00.000Z",
        ))
        .await
        .unwrap();
    repository
        .insert(&record(
            "b",
            "Week of Sep 1",
            "new",
            "2026-09-03T10:00:00.000Z",
        ))
        .await
        .unwrap();

    let found = repository
        .find_by_title("Week of Sep 1", None)
        .await
        .unwrap();
    assert_eq!(found.unwrap().id, "b");

    assert!(repository
        .find_by_title("week of sep 1 ", None)
        .await
        .unwrap()
        .is_none());
}

#[tokio::test]
async fn a_journal_lists_only_its_own_pages() {
    let pool = fresh_pool().await;
    insert_journal(&pool, "journal_one", "Journal 1").await;
    insert_journal(&pool, "journal_two", "Journal 2").await;
    let repository = DailyNotesRepository::new(pool);

    let mut mine = record("mine", "My page", "", "2026-09-17T10:00:00.000Z");
    mine.journal_id = Some("journal_one".to_string());
    let mut theirs = record("theirs", "Their page", "", "2026-09-17T11:00:00.000Z");
    theirs.journal_id = Some("journal_two".to_string());
    // Vault imports arrive owned by nothing.
    let unfiled = record("unfiled", "Imported note", "", "2026-09-17T12:00:00.000Z");

    for note in [&mine, &theirs, &unfiled] {
        repository.insert(note).await.unwrap();
    }

    let listed = repository.list_for_journal("journal_one").await.unwrap();
    assert_eq!(
        listed.iter().map(|n| n.id.as_str()).collect::<Vec<_>>(),
        vec!["mine"],
        "a journal must not show another journal's pages, nor unfiled ones"
    );

    // The cross-journal surfaces still see everything.
    assert_eq!(repository.list().await.unwrap().len(), 3);
}

#[tokio::test]
async fn deleting_a_journal_deletes_the_pages_it_owns() {
    let pool = fresh_pool().await;
    insert_journal(&pool, "journal_one", "Journal 1").await;
    insert_journal(&pool, "journal_two", "Journal 2").await;
    let repository = DailyNotesRepository::new(pool.clone());

    let mut doomed = record("doomed", "Page", "", "2026-09-17T10:00:00.000Z");
    doomed.journal_id = Some("journal_one".to_string());
    let mut survivor = record("survivor", "Page", "", "2026-09-17T10:00:00.000Z");
    survivor.journal_id = Some("journal_two".to_string());
    let unfiled = record("unfiled", "Imported note", "", "2026-09-17T10:00:00.000Z");

    for note in [&doomed, &survivor, &unfiled] {
        repository.insert(note).await.unwrap();
    }

    sqlx::query("DELETE FROM journals WHERE id = ?")
        .bind("journal_one")
        .execute(&pool)
        .await
        .unwrap();

    let remaining: Vec<String> = repository
        .list()
        .await
        .unwrap()
        .into_iter()
        .map(|note| note.id)
        .collect();
    assert!(
        !remaining.contains(&"doomed".to_string()),
        "a deleted journal must take its pages with it, or recreating one \
         resurrects them"
    );
    assert!(remaining.contains(&"survivor".to_string()));
    assert!(remaining.contains(&"unfiled".to_string()));
}

/// The reported bug: a synthesis was "saved" to a page no journal owned, and
/// the journal screen — which lists pages per journal and nothing else — had no
/// way to show it.
#[tokio::test]
async fn a_capture_lands_in_the_journal_written_to_last() {
    let pool = fresh_pool().await;
    insert_journal(&pool, "journal_one", "Journal 1").await;
    insert_journal(&pool, "journal_two", "Journal 2").await;
    let repository = DailyNotesRepository::new(pool);

    let mut older = record("older", "Page", "", "2026-09-17T10:00:00.000Z");
    older.journal_id = Some("journal_one".to_string());
    let mut newer = record("newer", "Page", "", "2026-09-18T10:00:00.000Z");
    newer.journal_id = Some("journal_two".to_string());
    // Newest of all, but owned by nothing: it must not make "nothing" the answer.
    let unfiled = record("unfiled", "Imported note", "", "2026-09-19T10:00:00.000Z");
    for note in [&older, &newer, &unfiled] {
        repository.insert(note).await.unwrap();
    }

    assert_eq!(
        repository.capture_journal_id().await.unwrap().as_deref(),
        Some("journal_two")
    );
}

#[tokio::test]
async fn a_capture_prefers_any_open_journal_to_none_and_skips_archived_ones() {
    let pool = fresh_pool().await;
    let repository = DailyNotesRepository::new(pool.clone());
    assert_eq!(repository.capture_journal_id().await.unwrap(), None);

    insert_journal(&pool, "journal_archived", "Old").await;
    sqlx::query("UPDATE journals SET is_archived = 1 WHERE id = 'journal_archived'")
        .execute(&pool)
        .await
        .unwrap();
    let mut page = record("page", "Page", "", "2026-09-18T10:00:00.000Z");
    page.journal_id = Some("journal_archived".to_string());
    repository.insert(&page).await.unwrap();
    assert_eq!(repository.capture_journal_id().await.unwrap(), None);

    // Empty, but open: better than a page nobody can reach.
    insert_journal(&pool, "journal_open", "Open").await;
    assert_eq!(
        repository.capture_journal_id().await.unwrap().as_deref(),
        Some("journal_open")
    );
}

/// Each journal has its own "today": a capture bound for one journal must not
/// be appended to another journal's page just because the titles match.
#[tokio::test]
async fn todays_page_is_looked_up_inside_one_journal() {
    let pool = fresh_pool().await;
    insert_journal(&pool, "journal_one", "Journal 1").await;
    insert_journal(&pool, "journal_two", "Journal 2").await;
    let repository = DailyNotesRepository::new(pool);

    let title = "Daily Notes · Friday, September 18";
    let mut theirs = record("theirs", title, "", "2026-09-18T10:00:00.000Z");
    theirs.journal_id = Some("journal_two".to_string());
    let unowned = record("unowned", title, "", "2026-09-18T11:00:00.000Z");
    for note in [&theirs, &unowned] {
        repository.insert(note).await.unwrap();
    }

    assert!(repository
        .find_by_title(title, Some("journal_one"))
        .await
        .unwrap()
        .is_none());
    assert_eq!(
        repository
            .find_by_title(title, Some("journal_two"))
            .await
            .unwrap()
            .unwrap()
            .id,
        "theirs"
    );
    assert_eq!(
        repository
            .find_by_title(title, None)
            .await
            .unwrap()
            .unwrap()
            .id,
        "unowned"
    );
}
