//! The text a synthesis writes from: entry transcripts, the map and reduce
//! prompts, and the citations carried across them.
use super::super::chat::ChatResponse;
use super::super::workspace_dto::SynthesisCitationDto;
use super::{source_key, JournalSynthesisEntry, SourceDto, SynthesisMessage};
use lattice_core::shared::text::split_into_parts;
use std::collections::{HashMap, HashSet};

pub(super) fn build_synthesis_transcript(
    messages: &[lattice_core::domain::conversation::ConversationMessage],
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
            content
        ));
    }

    lines.join("\n")
}

/// The map prompts for `entries`, each within `part_chars`.
///
/// Every entry is carried whole, in order. Entries are packed into parts that
/// fit; an entry too long for a part of its own is split into consecutive
/// pieces marked `part i of n`, each in a part, so the synthesis reads all of
/// it rather than its opening.
pub(super) fn plan_journal_map_prompts(
    entries: &[JournalSynthesisEntry],
    part_chars: usize,
) -> Vec<String> {
    const SEPARATOR: &str = "\n\n---\n\n";
    // The instructions around the entries, with room for the part numbers.
    let instructions = build_journal_map_prompt(&[], 1_000, 1_000).chars().count();
    let room = part_chars.saturating_sub(instructions).max(1);
    let mut blocks: Vec<String> = Vec::new();
    for (index, entry) in entries.iter().enumerate() {
        let block = format_entry_for_synthesis_prompt(index + 1, entry);
        if block.chars().count() <= room {
            blocks.push(block);
            continue;
        }
        let mark_room = format!("ENTRY {} (part 1000 of 1000, continued)\n", index + 1).len();
        let pieces = split_into_parts(&block, room.saturating_sub(mark_room).max(1));
        let total = pieces.len();
        blocks.extend(pieces.into_iter().enumerate().map(|(part, piece)| {
            format!(
                "ENTRY {} (part {} of {total}{})\n{piece}",
                index + 1,
                part + 1,
                if part > 0 { ", continued" } else { "" }
            )
        }));
    }

    let mut chunks: Vec<Vec<String>> = Vec::new();
    let mut current: Vec<String> = Vec::new();
    let mut current_chars = 0usize;
    for block in blocks {
        let size = block.chars().count() + SEPARATOR.len();
        if !current.is_empty() && current_chars + size > room {
            chunks.push(std::mem::take(&mut current));
            current_chars = 0;
        }
        current_chars += size;
        current.push(block);
    }
    if !current.is_empty() {
        chunks.push(current);
    }

    let total = chunks.len();
    chunks
        .iter()
        .enumerate()
        .map(|(index, chunk)| build_journal_map_prompt(chunk, index + 1, total))
        .collect()
}

fn format_entry_for_synthesis_prompt(number: usize, entry: &JournalSynthesisEntry) -> String {
    [
        format!("ENTRY {number}"),
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
                source.content
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
                        rewrite_citation_ids(&message.content, &remap)
                    )
                })
                .collect::<Vec<_>>()
                .join("\n");
            entry.transcript = rendered;
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
    messages: &[lattice_core::domain::conversation::ConversationMessage],
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
    entries: &[String],
    chunk_index: usize,
    chunk_total: usize,
) -> String {
    [
        format!(
            "Below are journal entries, chunk {chunk_index} of {chunk_total}. You will synthesize them."
        ),
        "".to_string(),
        "Entries:".to_string(),
        entries.join("\n\n---\n\n"),
        "".to_string(),
        format!("Now write the synthesis of these entries (chunk {chunk_index} of {chunk_total})."),
        "Use only the entry transcripts above.".to_string(),
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
        "- An entry marked (part i of n) continues across chunks; describe the part you have."
            .to_string(),
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
        "Below are chunk syntheses of a journal. You will merge them into one final brief."
            .to_string(),
        "".to_string(),
        chunks,
        "".to_string(),
        "Now merge the chunk syntheses above into one final journal brief.".to_string(),
        "Use only the chunk syntheses above.".to_string(),
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
        "- Keep every name, date, figure and limit the chunk syntheses state; cut repetition, not detail.".to_string(),
        "- If data is missing in a section, write `None identified.`".to_string(),
    ]
    .join("\n")
}

/// An intermediate merge, for chunk notes too many for one final brief:
/// several chunk syntheses become one, in the same sections, so the final
/// brief reads merged notes exactly as it reads a chunk's.
pub(super) fn build_journal_merge_prompt(
    chunk_outputs: &[String],
    group_index: usize,
    group_total: usize,
) -> String {
    let chunks = chunk_outputs
        .iter()
        .enumerate()
        .map(|(index, output)| format!("CHUNK SYNTHESIS {}\n{}", index + 1, output))
        .collect::<Vec<_>>()
        .join("\n\n---\n\n");
    [
        format!(
            "Merge these chunk syntheses into one chunk synthesis (group {group_index} of {group_total})."
        ),
        "Use only the chunk syntheses below; keep every concrete detail, decision, open question and evidence note.".to_string(),
        "Output markdown with exactly these sections:".to_string(),
        "### Entry Highlights".to_string(),
        "### Practical Details".to_string(),
        "### Decisions & Constraints".to_string(),
        "### Open Questions & Risks".to_string(),
        "### Evidence Notes".to_string(),
        "Rules:".to_string(),
        "- Keep the [Entry: <title>] prefix on each bullet.".to_string(),
        "- Preserve numeric source citations [n] verbatim; never renumber or invent citations.".to_string(),
        "- Merge duplicates; drop nothing else.".to_string(),
        "- If a section has no data, write one bullet: - None identified.".to_string(),
        "".to_string(),
        chunks,
    ]
    .join("\n")
}

/// `outputs` in consecutive groups whose merge prompt fits `part_chars`.
/// Always fewer groups than outputs when there are two or more, so merging
/// level by level ends in one: notes too long to share a prompt are merged in
/// pairs, and a request that then does not fit fails at the model rather than
/// being cut here.
pub(super) fn group_for_merge(outputs: &[String], part_chars: usize) -> Vec<Vec<String>> {
    let instructions = build_journal_reduce_prompt(&[]).chars().count().max(
        build_journal_merge_prompt(&[], 1_000, 1_000)
            .chars()
            .count(),
    );
    let room = part_chars.saturating_sub(instructions);
    let mut groups: Vec<Vec<String>> = Vec::new();
    let mut current: Vec<String> = Vec::new();
    let mut used = 0usize;
    for output in outputs {
        // "CHUNK SYNTHESIS n" and the separator.
        let size = output.chars().count() + 32;
        if !current.is_empty() && used + size > room {
            groups.push(std::mem::take(&mut current));
            used = 0;
        }
        used += size;
        current.push(output.clone());
    }
    if !current.is_empty() {
        groups.push(current);
    }
    if outputs.len() > 1 && groups.len() == outputs.len() {
        return outputs.chunks(2).map(<[String]>::to_vec).collect();
    }
    groups
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

#[cfg(test)]
mod tests {
    use super::super::SynthesisEntryKind;
    use super::*;

    fn entry(title: &str, transcript: &str) -> JournalSynthesisEntry {
        JournalSynthesisEntry {
            conversation_id: title.to_string(),
            title: title.to_string(),
            updated_at: "2026-09-05T10:00:00.000Z".to_string(),
            message_count: 1,
            transcript: transcript.to_string(),
            kind: SynthesisEntryKind::Conversation,
            source_id: title.to_string(),
            sources: Vec::new(),
            messages: Vec::new(),
        }
    }

    /// An entry is read whole, split into marked parts across prompts when one
    /// prompt cannot hold it.
    #[test]
    fn a_long_entry_is_read_whole_across_marked_parts() {
        let transcript: String = (0..4_000)
            .map(|index| format!("USER: line {index}\n"))
            .collect();
        let entries = vec![entry("Short", "USER: hello"), entry("Long", &transcript)];
        let part_chars = build_journal_map_prompt(&[], 1_000, 1_000).chars().count() + 8_000;
        let prompts = plan_journal_map_prompts(&entries, part_chars);

        assert!(prompts.len() > 2, "{} prompts", prompts.len());
        assert!(prompts
            .iter()
            .all(|prompt| prompt.chars().count() <= part_chars));
        assert!(prompts[0].contains("USER: hello"));
        for index in [0, 1_999, 3_999] {
            let line = format!("USER: line {index}\n");
            assert!(
                prompts.iter().any(|prompt| prompt.contains(&line)),
                "{line} never reached a prompt"
            );
        }
        assert!(prompts
            .iter()
            .any(|prompt| prompt.contains("ENTRY 2 (part 2 of")));
    }

    #[test]
    fn map_and_reduce_rules_follow_their_material() {
        let map = build_journal_map_prompt(&["ENTRY 1".to_string()], 1, 1);
        let reduce = build_journal_reduce_prompt(&["notes".to_string()]);

        assert!(map.find("ENTRY 1") < map.find("Output markdown"));
        assert!(reduce.find("CHUNK SYNTHESIS 1") < reduce.find("Output markdown"));
    }

    /// Entries that fit one prompt still make one prompt, as before.
    #[test]
    fn entries_that_fit_make_one_prompt() {
        let entries = vec![entry("A", "USER: one"), entry("B", "USER: two")];
        let prompts = plan_journal_map_prompts(&entries, 100_000);
        assert_eq!(prompts.len(), 1);
        assert!(prompts[0].contains("ENTRY 1") && prompts[0].contains("ENTRY 2"));
        assert!(!prompts[0].contains("ENTRY 1 (part") && !prompts[0].contains("ENTRY 2 (part"));
    }
}
