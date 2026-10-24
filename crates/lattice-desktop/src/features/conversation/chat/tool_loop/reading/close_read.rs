//! The close read: the passages a reading marked with a hyphen, shown to the
//! model again right before it writes their lines.
//!
//! A long reading decides each passage in a token or two, thousands of
//! tokens after the passage itself, and the local 9B marked passages that
//! bore on the question with a hyphen, the later in the list the likelier.
//! Asked again by label alone it wrote the same hyphens. Shown the passages
//! again it copied what they say: in the offline A/B each of the 15 long
//! readings that had missed facts then held all six, and the answers written
//! from them passed 27 of 28, against 11 of 30 without.
//!
//! The close read runs on the reading's own prompt: the notes as the model's
//! own message, then the marked passages, as many as the window has room
//! for, in label order. Those that do not fit go in further requests, each in
//! place of the one before, so the server reads only the new passages. A line
//! counts only when it copies its own passage. Each marked passage is shown
//! again once.
//!
//! The instruction comes after the passages and says again what the reading
//! asked for: the sentences that bear on the question, or a hyphen. Told only
//! to write "in the same form", the 9B took its form from the first passage
//! and copied whole passages that said nothing on the question. In the second
//! offline A/B (evals/retrieval/2026-10-24-CLOSE-READ.md) this wording kept
//! every fact the first one copied, 281 of 281, with 31 lines that held none
//! against 50 for the same sentences said before the passages.
//!
//! The window's room, the turn's clock and Stop bound it; nothing is counted.

use super::super::clock::TurnClock;
use super::super::final_round::say_last;
use super::notes::{as_kept, locate, parse_notes};
use super::{
    assistant, reading_request, room_left, tokens, whole_lines, Notes, Passages, PARAGRAPH_BREAK,
};
use crate::features::conversation::chat::cancellation::cancelled_error;
use crate::features::conversation::chat::turn_record::{TurnRecorder, TurnStepKind};
use lattice_core::application::ports::llm_port::{CompletionInput, CompletionRequest};
use lattice_core::application::ports::LLMPort;
use lattice_core::shared::error::Result;
use std::collections::BTreeMap;
use tokio::time::timeout;
use tokio_util::sync::CancellationToken;
use tracing::warn;

/// Said after the notes, with the passages they mark with a hyphen shown
/// again. `{passages}` is each of them as `[label] text`, a blank line apart.
pub(super) const CLOSE_READ_INSTRUCTION: &str = "Here again, each on its own, are the passages you marked with a hyphen.\n\n{passages}\n\nWrite one line for each of these passages, as before: its label, then the sentences of that passage that bear on my question, copied exactly as written and in the passage's own language — or its label and a single hyphen when it says nothing bearing on my question, however near its subject. Write only these lines.";

/// Show the model the passages its notes mark with a hyphen again, once, and
/// take each line it writes that copies its own passage in place of the
/// hyphen. Returns how many hyphens became notes.
///
/// It only ever adds to the notes: a request the provider fails ends it with
/// the notes as they are. Only Stop and the turn's deadline are errors, as
/// in the reading round.
pub(super) async fn close_read(
    llm: &dyn LLMPort,
    request: &CompletionRequest,
    passages: &Passages,
    stop: &[String],
    notes: &mut Notes,
    recorder: &TurnRecorder,
    cancel: &CancellationToken,
    clock: &TurnClock,
) -> Result<usize> {
    let marked = hyphens(notes);
    if marked.is_empty() {
        return Ok(0);
    }
    let room = room_for_passages(llm, request, &notes.text).await;
    let parts = pack(&measured(llm, passages, &marked).await, room);
    if parts.is_empty() {
        return Ok(0);
    }
    let count = parts.iter().map(Vec::len).sum::<usize>();
    let step = recorder.begin_guarded(
        TurnStepKind::Generate,
        "Reading the marked passages again",
        Some(match count {
            1 => "1 passage".to_string(),
            count => format!("{count} passages"),
        }),
    );
    // Every part is read beside the notes as the reading left them, so each
    // request replaces the one before it.
    let mut taken: BTreeMap<u32, String> = BTreeMap::new();
    let mut failed = false;
    for part in &parts {
        let Some(written) = ask(
            llm,
            request,
            passages,
            stop,
            &notes.text,
            part,
            cancel,
            clock,
        )
        .await?
        else {
            failed = true;
            break;
        };
        taken.extend(copies(&written, part, passages));
    }
    let added = taken.len();
    for (label, note) in taken {
        notes.lines.insert(label, Some(note));
        notes.noted.insert(label);
    }
    if added > 0 {
        notes.text = as_kept(&notes.lines);
        notes.tokens = tokens(llm, &notes.text).await;
        step.set_reasoning(Some(&notes.text));
    }
    let summary = match added {
        0 => "nothing added".to_string(),
        1 => "1 more passage bears on the question".to_string(),
        added => format!("{added} more passages bear on the question"),
    };
    if failed {
        step.failed(Some(summary));
    } else {
        step.done(Some(summary));
    }
    Ok(added)
}

/// The labels the notes mark with a hyphen, in label order.
fn hyphens(notes: &Notes) -> Vec<u32> {
    notes
        .lines
        .iter()
        .filter(|(_, note)| note.is_none())
        .map(|(label, _)| *label)
        .collect()
}

/// The room a close read has for passages: what the window has left once the
/// reading's prompt, the notes and the instruction are in it, measured as
/// sent, less the reading's output reservation.
async fn room_for_passages(llm: &dyn LLMPort, request: &CompletionRequest, said: &str) -> usize {
    let bare = reading_request(
        request,
        input(request, said, ""),
        request.max_output_tokens,
        &[],
        None,
    );
    let reserved = request.max_output_tokens.map_or(0, |reserved| {
        usize::try_from(reserved).unwrap_or(usize::MAX)
    });
    room_left(llm, &bare).await.saturating_sub(reserved)
}

/// Each marked passage with its size as it would be shown again, with the
/// blank line after it, in label order.
async fn measured(llm: &dyn LLMPort, passages: &Passages, marked: &[u32]) -> Vec<(u32, usize)> {
    let mut measured = Vec::with_capacity(marked.len());
    for &label in marked {
        let Some(text) = passages.raw(label) else {
            continue;
        };
        let size = tokens(llm, &format!("{}{PARAGRAPH_BREAK}", shown(label, text))).await;
        measured.push((label, size));
    }
    measured
}

/// The passages, in label order, split into requests that each fit `room`:
/// one request is filled before the next is begun. A passage larger than the
/// room on its own is not shown again.
pub(super) fn pack(sizes: &[(u32, usize)], room: usize) -> Vec<Vec<u32>> {
    let mut parts: Vec<Vec<u32>> = Vec::new();
    let mut used = 0;
    for &(label, size) in sizes.iter().filter(|(_, size)| *size <= room) {
        match parts.last_mut() {
            Some(part) if used + size <= room => part.push(label),
            _ => {
                parts.push(vec![label]);
                used = 0;
            }
        }
        used += size;
    }
    parts
}

/// One close-read request: the reading's prompt, the notes as the model's
/// own message, then the passages of `part` shown again, sent as the reading
/// was. The whole lines the model wrote, or `None` when the provider failed.
async fn ask(
    llm: &dyn LLMPort,
    request: &CompletionRequest,
    passages: &Passages,
    stop: &[String],
    said: &str,
    part: &[u32],
    cancel: &CancellationToken,
    clock: &TurnClock,
) -> Result<Option<String>> {
    let again = part
        .iter()
        .filter_map(|&label| Some(shown(label, passages.raw(label)?)))
        .collect::<Vec<_>>()
        .join(PARAGRAPH_BREAK);
    let remaining = clock.remaining()?;
    let asking = reading_request(
        request,
        input(request, said, &again),
        request.max_output_tokens,
        stop,
        Some(remaining),
    );
    let completion = timeout(remaining, llm.complete(&asking));
    let response = tokio::select! {
        biased;
        _ = cancel.cancelled() => return Err(cancelled_error()),
        result = completion => result.map_err(|_| clock.exhausted())?,
    };
    match response {
        Ok(response) => Ok(Some(
            whole_lines(&response.text, &response.finish_reason).to_string(),
        )),
        Err(_) if cancel.is_cancelled() => Err(cancelled_error()),
        Err(error) => {
            warn!(%error, "The model could not read the marked passages again; keeping the notes as they are");
            Ok(None)
        }
    }
}

/// The reading's prompt, the notes after it as the model's own message, and
/// the instruction with `again` shown in it, said last.
fn input(request: &CompletionRequest, said: &str, again: &str) -> Vec<CompletionInput> {
    let mut input = request.input.clone();
    input.push(assistant(said));
    say_last(
        &mut input,
        &CLOSE_READ_INSTRUCTION.replace("{passages}", again),
    );
    input
}

/// A passage as it is shown again.
fn shown(label: u32, text: &str) -> String {
    format!("[{label}] {text}")
}

/// The lines of `written` for labels in `part` that copy their own passage.
/// A label written twice is read by its last line, as the reading reads it.
fn copies(written: &str, part: &[u32], passages: &Passages) -> BTreeMap<u32, String> {
    parse_notes(written)
        .into_iter()
        .collect::<BTreeMap<_, _>>()
        .into_iter()
        .filter_map(|(label, note)| {
            let note = note?;
            (part.contains(&label) && locate(&note, passages).contains(&label))
                .then_some((label, note))
        })
        .collect()
}
