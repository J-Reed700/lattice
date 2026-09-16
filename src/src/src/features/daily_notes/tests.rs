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

async fn fresh_repository() -> DailyNotesRepository {
    let pool = SqlitePool::connect(":memory:").await.unwrap();
    sqlx::migrate!("./migrations").run(&pool).await.unwrap();
    DailyNotesRepository::new(pool)
}

fn record(id: &str, title: &str, content: &str, updated_at: &str) -> WorkspaceNoteRecord {
    WorkspaceNoteRecord {
        id: id.to_string(),
        title: title.to_string(),
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
        .find_by_title("Daily Notes · Saturday, September 6")
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
        .find_by_title("Daily Notes · Saturday, September 6")
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
        .find_by_title("Daily Notes · Saturday, September 6")
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

    let found = repository.find_by_title("Week of Sep 1").await.unwrap();
    assert_eq!(found.unwrap().id, "b");

    assert!(repository
        .find_by_title("week of sep 1 ")
        .await
        .unwrap()
        .is_none());
}
