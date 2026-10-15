//! The text a synthesis writes from: entry transcripts, the map and reduce
//! prompts, and the citations carried across them.
use super::super::chat::ChatResponse;
use super::super::workspace_dto::SynthesisCitationDto;
use super::{
    source_key, JournalSynthesisEntry, SourceDto, SynthesisMessage,
    JOURNAL_SYNTHESIS_CHUNK_CHAR_LIMIT, JOURNAL_SYNTHESIS_ENTRY_CHAR_LIMIT,
    JOURNAL_SYNTHESIS_MESSAGE_CHAR_LIMIT, WEEK_SYNTHESIS_SOURCE_CHAR_LIMIT,
};
use std::collections::{HashMap, HashSet};

pub(super) fn truncate_for_synthesis(text: &str, max_chars: usize) -> String {
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

pub(super) fn build_synthesis_transcript(
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

pub(super) fn chunk_journal_synthesis_entries(
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

pub(super) fn format_synthesis_sources(sources: &[SourceDto]) -> String {
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

pub(super) fn assign_synthesis_source_ids(entries: &mut [JournalSynthesisEntry]) -> Vec<SourceDto> {
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

pub(super) fn rewrite_citation_ids(text: &str, remap: &HashMap<u32, u32>) -> String {
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

pub(super) fn source_keys_by_message(
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

pub(super) fn build_journal_map_prompt(
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

pub(super) fn build_journal_reduce_prompt(chunk_outputs: &[String]) -> String {
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

pub(super) fn extract_latest_assistant_message(chat: &ChatResponse) -> Option<String> {
    chat.messages
        .iter()
        .rev()
        .find(|message| message.role == "assistant" && !message.content.trim().is_empty())
        .map(|message| message.content.trim().to_string())
}

pub(super) fn normalize_synthesis_scope(raw: Option<&str>) -> String {
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

pub(super) fn build_synthesis_citations(
    entries: &[JournalSynthesisEntry],
) -> Vec<SynthesisCitationDto> {
    entries
        .iter()
        .map(|entry| SynthesisCitationDto {
            kind: entry.kind.as_str().to_string(),
            id: entry.source_id.clone(),
            title: entry.title.clone(),
        })
        .collect()
}
