//! Repository + validation tests for passage references.
//!
//! The repository is exercised against a real in-memory SQLite database with
//! the real migrations applied (the `features::corpus_shape::tests` pattern);
//! the input rules are exercised through the pure `build_create_record`, which
//! needs neither a `Container` nor a pool.

#![cfg(test)]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]

use sqlx::SqlitePool;

use super::commands::{build_create_record, MAX_TEXT_CHARS};
use super::dto::CreatePassageReferenceRequestDto;
use super::repository::{PassageReferenceRecord, PassageReferenceRepository};
use crate::shared::error::AppError;

async fn fresh_repository() -> PassageReferenceRepository {
    let pool = SqlitePool::connect(":memory:").await.unwrap();
    sqlx::migrate!("./migrations").run(&pool).await.unwrap();
    PassageReferenceRepository::new(pool)
}

fn record(id: &str, created_at: &str) -> PassageReferenceRecord {
    PassageReferenceRecord {
        id: id.to_string(),
        document_id: "doc_1".to_string(),
        chunk_id: Some("chunk_1".to_string()),
        file_path: "/vault/paper.pdf".to_string(),
        file_name: "paper.pdf".to_string(),
        locator: Some("p. 12".to_string()),
        text: "We conducted a randomised controlled trial.".to_string(),
        title: Some("Trial design".to_string()),
        note: Some("Compare with the 2024 replication.".to_string()),
        created_at: created_at.to_string(),
    }
}

fn create_request(text: &str) -> CreatePassageReferenceRequestDto {
    CreatePassageReferenceRequestDto {
        document_id: "doc_1".to_string(),
        chunk_id: None,
        file_path: "/vault/paper.pdf".to_string(),
        file_name: "paper.pdf".to_string(),
        locator: None,
        text: text.to_string(),
        title: None,
        note: None,
    }
}

#[tokio::test]
async fn insert_then_get_roundtrips_every_field() {
    let repo = fresh_repository().await;

    let mut nulled = record("pref_null", "2026-09-01T10:00:00.000Z");
    nulled.chunk_id = None;
    nulled.locator = None;
    nulled.title = None;
    nulled.note = None;

    let full = record("pref_full", "2026-09-01T11:00:00.000Z");
    repo.insert(&full).await.unwrap();
    repo.insert(&nulled).await.unwrap();

    let fetched_full = repo.get("pref_full").await.unwrap();
    assert_eq!(fetched_full.document_id, full.document_id);
    assert_eq!(fetched_full.chunk_id, full.chunk_id);
    assert_eq!(fetched_full.file_path, full.file_path);
    assert_eq!(fetched_full.file_name, full.file_name);
    assert_eq!(fetched_full.locator, full.locator);
    assert_eq!(fetched_full.text, full.text);
    assert_eq!(fetched_full.title, full.title);
    assert_eq!(fetched_full.note, full.note);
    assert_eq!(fetched_full.created_at, full.created_at);

    let fetched_null = repo.get("pref_null").await.unwrap();
    assert_eq!(fetched_null.chunk_id, None);
    assert_eq!(fetched_null.locator, None);
    assert_eq!(fetched_null.title, None);
    assert_eq!(fetched_null.note, None);

    assert_eq!(repo.count().await.unwrap(), 2);
}

#[tokio::test]
async fn list_returns_newest_first() {
    let repo = fresh_repository().await;
    repo.insert(&record("pref_a", "2026-09-01T10:00:00.000Z"))
        .await
        .unwrap();
    repo.insert(&record("pref_c", "2026-09-03T10:00:00.000Z"))
        .await
        .unwrap();
    repo.insert(&record("pref_b", "2026-09-02T10:00:00.000Z"))
        .await
        .unwrap();

    let listed = repo.list(10).await.unwrap();
    let ids: Vec<&str> = listed.iter().map(|r| r.id.as_str()).collect();
    assert_eq!(ids, vec!["pref_c", "pref_b", "pref_a"]);
}

#[tokio::test]
async fn list_created_since_excludes_older() {
    let repo = fresh_repository().await;
    let cutoff = "2026-09-02T10:00:00.000Z";
    repo.insert(&record("pref_before", "2026-09-02T09:59:59.000Z"))
        .await
        .unwrap();
    repo.insert(&record("pref_at", cutoff)).await.unwrap();
    repo.insert(&record("pref_after", "2026-09-02T10:00:01.000Z"))
        .await
        .unwrap();

    let listed = repo.list_created_since(cutoff, 10).await.unwrap();
    let ids: Vec<&str> = listed.iter().map(|r| r.id.as_str()).collect();
    assert_eq!(ids, vec!["pref_after", "pref_at"]);
}

#[tokio::test]
async fn update_annotations_sets_and_clears() {
    let repo = fresh_repository().await;
    let mut row = record("pref_1", "2026-09-01T10:00:00.000Z");
    row.title = None;
    row.note = None;
    repo.insert(&row).await.unwrap();

    repo.update_annotations("pref_1", Some("A title"), Some("A note"))
        .await
        .unwrap();
    let set = repo.get("pref_1").await.unwrap();
    assert_eq!(set.title.as_deref(), Some("A title"));
    assert_eq!(set.note.as_deref(), Some("A note"));

    repo.update_annotations("pref_1", None, None).await.unwrap();
    let cleared = repo.get("pref_1").await.unwrap();
    assert_eq!(cleared.title, None);
    assert_eq!(cleared.note, None);
}

#[tokio::test]
async fn update_annotations_missing_id_is_not_found() {
    let repo = fresh_repository().await;
    let error = repo
        .update_annotations("pref_missing", Some("t"), None)
        .await
        .unwrap_err();
    assert!(matches!(error, AppError::NotFound(_)), "got {error:?}");
}

#[tokio::test]
async fn delete_returns_false_when_absent() {
    let repo = fresh_repository().await;
    assert!(!repo.delete("pref_missing").await.unwrap());
}

#[tokio::test]
async fn delete_removes_row() {
    let repo = fresh_repository().await;
    repo.insert(&record("pref_1", "2026-09-01T10:00:00.000Z"))
        .await
        .unwrap();
    assert!(repo.delete("pref_1").await.unwrap());
    assert_eq!(repo.count().await.unwrap(), 0);
    assert!(matches!(
        repo.get("pref_1").await.unwrap_err(),
        AppError::NotFound(_)
    ));
}

#[test]
fn create_impl_trims_and_rejects_empty_text() {
    let mut request = create_request("   Saved passage.   ");
    request.title = Some("   ".to_string());
    let built = build_create_record(request).unwrap();
    assert_eq!(built.text, "Saved passage.");
    assert_eq!(built.title, None);
    assert!(built.id.starts_with("pref_"));

    let error = build_create_record(create_request("   \n  ")).unwrap_err();
    assert!(matches!(error, AppError::InvalidInput(_)), "got {error:?}");

    let mut missing_document = create_request("Some text");
    missing_document.document_id = "  ".to_string();
    assert!(matches!(
        build_create_record(missing_document).unwrap_err(),
        AppError::InvalidInput(_)
    ));
}

#[test]
fn create_impl_truncates_oversized_text() {
    // Multi-byte characters so byte slicing would panic or corrupt.
    let oversized = "é".repeat(25_000);
    let built = build_create_record(create_request(&oversized)).unwrap();
    assert_eq!(built.text.chars().count(), MAX_TEXT_CHARS);
    assert!(built.text.ends_with('…'));
    assert!(std::str::from_utf8(built.text.as_bytes()).is_ok());
}
