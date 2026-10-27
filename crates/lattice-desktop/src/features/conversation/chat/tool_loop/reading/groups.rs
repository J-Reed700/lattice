//! The reading, a few passages at a time.
//!
//! Read in one pass, a long reading decides each passage in a token or two,
//! thousands of tokens after the passage itself, and the local 9B marked
//! passages that bore on the question with a hyphen, the later in the list
//! the likelier. Shown again a few at a time, the passages were read: every
//! fact the close read once recovered stood first or second in the group it
//! was shown in (evals/retrieval/2026-10-24-CLOSE-READ.md). So every passage
//! is read that way from the start, and nothing is read twice to repair a
//! first pass: the turn's message closes with a group of its passages shown
//! again, in place of the reading instruction, and the model writes their
//! lines. Shown one at a time the 9B marked fact passages with a hyphen more
//! often than in a group: it judges a passage better beside a few others.
//! evals/retrieval/2026-10-26-GROUPED-READING.md has the A/B that chose the
//! group's size.
//!
//! Each group's request is the turn's prompt up to its closing instruction,
//! so a server that keeps the prompt reads only the group, and a provider
//! that caches it bills the prompt once. The groups go one after another: on
//! a llama.cpp server that keeps the conversation's slot, and on a provider
//! each request reads the prompt the one before it cached.
//!
//! The notes are written out of the answer's output reservation, all the
//! groups and their sweeps together, and each group's out of what the window
//! has left beside it. A group the window cannot hold beside the prompt is
//! split in two; a passage it cannot hold shown again is read by its label
//! on the prompt as planned, which leaves the reservation by construction,
//! so every passage is read. The turn's clock and Stop bound it.

use super::super::clock::TurnClock;
use super::super::final_round::say_last;
use super::notes::parse_notes;
use super::{
    assistant, reading_request, replace_reading, room_left, sweep_instruction, tokens, whole_lines,
    Passages, PARAGRAPH_BREAK,
};
use crate::features::conversation::chat::cancellation::cancelled_error;
use lattice_core::application::ports::llm_port::{CompletionInput, CompletionRequest};
use lattice_core::application::ports::LLMPort;
use lattice_core::shared::error::Result;
use std::collections::{BTreeMap, VecDeque};
use tokio::time::timeout;
use tokio_util::sync::CancellationToken;
use tracing::warn;

/// Closes the turn's message for one group, in place of the reading
/// instruction. `{passages}` is each passage of the group as `[label] text`,
/// a blank line apart. The criterion and its place after the passages are
/// the close read's, measured there: said before them, or as "the same
/// form", the 9B copied whole passages that said nothing on the question.
pub(super) const GROUP_INSTRUCTION: &str = "Before answering, read these passages from above again, each on its own.\n\n{passages}\n\nWrite one line for each of these passages: its label, then the sentences of that passage that bear on my question, copied exactly as written and in the passage's own language — or its label and a single hyphen when it says nothing bearing on my question, however near its subject. Write only these lines.";

/// Closes the turn's message for a passage the window cannot hold shown
/// again: it is read where it stands. `{labels}` lists the labels.
pub(super) const LABELS_INSTRUCTION: &str = "Before answering, write one line for each of the passages {labels} above: its label, then the sentences of that passage that bear on my question, copied exactly as written and in the passage's own language — or its label and a single hyphen when it says nothing bearing on my question, however near its subject. Write only these lines.";

/// Passages read together, in label order: the size the A/B chose.
pub(super) const PASSAGES_PER_GROUP: usize = 4;

/// A group to read, and the prompt it is read on.
pub(super) struct Reading {
    pub(super) labels: Vec<u32>,
    pub(super) input: Vec<CompletionInput>,
}

/// The groups in label order, each with the prompt that shows it again when
/// the window holds that and a copy of its passages beside it, the most its
/// lines can be: a group it cannot hold is split in two, and a passage it
/// cannot hold is read by its label.
pub(super) async fn planned(
    llm: &dyn LLMPort,
    request: &CompletionRequest,
    passages: &Passages,
) -> Vec<Reading> {
    let labels: Vec<u32> = passages.labels().collect();
    let mut pending: VecDeque<Vec<u32>> = labels
        .chunks(PASSAGES_PER_GROUP)
        .map(<[u32]>::to_vec)
        .collect();
    let mut readings = Vec::new();
    while let Some(group) = pending.pop_front() {
        let again = shown(passages, &group);
        let input = group_input(request, &again);
        let room = room_left(
            llm,
            &reading_request(request, input.clone(), None, &[], None),
        )
        .await;
        let fits = room >= tokens(llm, &again).await;
        if fits {
            readings.push(Reading {
                labels: group,
                input,
            });
        } else if group.len() > 1 {
            let (first, second) = group.split_at(group.len() / 2);
            pending.push_front(second.to_vec());
            pending.push_front(first.to_vec());
        } else {
            let input = labels_input(request, &group);
            readings.push(Reading {
                labels: group,
                input,
            });
        }
    }
    readings
}

/// Each label's line, `None` for a hyphen, from every group read, in label
/// order, out of what the reservation has left after the groups before. A
/// group whose request the provider failed adds nothing, and the rest are
/// still read; only Stop and the turn's deadline are errors.
pub(super) async fn read_in_groups(
    llm: &dyn LLMPort,
    request: &CompletionRequest,
    passages: &Passages,
    cancel: &CancellationToken,
    clock: &TurnClock,
) -> Result<BTreeMap<u32, Option<String>>> {
    let reserved = request.max_output_tokens;
    let mut lines = BTreeMap::new();
    let mut written = 0usize;
    for group in planned(llm, request, passages).await {
        let left = reserved.map(|reserved| reserved.saturating_sub(as_u32(written)));
        if left == Some(0) {
            break;
        }
        lines.extend(read_group(llm, request, &group, left, cancel, clock).await?);
        written = tokens(llm, &super::notes::as_kept(&lines)).await;
    }
    Ok(lines)
}

/// One group read on its prompt, then a sweep for the labels its lines left
/// out, asked for again while each sweep accounts for at least one of them,
/// within `ceiling` and what the window has left beside each request.
async fn read_group(
    llm: &dyn LLMPort,
    request: &CompletionRequest,
    reading: &Reading,
    ceiling: Option<u32>,
    cancel: &CancellationToken,
    clock: &TurnClock,
) -> Result<BTreeMap<u32, Option<String>>> {
    let (group, input) = (reading.labels.as_slice(), &reading.input);
    // Nothing past the group's last label is one of its lines.
    let stop = group
        .last()
        .map(|last| super::past(*last))
        .unwrap_or_default();
    let mut lines = BTreeMap::new();
    let mut text = String::new();
    let mut written = 0usize;
    loop {
        let missing: Vec<u32> = group
            .iter()
            .copied()
            .filter(|label| !lines.contains_key(label))
            .collect();
        if missing.is_empty() {
            break;
        }
        let mut asking = input.clone();
        if !text.is_empty() {
            asking.push(assistant(&text));
            say_last(&mut asking, &sweep_instruction(&missing));
        }
        let room = as_u32(
            room_left(
                llm,
                &reading_request(request, asking.clone(), None, &[], None),
            )
            .await,
        );
        let left = ceiling.map_or(room, |ceiling| {
            ceiling.saturating_sub(as_u32(written)).min(room)
        });
        if left == 0 {
            break;
        }
        let remaining = clock.remaining()?;
        let reading = reading_request(request, asking, Some(left), &stop, Some(remaining));
        let completion = timeout(remaining, llm.complete(&reading));
        let response = tokio::select! {
            biased;
            _ = cancel.cancelled() => return Err(cancelled_error()),
            result = completion => result.map_err(|_| clock.exhausted())?,
        };
        let response = match response {
            Ok(response) => response,
            Err(_) if cancel.is_cancelled() => return Err(cancelled_error()),
            Err(error) => {
                warn!(%error, ?group, "The model could not read a group of passages; keeping what it had read");
                break;
            }
        };
        let whole = whole_lines(&response.text, &response.finish_reason);
        // Only the labels asked for: a sweep cannot overwrite a line, and a
        // line under another group's label is not this group's reading.
        let mut accounted = false;
        for (label, note) in parse_notes(whole) {
            if missing.contains(&label) {
                accounted = true;
                lines.insert(label, note);
            }
        }
        // A reading that accounts for no label it was asked for has nothing
        // to add, and the next would read the same way: the sweep ends.
        if !accounted {
            break;
        }
        if !text.is_empty() {
            text.push('\n');
        }
        text.push_str(whole.trim());
        written = tokens(llm, &text).await;
    }
    Ok(lines)
}

/// The group's passages as they are shown again: `[label] text`, a blank
/// line apart.
pub(super) fn shown(passages: &Passages, group: &[u32]) -> String {
    group
        .iter()
        .filter_map(|&label| Some(format!("[{label}] {}", passages.raw(label)?)))
        .collect::<Vec<_>>()
        .join(PARAGRAPH_BREAK)
}

/// The turn's prompt with `again` shown in place of the reading instruction.
pub(super) fn group_input(request: &CompletionRequest, again: &str) -> Vec<CompletionInput> {
    let mut input = request.input.clone();
    replace_reading(&mut input, &GROUP_INSTRUCTION.replace("{passages}", again));
    input
}

/// The turn's prompt with the reading instruction narrowed to `group`, its
/// passages read where they stand.
fn labels_input(request: &CompletionRequest, group: &[u32]) -> Vec<CompletionInput> {
    let mut input = request.input.clone();
    replace_reading(
        &mut input,
        &LABELS_INSTRUCTION.replace("{labels}", &super::label_list(group)),
    );
    input
}

fn as_u32(value: usize) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}
