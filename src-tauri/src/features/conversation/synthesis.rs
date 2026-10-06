//! Journal synthesis selection and model orchestration.
use super::chat::chat_with_conversation_impl as run_chat_with_conversation_impl;
use super::chat::{ChatResponse, ToolPreferences};
use super::commands as conversation;
use super::workspace_dto::*;
use crate::features::qa::dto::SourceDto;
use crate::interfaces::di::Container;
use crate::shared::{error::AppError, ipc::ApiError};
use chrono::Utc;
use std::collections::{HashMap, HashSet};
type SnapshotKey = (String, Option<String>, String, Option<String>, bool);
type SourceKey = (String, String, String, Option<SnapshotKey>);

// The same URL and search excerpt can refer to different captured page versions.
// Preserve that provenance when combining citations from separate conversations.
fn source_key(source: &SourceDto) -> SourceKey {
    (
        source.document_id.clone(),
        source.chunk_id.clone(),
        source.content.clone(),
        source.web_snapshot.as_ref().map(|snapshot| {
            (
                snapshot.url.clone(),
                snapshot.title.clone(),
                snapshot.text.clone(),
                snapshot.fetched_at.clone(),
                snapshot.truncated,
            )
        }),
    )
}
#[derive(Debug, Clone)]
struct SynthesisMessage {
    role: String,
    content: String,
    citation_keys: HashMap<u32, SourceKey>,
}
const JOURNAL_SYNTHESIS_ENTRY_LIMIT_DEFAULT: usize = 12;
const JOURNAL_SYNTHESIS_ENTRY_LIMIT_MAX: usize = 24;
const JOURNAL_SYNTHESIS_MESSAGE_CHAR_LIMIT: usize = 900;
const JOURNAL_SYNTHESIS_ENTRY_CHAR_LIMIT: usize = 6000;
const JOURNAL_SYNTHESIS_CHUNK_CHAR_LIMIT: usize = 14000;
const WEEK_SYNTHESIS_DAYS: i64 = 7;
const WEEK_SYNTHESIS_MAX_CONVERSATIONS: usize = 12;
const WEEK_SYNTHESIS_MAX_REFERENCES: usize = 20;
const WEEK_SYNTHESIS_MAX_NOTES: usize = 8;
const WEEK_SYNTHESIS_CANDIDATE_SCAN: i64 = 200;
const WEEK_SYNTHESIS_SOURCE_CHAR_LIMIT: usize = 600;
const WEEK_SYNTHESIS_PAGE_TITLE_PREFIX: &str = "Week of ";

/// What a synthesis entry was built from. Conversations keep today's meaning;
/// references and notes only ever appear under the "week" scope.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SynthesisEntryKind {
    Conversation,
    Reference,
    Note,
}
impl SynthesisEntryKind {
    fn as_str(self) -> &'static str {
        match self {
            SynthesisEntryKind::Conversation => "conversation",
            SynthesisEntryKind::Reference => "reference",
            SynthesisEntryKind::Note => "note",
        }
    }
}
#[derive(Debug, Clone)]
struct JournalSynthesisEntry {
    /// Empty for reference and note entries.
    conversation_id: String,
    title: String,
    updated_at: String,
    message_count: usize,
    transcript: String,
    kind: SynthesisEntryKind,
    /// Conversation id, passage reference id, or workspace note id.
    source_id: String,
    /// Chat citations retained from this entry's assistant messages.
    sources: Vec<SourceDto>,
    messages: Vec<SynthesisMessage>,
}
fn truncate_for_synthesis(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    let mut output = text
        .chars()
        .take(max_chars.saturating_sub(1))
        .collect::<String>();
    output.push('…');
    output
}

fn build_synthesis_transcript(
    messages: &[crate::domain::conversation::ConversationMessage],
) -> String {
    let mut lines = Vec::new();
    for message in messages {
        let content = message.content.trim();
        if content.is_empty() {
            continue;
        }
        lines.push(format!(
            "{}: {}",
            message.role.to_string().to_uppercase(),
            truncate_for_synthesis(content, JOURNAL_SYNTHESIS_MESSAGE_CHAR_LIMIT)
        ));
    }

    truncate_for_synthesis(&lines.join("\n"), JOURNAL_SYNTHESIS_ENTRY_CHAR_LIMIT)
}

fn chunk_journal_synthesis_entries(
    entries: &[JournalSynthesisEntry],
) -> Vec<Vec<JournalSynthesisEntry>> {
    let mut chunks: Vec<Vec<JournalSynthesisEntry>> = Vec::new();
    let mut current: Vec<JournalSynthesisEntry> = Vec::new();
    let mut current_chars: usize = 0;

    for entry in entries {
        let entry_chars = entry.transcript.len()
            + entry.title.len()
            + entry
                .sources
                .iter()
                .map(|source| source.content.len() + source.file_name.len() + 40)
                .sum::<usize>()
            + 120;
        if !current.is_empty() && current_chars + entry_chars > JOURNAL_SYNTHESIS_CHUNK_CHAR_LIMIT {
            chunks.push(current);
            current = Vec::new();
            current_chars = 0;
        }
        current.push(entry.clone());
        current_chars += entry_chars;
    }

    if !current.is_empty() {
        chunks.push(current);
    }

    chunks
}

fn format_entries_for_synthesis_prompt(entries: &[JournalSynthesisEntry]) -> String {
    entries
        .iter()
        .enumerate()
        .map(|(index, entry)| {
            [
                format!("ENTRY {}", index + 1),
                format!("Title: {}", entry.title),
                format!("Updated: {}", entry.updated_at),
                format!("Message Count: {}", entry.message_count),
                "Transcript (citation markers resolve to the source list below):".to_string(),
                if entry.transcript.is_empty() {
                    "(No message text captured.)".to_string()
                } else {
                    entry.transcript.clone()
                },
                format_synthesis_sources(&entry.sources),
            ]
            .join("\n")
        })
        .collect::<Vec<_>>()
        .join("\n\n---\n\n")
}

fn format_synthesis_sources(sources: &[SourceDto]) -> String {
    if sources.is_empty() {
        return "Sources: (none recorded)".to_string();
    }
    let lines = sources
        .iter()
        .map(|source| {
            let id = source.citation_id.unwrap_or_default();
            let title = if source.file_name.trim().is_empty() {
                &source.file_path
            } else {
                &source.file_name
            };
            let location = source
                .section
                .clone()
                .or_else(|| source.page_number.map(|n| format!("page {n}")));
            format!(
                "[{id}] {title}{}\nPassage: {}",
                location
                    .map(|value| format!(" — {value}"))
                    .unwrap_or_default(),
                truncate_for_synthesis(&source.content, WEEK_SYNTHESIS_SOURCE_CHAR_LIMIT)
            )
        })
        .collect::<Vec<_>>();
    format!(
        "Sources (cite only when supporting a claim):\n{}",
        lines.join("\n")
    )
}

fn assign_synthesis_source_ids(entries: &mut [JournalSynthesisEntry]) -> Vec<SourceDto> {
    let mut all_sources = Vec::new();
    let mut ids_by_source = HashMap::new();
    let mut next_id = 1u32;
    for entry in entries {
        let mut entry_ids = HashMap::new();
        for source in &mut entry.sources {
            let key = source_key(source);
            let id = if let Some(id) = ids_by_source.get(&key) {
                *id
            } else {
                let id = next_id;
                next_id = next_id.saturating_add(1);
                ids_by_source.insert(key.clone(), id);
                source.citation_id = Some(id);
                all_sources.push(source.clone());
                id
            };
            source.citation_id = Some(id);
            entry_ids.insert(key, id);
        }
        if !entry.messages.is_empty() {
            let rendered = entry
                .messages
                .iter()
                .map(|message| {
                    let remap = message
                        .citation_keys
                        .iter()
                        .filter_map(|(old, key)| entry_ids.get(key).map(|new| (*old, *new)))
                        .collect::<HashMap<_, _>>();
                    format!(
                        "{}: {}",
                        message.role.to_uppercase(),
                        rewrite_citation_ids(
                            &truncate_for_synthesis(
                                &message.content,
                                JOURNAL_SYNTHESIS_MESSAGE_CHAR_LIMIT
                            ),
                            &remap
                        )
                    )
                })
                .collect::<Vec<_>>()
                .join("\n");
            entry.transcript =
                truncate_for_synthesis(&rendered, JOURNAL_SYNTHESIS_ENTRY_CHAR_LIMIT);
        }
    }
    all_sources
}

fn rewrite_citation_ids(text: &str, remap: &HashMap<u32, u32>) -> String {
    if remap.is_empty() {
        return text.to_string();
    }
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(open) = rest.find('[') {
        out.push_str(&rest[..open]);
        let after_open = &rest[open + 1..];
        let Some(close) = after_open.find(']') else {
            out.push_str(&rest[open..]);
            return out;
        };
        let token = &after_open[..close];
        if let Ok(old_id) = token.parse::<u32>() {
            if let Some(new_id) = remap.get(&old_id) {
                out.push_str(&format!("[{new_id}]"));
            }
        } else {
            out.push_str(&rest[open..open + close + 2]);
        }
        rest = &after_open[close + 1..];
    }
    out.push_str(rest);
    out
}

fn source_keys_by_message(
    messages: &[crate::domain::conversation::ConversationMessage],
) -> (Vec<SourceDto>, Vec<SynthesisMessage>) {
    let mut unique = Vec::new();
    let mut seen = HashSet::new();
    let mut prepared = Vec::new();
    for message in messages {
        let mut citation_keys = HashMap::new();
        if let Some(items) = message
            .metadata
            .as_deref()
            .and_then(|raw| serde_json::from_str::<serde_json::Value>(raw).ok())
            .and_then(|value| value.get("sources").cloned())
        {
            if let Some(items) = items.as_array() {
                for (source_index, item) in items.iter().enumerate() {
                    let Ok(source) = serde_json::from_value::<SourceDto>(item.clone()) else {
                        continue;
                    };
                    // Old messages may lack `citationId`; the chat client has
                    // historically resolved those by their source-list order.
                    let old_id = source.citation_id.unwrap_or((source_index + 1) as u32);
                    let key = source_key(&source);
                    citation_keys.insert(old_id, key.clone());
                    if seen.insert(key) {
                        unique.push(source);
                    }
                }
            }
        }
        let content = message.content.trim();
        if !content.is_empty() {
            prepared.push(SynthesisMessage {
                role: message.role.to_string(),
                content: content.to_string(),
                citation_keys,
            });
        }
    }
    (unique, prepared)
}

fn build_journal_map_prompt(
    entries: &[JournalSynthesisEntry],
    chunk_index: usize,
    chunk_total: usize,
) -> String {
    [
        format!(
            "You are synthesizing journal entry chunk {} of {}.",
            chunk_index, chunk_total
        ),
        "Use only the provided entry transcripts.".to_string(),
        "Goal: capture practical details so the final output can replace re-reading every entry."
            .to_string(),
        "Output markdown with exactly these sections:".to_string(),
        "### Entry Highlights".to_string(),
        "### Practical Details".to_string(),
        "### Decisions & Constraints".to_string(),
        "### Open Questions & Risks".to_string(),
        "### Evidence Notes".to_string(),
        "Rules:".to_string(),
        "- Write concise but information-dense bullets (not generic summaries).".to_string(),
        "- Include concrete values when present (counts, ranges, timing, limits, caveats)."
            .to_string(),
        "- Prefix each bullet with [Entry: <title>].".to_string(),
        "- In Evidence Notes, map each important claim to one or more supporting entry titles."
            .to_string(),
        "- Preserve source citations as numeric markers [n] from the supplied source lists; never renumber, invent, or cite an unavailable number.".to_string(),
        "- If a section has no data, write one bullet: - None identified.".to_string(),
        "".to_string(),
        "Entries:".to_string(),
        format_entries_for_synthesis_prompt(entries),
    ]
    .join("\n")
}

fn build_journal_reduce_prompt(chunk_outputs: &[String]) -> String {
    let chunks = chunk_outputs
        .iter()
        .enumerate()
        .map(|(index, output)| format!("CHUNK SYNTHESIS {}\n{}", index + 1, output))
        .collect::<Vec<_>>()
        .join("\n\n---\n\n");

    [
        "Merge the chunk syntheses into one final journal brief.".to_string(),
        "Use only the chunk syntheses below.".to_string(),
        "Audience: someone who wants to understand and act quickly without reading every entry."
            .to_string(),
        "Output markdown with exactly these sections (in this order):".to_string(),
        "## Executive Summary".to_string(),
        "## Detailed Synthesis".to_string(),
        "## Action Plan".to_string(),
        "## Decisions and Assumptions".to_string(),
        "## Open Questions and Risks".to_string(),
        "## Evidence Map".to_string(),
        "Formatting rules:".to_string(),
        "- Executive Summary: 1 short paragraph (4-6 sentences), plain language.".to_string(),
        "- Detailed Synthesis: 3-6 subsections with '### <Theme>' headers. Under each theme, write one short paragraph plus bullets for critical specifics.".to_string(),
        "- Action Plan: numbered list (3-7 steps) with enough detail to execute.".to_string(),
        "- Decisions and Assumptions: bullets with explicit rationale when available."
            .to_string(),
        "- Open Questions and Risks: bullets with impact noted briefly.".to_string(),
        "- Evidence Map: markdown table with columns `Claim`, `Supporting entries`, `Confidence`."
            .to_string(),
        "- Preserve numeric source citations [n] verbatim from the chunk syntheses; never renumber or invent citations.".to_string(),
        "- Keep the brief concise but comprehensive; avoid fluff and repetition.".to_string(),
        "- If data is missing in a section, write `None identified.`".to_string(),
        "".to_string(),
        chunks,
    ]
    .join("\n")
}

fn extract_latest_assistant_message(chat: &ChatResponse) -> Option<String> {
    chat.messages
        .iter()
        .rev()
        .find(|message| message.role == "assistant" && !message.content.trim().is_empty())
        .map(|message| message.content.trim().to_string())
}

fn normalize_synthesis_scope(raw: Option<&str>) -> String {
    let scope = raw
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("deck")
        .to_lowercase();

    match scope.as_str() {
        "current" | "deck" | "pinned" | "conversation" | "week" => scope,
        _ => "deck".to_string(),
    }
}

fn build_synthesis_citations(entries: &[JournalSynthesisEntry]) -> Vec<SynthesisCitationDto> {
    entries
        .iter()
        .map(|entry| SynthesisCitationDto {
            kind: entry.kind.as_str().to_string(),
            id: entry.source_id.clone(),
            title: entry.title.clone(),
        })
        .collect()
}

fn is_within_week(updated_at: chrono::DateTime<Utc>, now: chrono::DateTime<Utc>) -> bool {
    updated_at >= now - chrono::Duration::days(WEEK_SYNTHESIS_DAYS)
}

fn is_week_synthesis_page(title: &str) -> bool {
    title
        .trim_start()
        .starts_with(WEEK_SYNTHESIS_PAGE_TITLE_PREFIX)
}

fn sort_entries_by_recency(entries: &mut [JournalSynthesisEntry]) {
    entries.sort_by(|left, right| {
        let left_time =
            crate::shared::persistence::timestamps::parse_db_timestamp(&left.updated_at)
                .unwrap_or(chrono::DateTime::<Utc>::MIN_UTC);
        let right_time =
            crate::shared::persistence::timestamps::parse_db_timestamp(&right.updated_at)
                .unwrap_or(chrono::DateTime::<Utc>::MIN_UTC);
        right_time.cmp(&left_time)
    });
}

async fn select_week_entries(
    container: &Container,
    max_entries: usize,
) -> Result<Vec<JournalSynthesisEntry>, ApiError> {
    let now = Utc::now();
    let mut entries: Vec<JournalSynthesisEntry> = Vec::new();

    // (a) Conversations — typed, already ordered updated_at DESC.
    let candidates = conversation::list_conversations_impl(
        container,
        Some(WEEK_SYNTHESIS_CANDIDATE_SCAN),
        Some(0),
    )
    .await
    .map_err(ApiError::from)?;

    let mut used_conversations = 0usize;
    for candidate in candidates {
        if used_conversations >= WEEK_SYNTHESIS_MAX_CONVERSATIONS {
            break;
        }
        if !is_within_week(candidate.updated_at, now) {
            continue;
        }
        let conversation_id = candidate.id.to_string();
        let messages =
            conversation::get_conversation_messages_impl(container, conversation_id.clone())
                .await
                .map_err(ApiError::from)?;
        let transcript = build_synthesis_transcript(&messages);
        let (sources, prompt_messages) = source_keys_by_message(&messages);
        if transcript.is_empty() {
            continue;
        }
        used_conversations += 1;
        entries.push(JournalSynthesisEntry {
            conversation_id: conversation_id.clone(),
            title: candidate.title,
            updated_at: crate::shared::persistence::timestamps::format_db_timestamp(
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
    let cutoff = crate::shared::persistence::timestamps::format_db_timestamp(
        now - chrono::Duration::days(WEEK_SYNTHESIS_DAYS),
    );
    let references = crate::features::references::repository::PassageReferenceRepository::new(
        container.db_pool().clone(),
    )
    .list_created_since(&cutoff, WEEK_SYNTHESIS_MAX_REFERENCES as i64)
    .await
    .map_err(ApiError::from)?;

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
        lines.push(truncate_for_synthesis(
            &record.text,
            WEEK_SYNTHESIS_SOURCE_CHAR_LIMIT,
        ));
        if let Some(note) = record.note.as_deref().filter(|n| !n.trim().is_empty()) {
            lines.push(format!(
                "MY NOTE: {}",
                truncate_for_synthesis(note, WEEK_SYNTHESIS_SOURCE_CHAR_LIMIT)
            ));
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
    let notes_repository = crate::features::daily_notes::repository::DailyNotesRepository::new(
        container.db_pool().clone(),
    );
    let conversation_repository =
        crate::features::conversation::repository::ConversationRepository::new(
            container.db_pool().clone(),
        );
    let notes = notes_repository
        .list_in_date_range(&start_date, &end_date)
        .await
        .map_err(ApiError::from)?;

    let mut used_notes = 0usize;
    for note in notes {
        if used_notes >= WEEK_SYNTHESIS_MAX_NOTES {
            break;
        }
        if note.content.trim().is_empty() || is_week_synthesis_page(&note.title) {
            continue;
        }
        used_notes += 1;
        // Weekly synthesis can consume an old page before the user opens it.
        // Recover and persist its archived citations at this boundary too.
        let note = crate::features::daily_notes::commands::hydrate_workspace_note(
            &notes_repository,
            &conversation_repository,
            note,
        )
        .await
        .map_err(ApiError::from)?;
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
            transcript: format!(
                "JOURNAL PAGE: {}\n{}",
                note.title,
                truncate_for_synthesis(&note.content, WEEK_SYNTHESIS_SOURCE_CHAR_LIMIT)
            ),
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

    sort_entries_by_recency(&mut entries);
    entries.truncate(max_entries);
    Ok(entries)
}

pub async fn synthesize_journal_entries_impl(
    request: SynthesizeJournalEntriesRequestDto,
    container: &Container,
    window: tauri::Window,
    on_progress: tauri::ipc::Channel<SynthesisProgressDto>,
) -> Result<SynthesizeJournalEntriesResponseDto, ApiError> {
    let report = |stage, entry_count, chunk_index, chunk_count| {
        // A closed renderer must not turn a completed generation into an error.
        let _ = on_progress.send(SynthesisProgressDto {
            stage,
            entry_count,
            chunk_index,
            chunk_count,
        });
    };
    report(SynthesisStage::Gathering, None, None, None);
    let normalized_scope = normalize_synthesis_scope(request.scope.as_deref());

    let mut seen = HashSet::new();
    let mut conversation_ids = Vec::new();
    for id in request.conversation_ids {
        let normalized = id.trim();
        if normalized.is_empty() {
            continue;
        }
        if seen.insert(normalized.to_string()) {
            conversation_ids.push(normalized.to_string());
        }
    }

    let max_entries = request
        .max_entries
        .unwrap_or(JOURNAL_SYNTHESIS_ENTRY_LIMIT_DEFAULT)
        .clamp(1, JOURNAL_SYNTHESIS_ENTRY_LIMIT_MAX);

    // The "week" scope selects server-side, so it carries no conversation ids.
    // Every other scope still requires an explicit selection.
    let entries = if normalized_scope == "week" {
        select_week_entries(container, max_entries).await?
    } else {
        if conversation_ids.is_empty() {
            return Err(ApiError::from(AppError::InvalidInput(
                "At least one conversation ID is required for journal synthesis".to_string(),
            )));
        }

        if conversation_ids.len() > max_entries {
            conversation_ids.truncate(max_entries);
        }

        let mut selected = Vec::new();
        for conversation_id in &conversation_ids {
            let conversation =
                conversation::get_conversation_impl(container, conversation_id.clone())
                    .await
                    .map_err(ApiError::from)?;
            let Some(conversation) = conversation else {
                continue;
            };

            let messages =
                conversation::get_conversation_messages_impl(container, conversation_id.clone())
                    .await
                    .map_err(ApiError::from)?;
            let transcript = build_synthesis_transcript(&messages);
            let (sources, prompt_messages) = source_keys_by_message(&messages);
            if transcript.is_empty() {
                continue;
            }

            let id = conversation.id.to_string();
            selected.push(JournalSynthesisEntry {
                conversation_id: id.clone(),
                title: conversation.title,
                updated_at: conversation.updated_at.to_rfc3339(),
                message_count: messages.len(),
                transcript,
                kind: SynthesisEntryKind::Conversation,
                source_id: id,
                sources,
                messages: prompt_messages,
            });
        }
        selected
    };

    if entries.is_empty() {
        let message = if normalized_scope == "week" {
            "Nothing from the past week to synthesize."
        } else {
            "No synthesizable message content found in selected conversations"
        };
        return Err(ApiError::from(AppError::InvalidInput(message.to_string())));
    }

    let mut entries = entries;
    let sources = assign_synthesis_source_ids(&mut entries);
    let chunks = chunk_journal_synthesis_entries(&entries);
    let tool_preferences = ToolPreferences {
        knowledge_base: false,
        web_search: false,
        deep_research_mode: false,
        followup_mode: true,
        turn_mode: Some("followup".to_string()),
        enabled_tools: None,
        focus_document_ids: None,
        explorer_focus: None,
        // The prompts say "use only the provided entry transcripts", and the
        // turn runs in a scratch conversation that belongs to no space the
        // entries came from. Anything retrieved for it is by definition from
        // the wrong place.
        closed_book: true,
    };

    let mut synthesis_conversation_id: Option<String> = None;
    let synthesis_result = async {
        let mut map_outputs = Vec::new();

        for (chunk_index, chunk) in chunks.iter().enumerate() {
            report(
                SynthesisStage::Reading,
                Some(entries.len()),
                Some(chunk_index + 1),
                Some(chunks.len()),
            );
            let map_prompt = build_journal_map_prompt(chunk, chunk_index + 1, chunks.len());
            let map_response = run_chat_with_conversation_impl(
                container,
                synthesis_conversation_id.clone(),
                map_prompt,
                Some(tool_preferences.clone()),
                None,
                None,
                None,
                None,
                window.clone(),
            )
            .await
            .map_err(ApiError::from)?;

            synthesis_conversation_id = Some(map_response.conversation_id.clone());
            let map_output = extract_latest_assistant_message(&map_response).ok_or_else(|| {
                ApiError::from(AppError::Other(
                    "Synthesis map stage returned no assistant output".to_string(),
                ))
            })?;
            map_outputs.push(map_output);
        }

        report(
            SynthesisStage::Writing,
            Some(entries.len()),
            None,
            Some(chunks.len()),
        );
        let reduce_prompt = build_journal_reduce_prompt(&map_outputs);
        let reduce_response = run_chat_with_conversation_impl(
            container,
            synthesis_conversation_id.clone(),
            reduce_prompt,
            Some(tool_preferences.clone()),
            None,
            None,
            None,
            None,
            window.clone(),
        )
        .await
        .map_err(ApiError::from)?;

        synthesis_conversation_id = Some(reduce_response.conversation_id.clone());
        let synthesis = extract_latest_assistant_message(&reduce_response).ok_or_else(|| {
            ApiError::from(AppError::Other(
                "Synthesis reduce stage returned no assistant output".to_string(),
            ))
        })?;

        Ok::<SynthesizeJournalEntriesResponseDto, ApiError>(SynthesizeJournalEntriesResponseDto {
            synthesis,
            scope: normalized_scope.clone(),
            entry_count: entries.len(),
            chunk_count: chunks.len(),
            conversation_ids: entries
                .iter()
                .filter(|entry| entry.kind == SynthesisEntryKind::Conversation)
                .map(|entry| entry.conversation_id.clone())
                .collect(),
            citations: build_synthesis_citations(&entries),
            sources: sources.clone(),
        })
    }
    .await;

    if let Some(temp_conversation_id) = synthesis_conversation_id {
        let _ = conversation::delete_conversation_impl(container, temp_conversation_id).await;
    }

    synthesis_result
}
#[cfg(test)]
mod week_scope_tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]

    use super::*;
    use chrono::TimeZone;

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
            raw_messages.push(crate::domain::conversation::ConversationMessage {
                id: format!("message-{index}"),
                conversation_id: Default::default(),
                role: crate::domain::conversation::MessageRole::Assistant,
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
        let message = crate::domain::conversation::ConversationMessage {
            id: "message-1".into(),
            conversation_id: Default::default(),
            role: crate::domain::conversation::MessageRole::Assistant,
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
}
