#![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]

use super::super::chat::ports::{StoredJournalPage, SynthesisEntries};
use super::prompts::{format_synthesis_sources, rewrite_citation_ids};
use super::week_scope::{
    is_week_synthesis_page, is_within_week, select_week_notes, sort_entries_by_recency,
};
use super::*;
use chrono::{TimeZone, Utc};

fn entry(kind: SynthesisEntryKind, source_id: &str, title: &str) -> JournalSynthesisEntry {
    JournalSynthesisEntry {
        conversation_id: if kind == SynthesisEntryKind::Conversation {
            source_id.to_string()
        } else {
            String::new()
        },
        title: title.to_string(),
        updated_at: "2026-09-05T10:00:00.000Z".to_string(),
        message_count: 0,
        transcript: "TRANSCRIPT".to_string(),
        kind,
        source_id: source_id.to_string(),
        sources: Vec::new(),
        messages: Vec::new(),
    }
}

#[test]
fn week_scope_is_accepted_and_not_coerced_to_deck() {
    assert_eq!(normalize_synthesis_scope(Some("week")), "week");
    assert_eq!(normalize_synthesis_scope(Some("  WEEK  ")), "week");
}

#[test]
fn conversation_scope_is_accepted_and_not_coerced_to_deck() {
    assert_eq!(
        normalize_synthesis_scope(Some("conversation")),
        "conversation"
    );
}

#[test]
fn unknown_scope_still_coerces_to_deck() {
    assert_eq!(normalize_synthesis_scope(Some("everything")), "deck");
    assert_eq!(normalize_synthesis_scope(Some("   ")), "deck");
    assert_eq!(normalize_synthesis_scope(None), "deck");
    // The three historical scopes must keep passing through untouched.
    assert_eq!(normalize_synthesis_scope(Some("current")), "current");
    assert_eq!(normalize_synthesis_scope(Some("pinned")), "pinned");
    assert_eq!(normalize_synthesis_scope(Some("deck")), "deck");
}

#[test]
fn select_week_entries_filters_by_fixed_cutoff() {
    let now = Utc.with_ymd_and_hms(2026, 9, 6, 12, 0, 0).unwrap();
    let six_days_ago = now - chrono::Duration::days(6);
    let eight_days_ago = now - chrono::Duration::days(8);
    let exactly_seven_days_ago = now - chrono::Duration::days(7);

    assert!(is_within_week(six_days_ago, now));
    assert!(is_within_week(exactly_seven_days_ago, now));
    assert!(!is_within_week(eight_days_ago, now));
}

#[test]
fn week_entries_skip_week_pages() {
    assert!(is_week_synthesis_page("Week of Sep 1"));
    assert!(is_week_synthesis_page("  Week of Aug 25"));
    assert!(!is_week_synthesis_page("Weekly review"));
    assert!(!is_week_synthesis_page("Sep 6"));
}

fn stored_page(id: &str, title: &str, content: &str) -> StoredJournalPage {
    StoredJournalPage {
        revision: 0,
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
        created_at: "2026-09-05T10:00:00.000Z".to_string(),
        updated_at: "2026-09-05T10:00:00.000Z".to_string(),
    }
}

/// Journal pages as stored, whose hydration fails for any page the
/// synthesis should have passed over and records the ones it used.
struct StoredPages {
    rows: Vec<StoredJournalPage>,
    hydrated: std::sync::Mutex<Vec<String>>,
}

#[async_trait]
impl SynthesisEntries for StoredPages {
    async fn references_created_since(
        &self,
        _cutoff: &str,
    ) -> Result<Vec<lattice_core::application::contracts::references::PassageReferenceRecord>> {
        unreachable!("only journal pages are read here")
    }

    async fn notes_in_range(
        &self,
        _start_date: &str,
        _end_date: &str,
    ) -> Result<Vec<StoredJournalPage>> {
        Ok(self.rows.clone())
    }

    async fn hydrate_note(
        &self,
        page: StoredJournalPage,
    ) -> Result<lattice_core::application::contracts::notes::WorkspaceNoteDto> {
        if page.id.starts_with("unused") {
            return Err(AppError::Database(format!(
                "{} was hydrated though the synthesis does not use it",
                page.id
            )));
        }
        self.hydrated.lock().unwrap().push(page.id.clone());
        Ok(
            lattice_core::application::contracts::notes::WorkspaceNoteDto {
                revision: page.revision,
                id: page.id,
                title: page.title,
                journal_id: page.journal_id,
                content: page.content,
                linked_document_ids: Vec::new(),
                linked_conversation_ids: Vec::new(),
                highlights: Vec::new(),
                sticky_notes: Vec::new(),
                conversation_snapshots: Vec::new(),
                sources: Vec::new(),
                created_at: page.created_at,
                updated_at: page.updated_at,
            },
        )
    }
}

#[tokio::test]
async fn week_notes_hydrate_only_the_pages_they_use() {
    let mut rows = vec![
        stored_page("unused-empty", "Sep 1", "   "),
        stored_page("unused-week", "Week of Aug 25", "Last week's synthesis"),
    ];
    // More pages than the old cap of eight: every one in the week is read.
    rows.extend((0..12).map(|index| stored_page(&format!("note-{index}"), "Sep 4", "Notes")));
    let store = StoredPages {
        rows,
        hydrated: std::sync::Mutex::default(),
    };

    let entries = select_week_notes(&store, "2026-08-30", "2026-09-06")
        .await
        .expect("a page the synthesis passes over cannot fail it");

    let used: Vec<String> = (0..12).map(|index| format!("note-{index}")).collect();
    assert_eq!(*store.hydrated.lock().unwrap(), used);
    assert_eq!(
        entries
            .iter()
            .map(|entry| entry.source_id.clone())
            .collect::<Vec<_>>(),
        used
    );
}

#[test]
fn citations_map_kinds_correctly() {
    let entries = vec![
        entry(SynthesisEntryKind::Conversation, "conv_1", "Sleep study"),
        entry(SynthesisEntryKind::Reference, "pref_1", "paper.pdf"),
        entry(SynthesisEntryKind::Note, "note_1", "Sep 4"),
    ];
    let citations = build_synthesis_citations(&entries);

    assert_eq!(citations.len(), 3);
    assert_eq!(citations[0].kind, "conversation");
    assert_eq!(citations[0].id, "conv_1");
    assert_eq!(citations[0].title, "Sleep study");
    assert_eq!(citations[1].kind, "reference");
    assert_eq!(citations[1].id, "pref_1");
    assert_eq!(citations[2].kind, "note");
    assert_eq!(citations[2].id, "note_1");
}

fn source(document: &str, chunk: &str, content: &str, citation_id: u32) -> SourceDto {
    serde_json::from_value(serde_json::json!({
        "documentId": document, "chunkId": chunk, "content": content,
        "score": 1.0, "path": null, "position": null,
        "fileName": format!("{document}.pdf"), "filePath": format!("/{document}.pdf"),
        "mimeType": "application/pdf", "category": "PDF", "fileSizeBytes": 1,
        "modifiedAt": "2026-09-01T00:00:00Z", "citationId": citation_id
    }))
    .unwrap()
}

#[test]
fn citations_are_remapped_per_message_and_survive_global_flattening() {
    let a = source("doc-a", "chunk-a", "Evidence A", 1);
    let b = source("doc-b", "chunk-b", "Evidence B", 1);
    let key_a = source_key(&a);
    let key_b = source_key(&b);
    let mut entries = vec![
        JournalSynthesisEntry {
            conversation_id: "c1".into(),
            title: "First".into(),
            updated_at: "".into(),
            message_count: 1,
            transcript: "ASSISTANT: claim [1]".into(),
            kind: SynthesisEntryKind::Conversation,
            source_id: "c1".into(),
            sources: vec![a],
            messages: vec![SynthesisMessage {
                role: "assistant".into(),
                content: "claim [1]".into(),
                citation_keys: HashMap::from([(1, key_a)]),
            }],
        },
        JournalSynthesisEntry {
            conversation_id: "c2".into(),
            title: "Second".into(),
            updated_at: "".into(),
            message_count: 1,
            transcript: "ASSISTANT: claim [1]".into(),
            kind: SynthesisEntryKind::Conversation,
            source_id: "c2".into(),
            sources: vec![b],
            messages: vec![SynthesisMessage {
                role: "assistant".into(),
                content: "claim [1]".into(),
                citation_keys: HashMap::from([(1, key_b)]),
            }],
        },
    ];
    let sources = assign_synthesis_source_ids(&mut entries);
    assert_eq!(
        sources
            .iter()
            .map(|source| source.citation_id)
            .collect::<Vec<_>>(),
        vec![Some(1), Some(2)]
    );
    assert!(entries[0].transcript.contains("claim [1]"));
    assert!(entries[1].transcript.contains("claim [2]"));
}

#[test]
fn colliding_citation_numbers_across_messages_in_one_conversation_remap_and_sources_survive_truncation(
) {
    let mut raw_messages = Vec::new();
    for index in 0..30 {
        let original = source(
            &format!("doc-{index}"),
            &format!("chunk-{index}"),
            &format!("Evidence passage {index}"),
            1,
        );
        raw_messages.push(lattice_core::domain::conversation::ConversationMessage {
            id: format!("message-{index}"),
            conversation_id: Default::default(),
            role: lattice_core::domain::conversation::MessageRole::Assistant,
            content: format!("Claim {index} [1]. {}", "x".repeat(1_000)),
            tokens: 100,
            created_at: Utc::now(),
            metadata: Some(serde_json::json!({ "sources": [original] }).to_string()),
            status: "completed".into(),
        });
    }
    let (sources, messages) = source_keys_by_message(&raw_messages);
    let mut entries = vec![JournalSynthesisEntry {
        conversation_id: "same-conversation".into(),
        title: "Many turns".into(),
        updated_at: "".into(),
        message_count: raw_messages.len(),
        transcript: build_synthesis_transcript(&raw_messages),
        kind: SynthesisEntryKind::Conversation,
        source_id: "same-conversation".into(),
        sources,
        messages,
    }];
    let numbered_sources = assign_synthesis_source_ids(&mut entries);
    assert_eq!(numbered_sources.len(), 30);
    assert_eq!(
        numbered_sources
            .last()
            .and_then(|source| source.citation_id),
        Some(30)
    );
    assert!(entries[0].transcript.contains("Claim 0 [1]"));
    assert!(entries[0].transcript.contains("Claim 1 [2]"));
    // The source list is independent of the transcript cap, so sources
    // from late turns remain available even when those turns are clipped.
    assert!(numbered_sources
        .iter()
        .any(|source| source.content == "Evidence passage 29"));
}

#[test]
fn same_url_with_different_snapshots_keeps_separate_citations() {
    let make_source = |text: &str| {
        let mut source = source("web:example", "web-chunk", "Same search excerpt", 1);
        source.web_snapshot = Some(
            serde_json::from_value(serde_json::json!({
                "url": "https://example.com/article", "title": "Article",
                "text": text, "fetchedAt": "2026-09-28T00:00:00Z", "truncated": false
            }))
            .unwrap(),
        );
        source
    };
    let mut first = entry(SynthesisEntryKind::Conversation, "first", "First");
    let mut second = entry(SynthesisEntryKind::Conversation, "second", "Second");
    first.sources.push(make_source("Original captured article"));
    second.sources.push(make_source("Changed captured article"));
    for entry in [&mut first, &mut second] {
        entry.messages.push(SynthesisMessage {
            role: "assistant".into(),
            content: "Claim [1]".into(),
            citation_keys: HashMap::from([(1, source_key(&entry.sources[0]))]),
        });
    }
    let mut entries = vec![first, second];
    let sources = assign_synthesis_source_ids(&mut entries);
    assert_eq!(sources.len(), 2);
    assert_eq!(
        sources[0].web_snapshot.as_ref().unwrap().text,
        "Original captured article"
    );
    assert_eq!(
        sources[1].web_snapshot.as_ref().unwrap().text,
        "Changed captured article"
    );
    assert!(entries[0].transcript.contains("Claim [1]"));
    assert!(entries[1].transcript.contains("Claim [2]"));
    // Saved page bodies are reader data, not extra synthesis prompt text.
    assert!(!format_synthesis_sources(&sources).contains("captured article"));
    let persisted = serde_json::to_string(&sources).unwrap();
    let restored: Vec<SourceDto> = serde_json::from_str(&persisted).unwrap();
    assert_eq!(source_key(&restored[1]), source_key(&sources[1]));
}

#[test]
fn repeated_source_passages_share_one_global_citation() {
    let first = source("doc-a", "chunk-a", "Same evidence", 1);
    let second = source("doc-a", "chunk-a", "Same evidence", 3);
    let key = source_key(&first);
    let make_entry = |source| JournalSynthesisEntry {
        conversation_id: "c".into(),
        title: "Entry".into(),
        updated_at: "".into(),
        message_count: 1,
        transcript: String::new(),
        kind: SynthesisEntryKind::Conversation,
        source_id: "c".into(),
        sources: vec![source],
        messages: vec![SynthesisMessage {
            role: "assistant".into(),
            content: "claim [1]".into(),
            citation_keys: HashMap::from([(1, key.clone())]),
        }],
    };
    let mut entries = vec![make_entry(first), make_entry(second)];
    let sources = assign_synthesis_source_ids(&mut entries);
    assert_eq!(sources.len(), 1);
    assert_eq!(entries[0].transcript, entries[1].transcript);
    assert!(entries[1].transcript.contains("[1]"));
}

#[test]
fn sparse_local_citation_ids_map_by_source_identity_and_orphans_are_removed() {
    let mapped = HashMap::from([(7, 2), (2, 1)]);
    assert_eq!(
        rewrite_citation_ids("A [7], B [2], orphan [9].", &mapped),
        "A [2], B [1], orphan ."
    );
}

#[test]
fn legacy_sources_without_explicit_citation_ids_use_chat_order_fallback() {
    let mut legacy = source("doc-a", "chunk-a", "Evidence A", 1);
    legacy.citation_id = None;
    let message = lattice_core::domain::conversation::ConversationMessage {
        id: "message-1".into(),
        conversation_id: Default::default(),
        role: lattice_core::domain::conversation::MessageRole::Assistant,
        content: "Claim [1]".into(),
        tokens: 1,
        created_at: Utc::now(),
        metadata: Some(serde_json::json!({ "sources": [legacy] }).to_string()),
        status: "completed".into(),
    };
    let (sources, messages) = source_keys_by_message(&[message]);
    assert_eq!(sources.len(), 1);
    assert_eq!(messages[0].citation_keys.len(), 1);
    assert!(messages[0].citation_keys.contains_key(&1));
}

#[test]
fn entries_sort_newest_first_and_tolerate_bad_timestamps() {
    let mut entries = vec![
        entry(SynthesisEntryKind::Note, "note_old", "Old"),
        entry(SynthesisEntryKind::Reference, "pref_bad", "Bad"),
        entry(SynthesisEntryKind::Conversation, "conv_new", "New"),
    ];
    entries[0].updated_at = "2026-09-01T10:00:00.000Z".to_string();
    entries[1].updated_at = "not a timestamp".to_string();
    entries[2].updated_at = "2026-09-05T10:00:00.000Z".to_string();

    sort_entries_by_recency(&mut entries);
    let ids: Vec<&str> = entries.iter().map(|e| e.source_id.as_str()).collect();
    assert_eq!(ids, vec!["conv_new", "note_old", "pref_bad"]);
}
