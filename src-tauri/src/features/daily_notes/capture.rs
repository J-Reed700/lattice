//! Pure capture transformations. Persistence owns the read/modify/write transaction.
use super::dto::{
    CaptureReferenceRequestDto, ConversationSnapshotDto, SnapshotMessageDto, WorkspaceNoteDto,
};
use crate::features::qa::dto::SourceDto;
use crate::shared::error::{AppError, Result};

/// The text a capture will append, or `InvalidInput` when there is nothing.
///
/// Pure, so the empty-input rule is testable without a `Container`.
pub(super) fn capture_snippet(content: &str) -> Result<&str> {
    let snippet = content.trim();
    if snippet.is_empty() {
        return Err(AppError::InvalidInput("Nothing to capture".to_string()));
    }
    Ok(snippet)
}

/// A capture appended to a page keeps one blank line between the old text and
/// the new; a capture onto an empty page is the page.
pub(super) fn appended_capture(existing: &str, snippet: &str) -> String {
    if existing.trim().is_empty() {
        snippet.to_string()
    } else {
        format!("{}\n\n{}", existing, snippet)
    }
}

/// Remap citation IDs in an appended synthesis when they overlap IDs already
/// used by the note. This keeps existing links stable while retaining all
/// metadata from the incoming source objects.
pub(super) fn append_sources_and_remap_citations(
    existing_sources: &[SourceDto],
    snippet: &str,
    incoming_sources: Vec<SourceDto>,
) -> (String, Vec<SourceDto>) {
    let mut used: std::collections::HashSet<u32> = existing_sources
        .iter()
        .filter_map(|source| source.citation_id)
        .collect();
    // Keep every incoming ID reserved too, so assigning a replacement for a
    // collision cannot steal an ID that another incoming source already uses.
    used.extend(
        incoming_sources
            .iter()
            .filter_map(|source| source.citation_id),
    );
    let mut next_id = used
        .iter()
        .copied()
        .max()
        .and_then(|id| id.checked_add(1))
        .unwrap_or(1);
    let mut remapped = std::collections::HashMap::new();
    let existing_ids: std::collections::HashSet<u32> = existing_sources
        .iter()
        .filter_map(|source| source.citation_id)
        .collect();

    for source in &incoming_sources {
        let Some(citation_id) = source.citation_id else {
            continue;
        };
        if !existing_ids.contains(&citation_id) {
            continue;
        }
        if remapped.contains_key(&citation_id) {
            continue;
        }
        while used.contains(&next_id) {
            next_id = next_id.checked_add(1).unwrap_or(1);
        }
        remapped.insert(citation_id, next_id);
        used.insert(next_id);
        next_id = next_id.checked_add(1).unwrap_or(1);
    }

    let content = if remapped.is_empty() {
        snippet.to_string()
    } else {
        // A single pass prevents replacement chains such as [1] -> [3] then
        // [3] -> [4] from changing the new citation a second time.
        if let Ok(pattern) = regex::Regex::new(r"\[(\d+)\]") {
            pattern
                .replace_all(snippet, |captures: &regex::Captures<'_>| {
                    captures[1]
                        .parse::<u32>()
                        .ok()
                        .and_then(|id| remapped.get(&id).copied())
                        .map(|id| format!("[{id}]"))
                        .unwrap_or_else(|| captures[0].to_string())
                })
                .into_owned()
        } else {
            snippet.to_string()
        }
    };

    let incoming_sources = incoming_sources
        .into_iter()
        .map(|source| {
            let mut source = source;
            if let Some(old_id) = source.citation_id {
                if let Some(new_id) = remapped.get(&old_id) {
                    source.citation_id = Some(*new_id);
                }
            }
            source
        })
        .collect();
    (content, incoming_sources)
}

fn capture_identity(value: &str) -> String {
    // Keep the existing capture identities stable when reading saved snapshots.
    let mut id = String::new();
    let mut replacing = false;
    for ch in value.chars() {
        if ch.is_ascii_alphanumeric() || ch == '_' || ch == '-' {
            id.push(ch);
            replacing = false;
        } else if !replacing {
            id.push('_');
            replacing = true;
        }
    }
    id.truncate(id.len().min(72));
    id
}

pub(super) fn apply_reference(
    note: &mut WorkspaceNoteDto,
    request: &CaptureReferenceRequestDto,
) -> Result<Option<String>> {
    capture_snippet(&request.message_content)?;
    let conversation = capture_identity(&request.conversation_id);
    let message = capture_identity(&request.message_id);
    let marker = format!("{conversation}::{message}");
    let pattern = regex::Regex::new(&format!(
        r"(?s)<!-- lattice-capture-start:{} -->.*?<!-- lattice-capture-end:{} -->",
        regex::escape(&marker),
        regex::escape(&marker)
    ))
    .map_err(|error| AppError::InternalError(error.to_string()))?;
    let content = pattern.replace_all(&note.content, regex::NoExpand(&request.message_content));
    let legacy = regex::Regex::new(
        r"(?s)<!--\s*lattice-capture-start:[^>]+-->\s*(.*?)\s*<!--\s*lattice-capture-end:[^>]+-->",
    )
    .map_err(|error| AppError::InternalError(error.to_string()))?;
    let content = legacy.replace_all(&content, |capture: &regex::Captures<'_>| {
        let inner = capture[1].replace("\r\n", "\n");
        inner
            .split_once("### Message")
            .map(|(_, text)| text)
            .unwrap_or(&inner)
            .trim()
            .to_string()
    });
    let content = content.trim_end();
    note.content = if content.contains(&request.message_content) {
        content.to_string()
    } else if content.is_empty() {
        request.message_content.clone()
    } else {
        format!("{content}\n\n{}", request.message_content)
    };
    for id in &request.document_ids {
        let id = id.trim();
        if !id.is_empty()
            && !note
                .linked_document_ids
                .iter()
                .any(|existing| existing == id)
        {
            note.linked_document_ids.push(id.to_string());
        }
    }
    if !note
        .linked_conversation_ids
        .contains(&request.conversation_id)
    {
        note.linked_conversation_ids
            .push(request.conversation_id.clone());
    }
    let snapshot_id = format!("capture_{conversation}_{message}");
    if request.add_snapshot {
        note.conversation_snapshots
            .retain(|snapshot| snapshot.id != snapshot_id);
        note.conversation_snapshots.insert(
            0,
            ConversationSnapshotDto {
                id: snapshot_id.clone(),
                conversation_id: request.conversation_id.clone(),
                conversation_title: request.conversation_title.clone(),
                captured_at: request.captured_at.clone(),
                message_count: 1,
                messages: vec![SnapshotMessageDto {
                    id: request.message_id.clone(),
                    role: request.message_role.clone(),
                    content: request.message_content.clone(),
                    created_at: request.captured_at.clone(),
                    metadata: None,
                }],
            },
        );
        note.conversation_snapshots.truncate(120);
    }
    Ok(request.add_snapshot.then_some(snapshot_id))
}
