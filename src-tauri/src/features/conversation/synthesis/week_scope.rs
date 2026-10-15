//! The "week" scope: the conversations, references and notes of the last
//! seven days.
use super::super::chat::ports::{ChatRuntime, SynthesisEntries};
use super::super::commands as conversation;
use super::prompts::{build_synthesis_transcript, source_keys_by_message};
use super::{
    source_key, JournalSynthesisEntry, SourceDto, SynthesisEntryKind, SynthesisMessage,
    WEEK_SYNTHESIS_CONVERSATION_PAGE, WEEK_SYNTHESIS_DAYS, WEEK_SYNTHESIS_PAGE_TITLE_PREFIX,
};
use chrono::Utc;
use lattice_core::shared::error::Result;

pub(super) fn is_within_week(
    updated_at: chrono::DateTime<Utc>,
    now: chrono::DateTime<Utc>,
) -> bool {
    updated_at >= now - chrono::Duration::days(WEEK_SYNTHESIS_DAYS)
}

pub(super) fn is_week_synthesis_page(title: &str) -> bool {
    title
        .trim_start()
        .starts_with(WEEK_SYNTHESIS_PAGE_TITLE_PREFIX)
}

pub(super) fn sort_entries_by_recency(entries: &mut [JournalSynthesisEntry]) {
    entries.sort_by(|left, right| {
        let left_time =
            lattice_core::shared::persistence::timestamps::parse_db_timestamp(&left.updated_at)
                .unwrap_or(chrono::DateTime::<Utc>::MIN_UTC);
        let right_time =
            lattice_core::shared::persistence::timestamps::parse_db_timestamp(&right.updated_at)
                .unwrap_or(chrono::DateTime::<Utc>::MIN_UTC);
        right_time.cmp(&left_time)
    });
}

/// Everything of the past seven days: every conversation, saved passage and
/// journal page in the window, whole. The synthesis splits what does not fit
/// one model call across several rather than choosing a few.
pub(super) async fn select_week_entries(
    runtime: &dyn ChatRuntime,
) -> Result<Vec<JournalSynthesisEntry>> {
    let now = Utc::now();
    let mut entries: Vec<JournalSynthesisEntry> = Vec::new();

    // (a) Conversations — typed, ordered updated_at DESC, so paging stops at
    // the first one older than the week.
    let mut candidates = Vec::new();
    let mut offset = 0i64;
    loop {
        let page = conversation::list_conversations_impl(
            runtime,
            Some(WEEK_SYNTHESIS_CONVERSATION_PAGE),
            Some(offset),
        )
        .await?;
        let read = page.len() as i64;
        let reached_older = page
            .iter()
            .any(|candidate| !is_within_week(candidate.updated_at, now));
        candidates.extend(page);
        if read < WEEK_SYNTHESIS_CONVERSATION_PAGE || reached_older {
            break;
        }
        offset += read;
    }

    for candidate in candidates {
        if !is_within_week(candidate.updated_at, now) {
            continue;
        }
        let conversation_id = candidate.id.to_string();
        let messages =
            conversation::get_conversation_messages_impl(runtime, conversation_id.clone()).await?;
        let transcript = build_synthesis_transcript(&messages);
        let (sources, prompt_messages) = source_keys_by_message(&messages);
        if transcript.is_empty() {
            continue;
        }
        entries.push(JournalSynthesisEntry {
            conversation_id: conversation_id.clone(),
            title: candidate.title,
            updated_at: lattice_core::shared::persistence::timestamps::format_db_timestamp(
                candidate.updated_at,
            ),
            message_count: messages.len(),
            transcript,
            kind: SynthesisEntryKind::Conversation,
            source_id: conversation_id,
            sources,
            messages: prompt_messages,
        });
    }

    // (b) Saved passages — our own table, always written canonically.
    let cutoff = lattice_core::shared::persistence::timestamps::format_db_timestamp(
        now - chrono::Duration::days(WEEK_SYNTHESIS_DAYS),
    );
    let references = runtime.references_created_since(&cutoff).await?;

    for record in references {
        let mut lines = vec![format!(
            "SAVED PASSAGE from {}{}",
            record.file_name,
            record
                .locator
                .as_deref()
                .map(|locator| format!(" ({locator})"))
                .unwrap_or_default(),
        )];
        lines.push(record.text.clone());
        if let Some(note) = record.note.as_deref().filter(|n| !n.trim().is_empty()) {
            lines.push(format!("MY NOTE: {note}"));
        }
        let title = record
            .title
            .clone()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| record.file_name.clone());
        let source = SourceDto {
            document_id: record.document_id.clone(),
            chunk_id: record.chunk_id.clone().unwrap_or_else(|| record.id.clone()),
            content: record.text.clone(),
            score: 1.0,
            path: Some(record.file_path.clone()),
            position: None,
            file_name: record.file_name.clone(),
            file_path: record.file_path.clone(),
            mime_type: String::new(),
            category: "Saved passage".to_string(),
            file_size_bytes: 0,
            modified_at: record.created_at.clone(),
            excerpt: Some(record.text.clone()),
            highlights: None,
            section: record.locator.clone(),
            chunk_index: None,
            page_number: None,
            from_ocr: false,
            chunk_excerpts: None,
            citation_id: None,

            web_snapshot: None,
        };

        entries.push(JournalSynthesisEntry {
            conversation_id: String::new(),
            title,
            updated_at: record.created_at.clone(),
            message_count: 0,
            transcript: lines.join("\n"),
            kind: SynthesisEntryKind::Reference,
            source_id: record.id.clone(),
            sources: vec![source],
            messages: Vec::new(),
        });
    }

    // (c) Journal pages — date() on both sides handles legacy timestamps.
    let start_date = (now - chrono::Duration::days(WEEK_SYNTHESIS_DAYS))
        .format("%Y-%m-%d")
        .to_string();
    let end_date = now.format("%Y-%m-%d").to_string();
    entries.extend(select_week_notes(runtime, &start_date, &end_date).await?);

    sort_entries_by_recency(&mut entries);
    Ok(entries)
}

/// The journal pages a week synthesis reads, chosen on their stored rows:
/// empty pages and earlier week pages are skipped, and every other page in
/// the range is read. Only a chosen page is hydrated, so a page the synthesis
/// passes over is neither rewritten nor able to fail it.
pub(super) async fn select_week_notes<E: SynthesisEntries + ?Sized>(
    store: &E,
    start_date: &str,
    end_date: &str,
) -> Result<Vec<JournalSynthesisEntry>> {
    let mut entries: Vec<JournalSynthesisEntry> = Vec::new();
    let notes = store.notes_in_range(start_date, end_date).await?;

    for note in notes {
        if note.content.trim().is_empty() || is_week_synthesis_page(&note.title) {
            continue;
        }
        // Weekly synthesis can consume an old page before the user opens it.
        // Recover and persist its archived citations at this boundary too.
        let note = store.hydrate_note(note).await?;
        let note_sources = note.sources;
        let note_citation_keys = note_sources
            .iter()
            .filter_map(|source| source.citation_id.map(|id| (id, source_key(source))))
            .collect();
        entries.push(JournalSynthesisEntry {
            conversation_id: String::new(),
            title: note.title.clone(),
            updated_at: note.updated_at.clone(),
            message_count: 0,
            transcript: format!("JOURNAL PAGE: {}\n{}", note.title, note.content),
            kind: SynthesisEntryKind::Note,
            source_id: note.id.clone(),
            sources: note_sources,
            messages: vec![SynthesisMessage {
                role: "journal page".to_string(),
                content: note.content.clone(),
                citation_keys: note_citation_keys,
            }],
        });
    }

    Ok(entries)
}
