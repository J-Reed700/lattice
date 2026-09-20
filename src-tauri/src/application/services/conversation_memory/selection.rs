//! Choosing what a run compacts, and what it shows the model alongside it.
//!
//! Design: `docs/design/2026-09-19-conversation-memory.md` §7.2, §7.3 step 5.
//!
//! Pure functions over one window of source messages. They decide where the
//! compacted prefix ends and which neighbouring passages travel with a batch —
//! two questions with no I/O in them, and both easier to test alone than
//! through a whole run.

use crate::domain::conversation_memory::{SourceMessage, SourceRole};

use super::prompts::ContextPassage;
use super::segment::{clip_chars, SourceSegment};
use super::{MAX_CONTEXT_PASSAGE_CHARS, MIN_RETAINED_TURNS};

/// How many messages of `window` belong in the compacted prefix.
///
/// Returns `None` when everything read is inside the retained tail, which is the
/// normal answer for a short conversation: the ledger stays empty until a
/// compaction is actually needed (§7.1).
///
/// The rules, all from §7.2:
///
/// * The cut lands on a turn boundary, so a user request is never separated from
///   the answer to it.
/// * At least [`MIN_RETAINED_TURNS`] complete turns stay raw, plus the current
///   user message when it has no answer yet — that message is never summarised
///   during its own turn.
/// * `keep_recent` is a minimum, not a target: the cut moves earlier to reach a
///   turn boundary, never later.
///
/// When the work cap stopped the window short of the transcript, the retention
/// rule is tried first anyway — the cap may have stopped only a message or two
/// from the end, and the tail it protects is about the prompt, not about this
/// window. Only if that leaves nothing to do does the cut fall back to the last
/// turn boundary in the window. Without that fallback a work cap smaller than
/// the retained tail would never advance the watermark at all.
pub(super) fn select_boundary(
    window: &[SourceMessage],
    reached_end: bool,
    keep_recent: usize,
) -> Option<usize> {
    let turn_starts: Vec<usize> = window
        .iter()
        .enumerate()
        .filter(|(_, message)| message.role == SourceRole::User)
        .map(|(index, _)| index)
        .collect();

    // A trailing user message has no answer yet, so it is the current turn and
    // one more turn has to be retained on top of the minimum.
    let mut retained_turns = MIN_RETAINED_TURNS;
    if window
        .last()
        .is_some_and(|message| message.role == SourceRole::User)
    {
        retained_turns += 1;
    }
    if let Some(cut) = retained_cut(window, &turn_starts, retained_turns, keep_recent) {
        return Some(cut);
    }
    if reached_end {
        return None;
    }
    match turn_starts.last() {
        Some(&start) if start > 0 => Some(start),
        _ => Some(window.len()),
    }
}

/// The latest turn boundary that still leaves the required tail raw.
fn retained_cut(
    window: &[SourceMessage],
    turn_starts: &[usize],
    retained_turns: usize,
    keep_recent: usize,
) -> Option<usize> {
    let ceiling = window.len().saturating_sub(keep_recent);
    let mut best = None;
    for (position, &start) in turn_starts.iter().enumerate() {
        // Index zero would leave an empty prefix: there is no earlier turn to
        // compact.
        if start == 0 {
            continue;
        }
        // `turn_starts` ascends, so once either condition fails it fails for
        // every later candidate too.
        if start > ceiling {
            break;
        }
        if turn_starts.len() - position < retained_turns {
            break;
        }
        best = Some(start);
    }
    best
}

/// Messages beside a batch that explain it: the one before its first message and
/// the one after its last, within this attempt's window.
///
/// Bounded to two on purpose. The case this serves is a short reply — "yes,
/// option B" — whose meaning lives in the message before it; more context than
/// that buys little and costs the batch's whole byte budget.
pub(super) fn adjacent_context<'a>(
    eligible: &'a [SourceMessage],
    batch: &[SourceSegment],
) -> Vec<&'a SourceMessage> {
    let mut out: Vec<&SourceMessage> = Vec::new();
    if let Some(first) = batch.first() {
        if let Some(position) = eligible
            .iter()
            .position(|message| message.id == first.message_id)
        {
            if let Some(previous) = position.checked_sub(1).and_then(|at| eligible.get(at)) {
                out.push(previous);
            }
        }
    }
    if let Some(last) = batch.last() {
        if let Some(position) = eligible
            .iter()
            .rposition(|message| message.id == last.message_id)
        {
            if let Some(next) = eligible.get(position + 1) {
                if !out.iter().any(|existing| existing.id == next.id) {
                    out.push(next);
                }
            }
        }
    }
    out
}

/// Every message the batch may quote: its own, plus its context.
///
/// A segment of a split message carries the whole message here, because §7.4
/// validates a quote crossing a segment boundary against the complete original.
pub(super) fn quotable_messages(
    eligible: &[SourceMessage],
    batch: &[SourceSegment],
    context: &[&SourceMessage],
) -> Vec<SourceMessage> {
    let mut out: Vec<SourceMessage> = Vec::new();
    for segment in batch {
        if out.iter().any(|message| message.id == segment.message_id) {
            continue;
        }
        if let Some(message) = eligible
            .iter()
            .find(|message| message.id == segment.message_id)
        {
            out.push(message.clone());
        }
    }
    for message in context {
        if !out.iter().any(|existing| existing.id == message.id) {
            out.push((*message).clone());
        }
    }
    out
}

/// Render context messages for the prompt, clipped and flagged as clipped.
pub(super) fn render_context<'a>(context: &[&'a SourceMessage]) -> Vec<ContextPassage<'a>> {
    context
        .iter()
        .map(|message| {
            let (text, truncated) = clip_chars(&message.content, MAX_CONTEXT_PASSAGE_CHARS);
            ContextPassage {
                message_id: message.id.as_str(),
                sequence: message.sequence,
                role: message.role.as_str(),
                text,
                truncated,
            }
        })
        .collect()
}
