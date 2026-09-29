//! Repository + capture-rule tests for daily notes.
//!
//! The repository runs against a real in-memory SQLite database with the real
//! migrations applied (the `features::references::tests` pattern); the capture
//! rules are exercised through the pure helpers, which need neither a
//! `Container` nor a pool.

#![cfg(test)]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]

use sqlx::SqlitePool;

use super::commands::{
    append_sources_and_remap_citations, appended_capture, capture_snippet, hydrate_workspace_note,
    row_to_dto, WorkspaceNoteDto,
};
use super::repository::{DailyNotesRepository, WorkspaceNoteRecord};
use crate::features::conversation::repository::ConversationRepository;
use crate::features::qa::dto::{SourceChunkExcerptDto, SourceDto, WebSnapshotDto};
use crate::shared::error::AppError;

fn source(citation_id: u32, document_id: &str) -> SourceDto {
    SourceDto {
        document_id: document_id.into(),
        chunk_id: format!("{document_id}-chunk"),
        content: "source content".into(),
        score: 1.0,
        path: None,
        position: None,
        file_name: format!("{document_id}.md"),
        file_path: format!("/vault/{document_id}.md"),
        mime_type: "text/markdown".into(),
        category: "Markdown".into(),
        file_size_bytes: 10,
        modified_at: "2026-09-28T00:00:00Z".into(),
        excerpt: Some("excerpt".into()),
        highlights: Some(vec!["highlight".into()]),
        section: Some("Heading".into()),
        chunk_index: Some(0),
        page_number: None,
        chunk_excerpts: None,
        citation_id: Some(citation_id),
        web_snapshot: None,
    }
}

#[test]
fn capture_remaps_only_colliding_citations_without_cascading() {
    let (content, sources) = append_sources_and_remap_citations(
        &[source(1, "existing")],
        "First [1], second [2]",
        vec![source(1, "incoming-one"), source(2, "incoming-two")],
    );
    assert_eq!(content, "First [3], second [2]");
    assert_eq!(sources[0].citation_id, Some(3));
    assert_eq!(sources[1].citation_id, Some(2));
    assert_eq!(sources[0].excerpt.as_deref(), Some("excerpt"));
    assert!(sources[0].chunk_excerpts.is_none());
}

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
        sources_json: "[]".to_string(),
        created_at: updated_at.to_string(),
        updated_at: updated_at.to_string(),
    }
}

#[tokio::test]
async fn vault_updates_preserve_saved_source_metadata() {
    let repository = fresh_repository().await;
    let mut note = record("with_sources", "Page", "See [1]", "2026-09-17T10:00:00Z");
    let sources: Vec<SourceDto> = (1..=25)
        .map(|id| {
            let mut source = source(id, &format!("doc-{id}"));
            source.page_number = Some(id);
            source.chunk_excerpts = Some(vec![SourceChunkExcerptDto {
                chunk_id: format!("doc-{id}-extra"),
                excerpt: format!("supporting excerpt {id}"),
                section: Some(format!("Section {id}")),
                chunk_index: Some(id as usize),
                page_number: Some(id),
                score: 0.8,
                highlights: Some(vec![format!("term {id}")]),
            }]);
            source
        })
        .collect();
    let expected_sources = serde_json::to_value(&sources).unwrap();
    note.sources_json = serde_json::to_string(&sources).unwrap();
    repository.insert(&note).await.unwrap();

    // A normal note edit must bind the new column and preserve every source
    // field through the repository's UPDATE path.
    let mut edited = repository.get("with_sources").await.unwrap();
    edited.content = "Updated body".into();
    repository.update(&edited).await.unwrap();

    let stored = row_to_dto(repository.get("with_sources").await.unwrap()).unwrap();
    assert_eq!(
        serde_json::to_value(&stored.sources).unwrap(),
        expected_sources
    );

    repository
        .upsert_from_vault(super::repository::VaultNoteUpsert {
            id: "with_sources",
            title: "Renamed from disk",
            content: "Edited in vault",
            created_at: "2026-09-17T10:00:00Z",
            updated_at: "2026-09-17T11:00:00Z",
        })
        .await
        .unwrap();

    let stored = row_to_dto(repository.get("with_sources").await.unwrap()).unwrap();
    assert_eq!(
        serde_json::to_value(&stored.sources).unwrap(),
        expected_sources
    );
}

#[tokio::test]
async fn opening_an_old_page_hydrates_and_persists_its_web_snapshot() {
    let pool = fresh_pool().await;
    let notes = DailyNotesRepository::new(pool.clone());
    let conversations = ConversationRepository::new(pool.clone());
    let conversation = conversations
        .create_conversation("Archived source", "test", None)
        .await
        .unwrap();
    let conversation_id = conversation.id.to_string();
    let url = "HTTPS://en.wikipedia.org/wiki/Citation";
    conversations
        .store_conversation_web_source_snapshot(
            conversation_id.clone(),
            url.into(),
            Some("Citation (wiki)".into()),
            "The archived article text.".into(),
            false,
        )
        .await
        .unwrap();

    let mut old_note = record("old-page", "Old page", "See [1]", "2026-09-20T00:00:00Z");
    old_note.linked_conversation_ids =
        serde_json::to_string(&vec![conversation_id.clone()]).unwrap();
    let mut citation = source(1, "web-source");
    citation.file_path = url.into();
    // This is an older journal payload: it has a general HTTPS URL and no
    // web: document-ID prefix or inline snapshot.
    old_note.sources_json = serde_json::to_string(&vec![citation]).unwrap();
    notes.insert(&old_note).await.unwrap();

    let hydrated =
        hydrate_workspace_note(&notes, &conversations, notes.get("old-page").await.unwrap())
            .await
            .unwrap();
    let snapshot = hydrated.sources[0].web_snapshot.as_ref().unwrap();
    assert_eq!(snapshot.url, url);
    assert_eq!(snapshot.text, "The archived article text.");
    assert_eq!(snapshot.title.as_deref(), Some("Citation (wiki)"));
    assert!(snapshot.fetched_at.is_some());

    let stored: Vec<SourceDto> =
        serde_json::from_str(&notes.get("old-page").await.unwrap().sources_json).unwrap();
    assert_eq!(stored[0].web_snapshot.as_ref().unwrap().text, snapshot.text);

    // The new snapshot belongs to the page itself now. Deleting the source
    // conversation removes its archive row but does not erase the journal copy.
    sqlx::query("DELETE FROM conversations WHERE id = ?")
        .bind(&conversation_id)
        .execute(&pool)
        .await
        .unwrap();
    let after_delete: Vec<SourceDto> =
        serde_json::from_str(&notes.get("old-page").await.unwrap().sources_json).unwrap();
    assert_eq!(
        after_delete[0].web_snapshot.as_ref().unwrap().text,
        "The archived article text."
    );
}

#[tokio::test]
async fn hydration_skips_conflicting_archives_and_preserves_existing_snapshots() {
    let pool = fresh_pool().await;
    let notes = DailyNotesRepository::new(pool.clone());
    let conversations = ConversationRepository::new(pool.clone());
    let first = conversations
        .create_conversation("First", "test", None)
        .await
        .unwrap()
        .id
        .to_string();
    let second = conversations
        .create_conversation("Second", "test", None)
        .await
        .unwrap()
        .id
        .to_string();
    let missing = conversations
        .create_conversation("No archive", "test", None)
        .await
        .unwrap()
        .id
        .to_string();
    let url = "https://example.org/wiki";
    for (conversation_id, content) in [
        (&first, "First archived version"),
        (&second, "Different archived version"),
    ] {
        conversations
            .store_conversation_web_source_snapshot(
                conversation_id.clone(),
                url.into(),
                Some("Example".into()),
                content.into(),
                false,
            )
            .await
            .unwrap();
    }

    let linked_ids = serde_json::to_string(&vec![first.clone(), second]).unwrap();
    let mut missing_snapshot = source(1, "url-source");
    missing_snapshot.file_path = url.into();
    let mut note = record("conflict", "Conflict", "", "2026-09-20T00:00:00Z");
    note.linked_conversation_ids = linked_ids.clone();
    note.sources_json = serde_json::to_string(&vec![missing_snapshot]).unwrap();
    notes.insert(&note).await.unwrap();

    let hydrated =
        hydrate_workspace_note(&notes, &conversations, notes.get("conflict").await.unwrap())
            .await
            .unwrap();
    assert!(hydrated.sources[0].web_snapshot.is_none());
    assert_eq!(
        notes.get("conflict").await.unwrap().sources_json,
        note.sources_json
    );

    let mut provenance_unknown = source(3, "provenance-unknown");
    provenance_unknown.file_path = url.into();
    let mut unknown_note = record("unknown", "Unknown", "", "2026-09-20T00:00:00Z");
    unknown_note.linked_conversation_ids = serde_json::to_string(&vec![first, missing]).unwrap();
    unknown_note.sources_json = serde_json::to_string(&vec![provenance_unknown]).unwrap();
    notes.insert(&unknown_note).await.unwrap();

    let hydrated =
        hydrate_workspace_note(&notes, &conversations, notes.get("unknown").await.unwrap())
            .await
            .unwrap();
    assert!(hydrated.sources[0].web_snapshot.is_none());
    assert_eq!(
        notes.get("unknown").await.unwrap().sources_json,
        unknown_note.sources_json
    );

    let saved_snapshot = WebSnapshotDto {
        url: url.into(),
        title: Some("Previously saved title".into()),
        text: "Previously saved page text".into(),
        fetched_at: Some("2026-09-19T00:00:00Z".into()),
        truncated: true,
    };
    let mut already_archived = source(2, "already-archived");
    already_archived.file_path = url.into();
    already_archived.web_snapshot = Some(saved_snapshot.clone());
    let mut note_with_snapshot = record("saved", "Saved", "", "2026-09-20T00:00:00Z");
    note_with_snapshot.linked_conversation_ids = linked_ids;
    note_with_snapshot.sources_json = serde_json::to_string(&vec![already_archived]).unwrap();
    notes.insert(&note_with_snapshot).await.unwrap();

    let hydrated =
        hydrate_workspace_note(&notes, &conversations, notes.get("saved").await.unwrap())
            .await
            .unwrap();
    assert_eq!(
        hydrated.sources[0].web_snapshot.as_ref().unwrap().text,
        saved_snapshot.text
    );
    let stored: Vec<SourceDto> =
        serde_json::from_str(&notes.get("saved").await.unwrap().sources_json).unwrap();
    assert_eq!(
        stored[0].web_snapshot.as_ref().unwrap().text,
        saved_snapshot.text
    );
}

#[tokio::test]
async fn hydration_compare_and_swap_does_not_overwrite_a_concurrent_note_edit() {
    let pool = fresh_pool().await;
    let repository = DailyNotesRepository::new(pool.clone());
    let note = record("racing", "Before", "Before edit", "2026-09-20T00:00:00Z");
    repository.insert(&note).await.unwrap();
    sqlx::query(
        "UPDATE daily_notes_workspace SET title = ?, content = ?, updated_at = ? WHERE id = ?",
    )
    .bind("Concurrent title")
    .bind("Concurrent content")
    .bind("2026-09-21T00:00:00Z")
    .bind(&note.id)
    .execute(&pool)
    .await
    .unwrap();

    let updated = repository
        .update_sources_if_unchanged(
            &note.id,
            &note.sources_json,
            &note.updated_at,
            r#"[{"webSnapshot":{"text":"stale"}}]"#,
        )
        .await
        .unwrap();
    assert!(!updated);
    let stored = repository.get(&note.id).await.unwrap();
    assert_eq!(stored.title, "Concurrent title");
    assert_eq!(stored.content, "Concurrent content");
    assert_eq!(stored.sources_json, note.sources_json);
}

#[test]
fn older_workspace_note_payloads_default_to_no_sources() {
    let note: WorkspaceNoteDto = serde_json::from_value(serde_json::json!({
        "id": "n", "title": "Page", "journalId": null, "content": "",
        "linkedDocumentIds": [], "linkedConversationIds": [], "highlights": [],
        "stickyNotes": [], "conversationSnapshots": [], "createdAt": "now", "updatedAt": "now"
    }))
    .unwrap();
    assert!(note.sources.is_empty());
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
