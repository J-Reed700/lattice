//! The `read_conversation_history` tool.
use super::{
    clip_to_bytes, floor_boundary, FunctionResult, HistoryToolMemo, HistoryToolScope, ReadArgs,
    ReadCursor, MAX_READ_MESSAGES, MIN_RESPONSE_BYTES,
};
use lattice_core::application::ports::conversation_memory::{
    ConversationMemoryReadPort, SourceReadLimits,
};
use lattice_core::domain::conversation::memory::SourceMessage;
use lattice_core::shared::error::Result;

pub(super) async fn execute_read(
    port: &dyn ConversationMemoryReadPort,
    scope: &HistoryToolScope,
    memo: &mut HistoryToolMemo,
    arguments: &serde_json::Value,
) -> Result<FunctionResult> {
    let args: ReadArgs = serde_json::from_value(arguments.clone()).unwrap_or_default();
    let wanted_ids: Vec<String> = args
        .message_ids
        .iter()
        .map(|id| id.trim().to_string())
        .filter(|id| !id.is_empty())
        .take(MAX_READ_MESSAGES)
        .collect();
    let by_id = !wanted_ids.is_empty();
    let by_range = args.from_sequence.is_some();
    if by_id && by_range {
        return Ok(FunctionResult::error(
            "AMBIGUOUS_RANGE",
            "Give either message_ids or from_sequence, not both: the two name different text \
             and the result could not be labelled honestly.",
        ));
    }
    if !by_id && !by_range {
        return Ok(FunctionResult::error(
            "NOTHING_REQUESTED",
            "Give message_ids from a search result, or from_sequence to read an interval.",
        ));
    }

    // What this turn already sent for this exact range. Read out before any
    // mutation so the lookup and the later update cannot disagree.
    let key = read_key(by_id, &wanted_ids, &args);
    let previous = memo.reads.get(&key);
    let exhausted = previous.is_some_and(|progress| progress.exhausted);
    let first_start = previous.map(|progress| progress.first_start).unwrap_or(0);
    let memo_cursor = previous.and_then(|progress| progress.next);
    let requested_start = args.start_byte.unwrap_or(0);
    if exhausted && (args.start_byte.is_none() || requested_start == first_start) {
        // §9.3: an identical, already-exhausted range gets a reference, not the
        // same text a second time.
        return Ok(FunctionResult::success(serde_json::json!({
            "conversation_scope": "current",
            "already_delivered": true,
            "message_ids": previous.map(|p| p.message_ids.clone()).unwrap_or_default(),
            "sequences": previous.map(|p| p.sequences.clone()).unwrap_or_default(),
            "note": "You already have the full text of this range earlier in this turn. \
                     Re-read it there, or request a different range.",
        })));
    }
    // An explicit offset is the model being deliberate and wins; otherwise a
    // repeat of a truncated range continues where it stopped rather than
    // re-sending what the model already has.
    let resume = if args.start_byte.is_some() {
        None
    } else {
        memo_cursor
    };
    let mut offset = args
        .start_byte
        .or(resume.map(|cursor| cursor.start_byte))
        .unwrap_or(0) as usize;

    let turn_remaining = memo.turn_remaining();
    if turn_remaining < MIN_RESPONSE_BYTES {
        return Ok(FunctionResult::error(
            "TURN_BUDGET_EXHAUSTED",
            "This turn has spent its history-reading budget. Answer from what you have and \
             name what you could not check.",
        ));
    }
    let budget = scope
        .budget
        .resolve_bytes(args.max_bytes)
        .min(turn_remaining);
    // The port's ceilings stay at the §14 defaults: the *response* budget is
    // applied below, so a single message larger than the budget still arrives
    // and can be clipped into a resumable cursor instead of vanishing.
    let limits = SourceReadLimits::new(MAX_READ_MESSAGES, SourceReadLimits::DEFAULT.max_bytes);

    let mut messages = if by_id {
        port.read_messages(scope.conversation_id(), &wanted_ids, limits)
            .await?
    } else {
        let from = resume
            .map(|cursor| cursor.from_sequence)
            .or(args.from_sequence)
            .unwrap_or_default();
        let to = args.to_sequence.unwrap_or(i64::MAX);
        port.read_sequence_range(scope.conversation_id(), from, to, limits)
            .await?
            .messages
    };
    // The port cap bound the fetch, so more of this range probably exists. An
    // observation, not a guess: the alternative is comparing against a
    // requested end that may be past the end of the conversation.
    let port_capped = messages.len() >= MAX_READ_MESSAGES;
    if let Some(cursor) = resume.filter(|_| by_id) {
        messages.retain(|message| message.sequence >= cursor.from_sequence);
    }

    // A failed assistant generation was never an answer this conversation gave.
    // Handing it back as history would attribute an abandoned draft to the
    // assistant, so it is dropped and counted rather than shown (§6.2).
    let before = messages.len();
    messages.retain(SourceMessage::is_eligible_source);
    let omitted_failed_outputs = before - messages.len();

    let delivered_ids: Vec<String> = messages.iter().map(|m| m.id.clone()).collect();
    let delivered_sequences: Vec<i64> = messages.iter().map(|m| m.sequence).collect();
    // An id from another conversation never comes back from the port, so its
    // absence here is the whole answer the model gets about it. A suffix the
    // budget could not carry is absent for a different reason, and the label
    // covers both rather than claiming to know which.
    let not_readable: Vec<String> = wanted_ids
        .iter()
        .filter(|id| !delivered_ids.iter().any(|found| found == *id))
        .cloned()
        .collect();

    let mut remaining = budget;
    let mut rendered = Vec::new();
    let mut cursor: Option<ReadCursor> = None;
    let mut rendered_all = true;
    for message in &messages {
        if remaining == 0 {
            cursor = Some(ReadCursor {
                from_sequence: message.sequence,
                start_byte: 0,
            });
            rendered_all = false;
            break;
        }
        let from = floor_boundary(&message.content, offset);
        offset = 0;
        let (text, clipped) = clip_to_bytes(&message.content[from..], remaining);
        let end = from + text.len();
        remaining = remaining.saturating_sub(text.len());
        memo.bytes_delivered = memo.bytes_delivered.saturating_add(text.len());
        rendered.push(serde_json::json!({
            "message_id": message.id,
            "sequence": message.sequence,
            "role": message.role.as_str(),
            "text": text,
            "start_byte": from,
            "end_byte": end,
            "is_whole_message": from == 0 && !clipped,
        }));
        if clipped {
            cursor = Some(ReadCursor {
                from_sequence: message.sequence,
                start_byte: end as u64,
            });
            rendered_all = false;
            break;
        }
    }
    if cursor.is_none() && port_capped && rendered_all {
        cursor = messages.last().map(|last| ReadCursor {
            from_sequence: last.sequence.saturating_add(1),
            start_byte: 0,
        });
    }

    let progress = memo.reads.entry(key).or_default();
    if progress.message_ids.is_empty() {
        progress.first_start = requested_start;
        progress.message_ids = delivered_ids;
        progress.sequences = delivered_sequences;
    }
    progress.next = cursor;
    progress.exhausted = cursor.is_none();

    Ok(FunctionResult::success(serde_json::json!({
        "conversation_scope": "current",
        "messages": rendered,
        "truncated": cursor.is_some(),
        "continuation": cursor.map(|cursor| serde_json::json!({
            "from_sequence": cursor.from_sequence,
            "start_byte": cursor.start_byte,
        })),
        "budget_bytes": budget,
        "not_readable_message_ids": not_readable,
        "omitted_failed_outputs": omitted_failed_outputs,
        "note": "Exact source text from this conversation. Ids listed as not readable are \
                 either outside this conversation or did not fit this response.",
    })))
}

/// The memo key for one read request. A function so the lookup before the read
/// and the update after it cannot drift apart.
fn read_key(by_id: bool, wanted_ids: &[String], args: &ReadArgs) -> String {
    if by_id {
        let mut sorted = wanted_ids.to_vec();
        sorted.sort();
        format!("ids:{}", sorted.join(","))
    } else {
        format!(
            "range:{}-{}",
            args.from_sequence.unwrap_or_default(),
            args.to_sequence
                .map(|to| to.to_string())
                .unwrap_or_else(|| "end".to_string())
        )
    }
}
