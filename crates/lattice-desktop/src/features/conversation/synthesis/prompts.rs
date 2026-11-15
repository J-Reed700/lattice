//! What a synthesis writes from: entry transcripts, the numbering of their
//! sources, and the chunks the map reads.
use super::super::workspace_dto::SynthesisCitationDto;
use super::notes::{map_overhead, notes_chars, part_size, synthesis_overhead, EntryPart, Note};
use super::{source_key, JournalSynthesisEntry, SourceDto, SynthesisMessage};
use lattice_core::shared::error::Result;
use lattice_core::shared::text::split_into_parts;
use std::collections::{HashMap, HashSet};

pub(super) fn build_synthesis_transcript(
    messages: &[lattice_core::domain::conversation::ConversationMessage],
) -> String {
    let mut lines = Vec::new();
    for message in messages {
        let text = message.text_without_references();
        let content = text.trim();
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

/// The map chunks for `entries`, each within `part_chars`.
///
/// Every entry is carried whole, in order. Entries are packed into chunks that
/// fit; an entry too long for a chunk of its own is split into consecutive
/// pieces, each in a chunk, so the synthesis reads all of it rather than its
/// opening.
pub(super) fn plan_journal_map_chunks(
    entries: &[JournalSynthesisEntry],
    part_chars: usize,
) -> Result<Vec<Vec<EntryPart>>> {
    const SEPARATOR: &str = "\n\n---\n\n";
    let room = part_chars.saturating_sub(map_overhead()?).max(1);
    let mut parts: Vec<EntryPart> = Vec::new();
    for (index, entry) in entries.iter().enumerate() {
        let whole = EntryPart::of(index, entry, 1, 1, entry.transcript.clone(), &entry.sources);
        if part_size(&whole)? <= room {
            parts.push(whole);
            continue;
        }
        let framing = part_size(&EntryPart::of(index, entry, 1, 1, String::new(), &[]))?;
        let with_sources = EntryPart::of(index, entry, 1, 1, String::new(), &entry.sources);
        let sources = part_size(&with_sources)?.saturating_sub(framing);
        let text_room = room.saturating_sub(framing);
        let text_room = match text_room.saturating_sub(sources) {
            0 => text_room,
            left => left,
        };
        let pieces = split_into_parts(&entry.transcript, text_room.max(1));
        let total = pieces.len();
        let cited: Vec<Vec<u32>> = pieces.iter().map(|piece| cited_ids(piece)).collect();
        for (at, piece) in pieces.into_iter().enumerate() {
            let own: Vec<SourceDto> = entry
                .sources
                .iter()
                .filter(|source| {
                    let id = source.citation_id.unwrap_or_default();
                    let here = cited.get(at).is_some_and(|ids| ids.contains(&id));
                    let nowhere = at == 0 && !cited.iter().any(|ids| ids.contains(&id));
                    here || nowhere
                })
                .cloned()
                .collect();
            parts.push(EntryPart::of(index, entry, at + 1, total, piece, &own));
        }
    }

    let mut chunks: Vec<Vec<EntryPart>> = Vec::new();
    let mut current: Vec<EntryPart> = Vec::new();
    let mut current_chars = 0usize;
    for part in parts {
        let size = part_size(&part)? + SEPARATOR.len();
        if !current.is_empty() && current_chars + size > room {
            chunks.push(std::mem::take(&mut current));
            current_chars = 0;
        }
        current_chars += size;
        current.push(part);
    }
    if !current.is_empty() {
        chunks.push(current);
    }
    Ok(chunks)
}

/// The ids `[n]` a stretch of the app's own transcript cites.
fn cited_ids(text: &str) -> Vec<u32> {
    let mut ids = Vec::new();
    let mut rest = text;
    while let Some(open) = rest.find('[') {
        let after = &rest[open + 1..];
        let Some(close) = after.find(']') else {
            break;
        };
        if let Ok(id) = after[..close].parse::<u32>() {
            ids.push(id);
        }
        rest = &after[close + 1..];
    }
    ids
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

pub(super) fn rewrite_citation_ids<V: std::fmt::Display>(
    text: &str,
    remap: &HashMap<u32, V>,
) -> String {
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
        let text = message.text_without_references();
        let content = text.trim();
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

/// `outputs` in consecutive groups whose merge request fits `part_chars`.
/// Always fewer groups than outputs when there are two or more, so merging
/// level by level ends in one: notes too long to share a request are merged in
/// pairs, and a request that then does not fit fails at the model rather than
/// being cut here.
pub(super) fn group_for_merge(
    outputs: &[Vec<Note>],
    titles: &[String],
    part_chars: usize,
) -> Result<Vec<Vec<Vec<Note>>>> {
    let room = part_chars.saturating_sub(synthesis_overhead()?);
    let mut groups: Vec<Vec<Vec<Note>>> = Vec::new();
    let mut current: Vec<Vec<Note>> = Vec::new();
    let mut used = 0usize;
    for output in outputs {
        // "CHUNK SYNTHESIS n" and the separator.
        let size = notes_chars(output, titles) + 32;
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
        return Ok(outputs.chunks(2).map(<[Vec<Note>]>::to_vec).collect());
    }
    Ok(groups)
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
    use super::super::week_scope_tests::source;
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

    fn texts(chunks: &[Vec<EntryPart>]) -> String {
        chunks
            .iter()
            .flatten()
            .map(|part| part.transcript.0.clone())
            .collect()
    }

    /// An entry is read whole, split into marked parts across chunks when one
    /// chunk cannot hold it.
    #[test]
    fn a_long_entry_is_read_whole_across_marked_parts() {
        let transcript: String = (0..4_000)
            .map(|index| format!("USER: line {index}\n"))
            .collect();
        let entries = vec![entry("Short", "USER: hello"), entry("Long", &transcript)];
        let part_chars = map_overhead().unwrap() + 8_000;
        let chunks = plan_journal_map_chunks(&entries, part_chars).unwrap();

        assert!(chunks.len() > 2, "{} chunks", chunks.len());
        let text = texts(&chunks);
        assert!(text.contains("USER: hello"));
        for index in [0, 1_999, 3_999] {
            let line = format!("USER: line {index}\n");
            assert!(text.contains(&line), "{line} never reached a chunk");
        }
        let long: Vec<&EntryPart> = chunks
            .iter()
            .flatten()
            .filter(|part| *part.entry.value() == 1)
            .collect();
        assert!(long.len() > 1);
        assert!(long.iter().all(|part| part.parts.0 == long.len()));
        assert_eq!(long.last().map(|part| part.part.0), Some(long.len()));
    }

    /// Entries that fit one chunk still make one chunk.
    #[test]
    fn entries_that_fit_make_one_chunk() {
        let entries = vec![entry("A", "USER: one"), entry("B", "USER: two")];
        let chunks = plan_journal_map_chunks(&entries, 100_000).unwrap();
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].len(), 2);
        assert!(chunks[0].iter().all(|part| part.parts.0 == 1));
    }

    /// A piece of a long entry carries the sources its text cites, and a
    /// source no piece cites goes with the first.
    #[test]
    fn a_piece_carries_the_sources_it_cites() {
        let mut long = entry("Long", "");
        long.transcript = (0..400)
            .map(|index| {
                format!(
                    "USER: line {index} [{}]\n",
                    if index < 200 { 1 } else { 2 }
                )
            })
            .collect();
        long.sources = vec![
            source("doc-a", "chunk-a", "Tide table", 1),
            source("doc-b", "chunk-b", "Kiln log", 2),
            source("doc-c", "chunk-c", "Harbour chart", 3),
        ];
        let part_chars = map_overhead().unwrap() + 4_000;
        let chunks = plan_journal_map_chunks(&[long], part_chars).unwrap();
        let parts: Vec<&EntryPart> = chunks.iter().flatten().collect();

        assert!(parts.len() > 1);
        let ids = |part: &EntryPart| -> Vec<u32> {
            part.sources
                .iter()
                .map(|source| *source.id.value())
                .collect()
        };
        assert_eq!(ids(parts[0]), [1, 3]);
        let last = ids(parts[parts.len() - 1]);
        assert!(last.contains(&2) && !last.contains(&1));
    }
}
