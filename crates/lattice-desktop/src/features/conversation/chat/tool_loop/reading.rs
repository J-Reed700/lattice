//! Reading every passage before writing the answer, then checking the answer
//! used what the reading found.
//!
//! Asked for everything a document records about a subject, the local 9B
//! skimmed: one bullet per passage it happened to notice, then it stopped,
//! leaving out one to three of the passages that held the facts — the second
//! passage as often as the twentieth. Which ones changed with the seed; that
//! some were dropped did not. It is the multi-needle shortfall long-context
//! benchmarks measure, and neither a shorter prompt nor thinking cured it.
//!
//! So a grounded answer is written from notes on one cached prompt. The
//! model first reads (Chain-of-Note, with the notes copied rather than
//! paraphrased), a few passages at a time, each group shown again where the
//! message closes: one line per passage, the sentences of it that bear on
//! the question as written, or a hyphen (see `reading::groups`). It then
//! writes the answer from its own notes. Last, the app compares the notes and
//! the answer: a passage the notes copy word for word that the answer never
//! cites was skipped, and the model is asked to add it. Grounding asks
//! whether every cited claim is in a passage; this asks whether every passage
//! the model found is in the answer.
//!
//! Nothing here looks at the question or depends on its language. The inputs
//! are the labels the message shows, the model's notes and the labels in its
//! answer. Every loop ends when a finite set of labels stops shrinking. The
//! notes are written out of the answer's output reservation, all the groups
//! and their sweeps together; the answer is held to what the window has left
//! after the notes, and goes without them when they would take more of it
//! than they leave. The turn's clock and Stop bound all of it.
//!
//! `scripts/replay_answer_round.py` sends these same instructions to measure
//! the rounds, and `scripts/replay_grouped_reading_ab.py` measured the
//! grouped reading against the single reading and its close read;
//! docs/design/2026-10-23-answer-coverage.md and
//! evals/retrieval/2026-10-26-GROUPED-READING.md have the numbers.

use super::clock::TurnClock;
use super::final_round::{say_last, written_call_start};
use super::{answer_was_cut_short, tool_schema_text, StreamEmitter, CUT_SHORT_NOTE};
use crate::features::conversation::chat::cancellation::cancelled_error;
use crate::features::conversation::chat::turn_record::{TurnRecorder, TurnStepKind};
use crate::features::conversation::chat::verification::cited_labels;
use lattice_core::application::contracts::sources::SourceDto;
use lattice_core::application::ports::llm_port::{
    CompletionInput, CompletionRequest, ContentPart, SamplingOverride, ThinkingMode, ToolChoice,
    TEMPLATE_TOKENS_FOR_ANSWER, TEMPLATE_TOKENS_PER_MESSAGE,
};
use lattice_core::application::ports::LLMPort;
use lattice_core::shared::error::Result;
use lattice_core::shared::text::normalize_whitespace;
use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet};
use std::time::Duration;
use tokio::time::timeout;
use tokio_util::sync::CancellationToken;
use tracing::{debug, info, warn};

mod groups;
mod notes;
use groups::read_in_groups;
use notes::{as_kept, attribute};

/// Closes a message that shows numbered passages. It is part of the message,
/// so the round that reads and the round that writes share the prompt to its
/// last token, and the turn's budget prices it with the rest.
pub(in crate::features::conversation::chat) const READING_INSTRUCTION: &str = "Before answering, go through every numbered passage above in order and write one line for each: its label, then the sentences of that passage that bear on my question, copied exactly as written and in the passage's own language. For a passage that says nothing bearing on my question, write its label and a single hyphen. Account for every label. Write only these lines; the answer comes after.";

/// Said after a group's lines that left labels out. `{labels}` lists them.
pub(super) const SWEEP_INSTRUCTION: &str =
    "You did not account for {labels}. Write one line for each of them, in the same form.";

/// Said after the notes, last, to turn the reading into the answer.
pub(super) const WRITE_INSTRUCTION: &str = "Now answer my question directly, in the passages' own words, keeping their dates, figures and names as given. Cover every passage your notes quote and cite each with its label. Do not describe the notes or the passages, and say nothing about passages that do not bear on my question.";

/// Said after an answer that left out passages its notes quote. `{quotes}`
/// gives each one's label and note.
pub(super) const CONTINUE_INSTRUCTION: &str = "Your answer does not use these passages, which your notes quote as bearing on my question: {quotes}. Continue your answer with what each of them says, in the same form as above, citing each with its label; do not repeat what you have written. If one of those notes was mistaken, write nothing for it.";

/// Between the answer and what a continuation adds to it.
const PARAGRAPH_BREAK: &str = "\n\n";

/// A numbered passage the message shows: its trimmed text for showing it
/// again in its group, and its whitespace-normalized text for verifying
/// copies.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct Passage {
    pub(super) raw: String,
    pub(super) normalized: String,
}

impl Passage {
    pub(super) fn new(text: impl Into<String>) -> Self {
        let text = text.into();
        Self {
            raw: text.trim().to_string(),
            normalized: normalize_whitespace(&text),
        }
    }
}

impl<T: Into<String>> From<T> for Passage {
    fn from(text: T) -> Self {
        Self::new(text)
    }
}

/// The numbered passages the message shows, by label, each as the text a note
/// has to be copied from to count.
#[derive(Debug, Default)]
pub(super) struct Passages(pub(super) BTreeMap<u32, Passage>);

impl Passages {
    /// The sources whose label the message shows. A source the message does
    /// not number is not one the model was asked to read. Several sources
    /// under one label — an attachment carried in chunks — read as one text.
    fn shown(sources: &[SourceDto], message: &str) -> Self {
        let mut texts: BTreeMap<u32, String> = BTreeMap::new();
        for source in sources {
            let Some(label) = source.citation_id else {
                continue;
            };
            if message.contains(&format!("[{label}]")) {
                let text = texts.entry(label).or_default();
                text.push(' ');
                text.push_str(&source.content);
            }
        }
        Self(
            texts
                .into_iter()
                .map(|(label, text)| (label, Passage::from(text)))
                .collect(),
        )
    }

    pub(super) fn labels(&self) -> impl Iterator<Item = u32> + '_ {
        self.0.keys().copied()
    }

    pub(super) fn raw(&self, label: u32) -> Option<&str> {
        self.0.get(&label).map(|p| p.raw.as_str())
    }

    pub(super) fn len(&self) -> usize {
        self.0.len()
    }

    pub(super) fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// What the reading round wrote.
#[derive(Debug)]
pub(super) struct Notes {
    /// The note lines and nothing else, as kept (see [`notes::as_kept`]):
    /// replayed to the model as its own message, and shown on the reading
    /// step.
    text: String,
    /// Each label's note when it copies that passage, `None` otherwise: a
    /// hyphen, or a line that copies no passage or another one (a copy of
    /// another passage is that passage's note, see [`notes::attribute`]).
    lines: BTreeMap<u32, Option<String>>,
    /// The labels whose note is copied word for word from that passage. Only
    /// these can find an answer incomplete: a note that is a paraphrase, a
    /// "none", or a quote found in several passages proves nothing.
    noted: BTreeSet<u32>,
    /// `text` in tokens, as the backend counts them where it can.
    tokens: usize,
}

/// The passages to read, when the turn's message asks for a reading: it
/// closes with [`READING_INSTRUCTION`], which the prompt builder adds only to
/// a message that shows numbered passages. `None` too when none of the labels
/// it shows is a source the turn holds: there is nothing to read, and the
/// instruction is taken back (see [`withdraw`]).
pub(super) fn asked(input: &[CompletionInput], sources: &[SourceDto]) -> Option<Passages> {
    let message = input.iter().rev().find(|item| item.is_user())?.text();
    message
        .contains(READING_INSTRUCTION)
        .then(|| Passages::shown(sources, &message))
        .filter(|passages| !passages.is_empty())
}

/// The reading: every passage read in its group (see `reading::groups`),
/// each note then put under the passage it was copied from, and the notes
/// replayed as the lines as kept.
///
/// `Ok(None)` when no notes came of it — the provider failed, the model wrote
/// no line under a label, or no note is a copy of its passage — and the
/// answer is then asked for as it was before reading existed (see
/// [`withdraw`]). Only Stop and the turn's deadline are errors.
pub(super) async fn read_passages(
    llm: &dyn LLMPort,
    request: &CompletionRequest,
    passages: &Passages,
    recorder: &TurnRecorder,
    cancel: &CancellationToken,
    clock: &TurnClock,
) -> Result<Option<Notes>> {
    let step = recorder.begin_guarded(
        TurnStepKind::Generate,
        "Reading the passages",
        Some(format!("{} passages", passages.len())),
    );
    let mut lines = read_in_groups(llm, request, passages, cancel, clock).await?;
    if lines.is_empty() {
        warn!("The reading wrote no notes; answering without them");
        step.failed(Some("No notes".into()));
        return Ok(None);
    }
    let (noted, moved) = attribute(&mut lines, passages);
    // Only a note that copies its own passage reaches the writer. A line
    // copying no passage, or another passage's, would put under its label
    // words that passage does not say; replayed as a hyphen it costs nothing,
    // since the passage itself is still in the prompt. Replayed this way the
    // answers kept every fact the full notes gave them (149 of 152 facts on
    // 30 questions, evals/retrieval/2026-10-26-GROUPED-READING.md).
    for (label, note) in &mut lines {
        if !noted.contains(label) {
            *note = None;
        }
    }
    let quoted = lines.values().filter(|note| note.is_some()).count();
    if noted.is_empty() {
        info!(
            passages = passages.len(),
            accounted = lines.len(),
            quoted,
            "The reading copied no passage word for word; answering without it"
        );
        step.failed(Some("No passage copied".into()));
        return Ok(None);
    }
    let text = as_kept(&lines);
    let written = tokens(llm, &text).await;
    step.set_reasoning(Some(&text));
    step.done(Some(format!(
        "{quoted} of {} passages bear on the question",
        passages.len()
    )));
    info!(
        passages = passages.len(),
        accounted = lines.len(),
        quoted,
        verbatim = noted.len(),
        moved = ?moved,
        tokens = written,
        "Read the passages before answering"
    );
    Ok(Some(Notes {
        text,
        lines,
        noted,
        tokens: written,
    }))
}

/// Give the writing round the notes, as the model's own message, and the
/// instruction to write, said last. Everything before them is the reading
/// round's request unchanged, so the server reads only these.
///
/// The prompt was planned to leave the answer its reservation, and the notes
/// and the instruction now take part of it. So the writing round is measured
/// as it will be sent, and its answer held to the room left in the window the
/// turn was planned against, never more than the reservation. When that room
/// is smaller than the notes themselves, the notes would cost the answer more
/// than they give it: they are not given, the request is left as it was, and
/// `false` says so.
pub(super) async fn append_notes(
    llm: &dyn LLMPort,
    request: &mut CompletionRequest,
    notes: &Notes,
) -> bool {
    let planned = request.input.len();
    request.input.push(assistant(&notes.text));
    say_last(&mut request.input, WRITE_INSTRUCTION);
    let room = room_left(llm, request).await;
    if room < notes.tokens {
        info!(
            room,
            notes = notes.tokens,
            "The notes would leave the answer less room than they take; answering without them"
        );
        request.input.truncate(planned);
        return false;
    }
    request.max_output_tokens = Some(within(request.max_output_tokens, room));
    true
}

/// Whether a research turn's saved rounds hold its reading: the notes, and
/// the instruction to write from them said after.
pub(super) fn noted_in(transcript: &[CompletionInput]) -> bool {
    transcript
        .iter()
        .any(|item| item.text().ends_with(WRITE_INSTRUCTION))
}

/// Take the reading instruction back out of the turn's message when no notes
/// came of it, so the answer is asked for exactly as before reading existed.
pub(super) fn withdraw(input: &mut [CompletionInput]) {
    replace_reading(input, "");
}

/// Close the turn's message with `instead` where the reading instruction
/// stood, or with nothing after its passages and question when it is empty.
fn replace_reading(input: &mut [CompletionInput], instead: &str) {
    let said = format!("\n\n{READING_INSTRUCTION}");
    let instead = if instead.is_empty() {
        String::new()
    } else {
        format!("\n\n{instead}")
    };
    let Some(item) = input.iter_mut().rev().find(|item| item.is_user()) else {
        return;
    };
    match item {
        CompletionInput::Message { content, .. } => *content = content.replacen(&said, &instead, 1),
        CompletionInput::Multimodal { parts, .. } => {
            for part in parts {
                if let ContentPart::Text { text } = part {
                    *text = text.replacen(&said, &instead, 1);
                }
            }
        }
        _ => {}
    }
}

/// Ask the model to add the passages its notes quote and its answer does not
/// cite, after the answer, for as long as each continuation cites at least
/// one of them. The set shrinks every time round, so it ends.
///
/// A continuation is written whole before any of it is shown, then shown and
/// kept as one piece, so what the reader saw and what is saved are the same
/// text, and a continuation that fails or runs out of time takes nothing from
/// an answer that was already finished: the answer stands as written. An
/// answer stopped at its output limit is left as it is too: it already says
/// why it ends there. Only Stop is an error.
pub(super) async fn add_what_was_left_out(
    llm: &dyn LLMPort,
    request: &mut CompletionRequest,
    notes: &Notes,
    answer: &mut String,
    emitter: &StreamEmitter<'_>,
    recorder: &TurnRecorder,
    cancel: &CancellationToken,
    clock: &TurnClock,
) -> Result<()> {
    if answer.ends_with(CUT_SHORT_NOTE) {
        return Ok(());
    }
    let mut left = left_out(&notes.noted, &cited_labels(answer));
    // The turn ends after this, so the request is changed in place: calls
    // forbidden and no thinking, as for any round that may only write.
    request.tool_choice = (!request.tools.is_empty()).then_some(ToolChoice::None);
    request.thinking = ThinkingMode::Off;
    while !left.is_empty() {
        let Ok(remaining) = clock.remaining() else {
            break;
        };
        say_last(&mut request.input, &continue_instruction(&left, notes));
        let room = room_left(llm, request).await;
        if room == 0 {
            info!(left_out = ?left, "No room left in the window to add what the answer left out; keeping the answer");
            break;
        }
        info!(left_out = ?left, "The answer left out passages its notes quote; asking for them");
        request.max_output_tokens = Some(within(request.max_output_tokens, room));
        request.time_budget = Some(remaining);
        let step = recorder.begin_guarded(TurnStepKind::Generate, "Adding what was left out", None);
        let completion = timeout(remaining, llm.complete(request));
        let response = tokio::select! {
            biased;
            _ = cancel.cancelled() => return Err(cancelled_error()),
            result = completion => result,
        };
        let mut response = match response {
            Ok(Ok(response)) => response,
            Ok(Err(_)) if cancel.is_cancelled() => return Err(cancelled_error()),
            Ok(Err(error)) => {
                warn!(%error, "Could not add what the answer left out; keeping the answer");
                step.failed(Some("nothing added".into()));
                break;
            }
            Err(_) => {
                warn!(
                    "The turn's time ran out adding what the answer left out; keeping the answer"
                );
                step.failed(Some("nothing added".into()));
                break;
            }
        };
        // A call written as text is cut out: this round may not call.
        let (written, called) = match written_call_start(&response.text) {
            Some(start) => (&response.text[..start], true),
            None => (response.text.as_str(), false),
        };
        let written = written.trim();
        let cut_short = match answer_was_cut_short(&response.finish_reason, written, false) {
            Ok(cut_short) => cut_short && !called,
            Err(error) => {
                warn!(%error, "Could not add what the answer left out; keeping the answer");
                step.failed(Some("nothing added".into()));
                break;
            }
        };
        let added = cited_labels(written).intersection(&left).count();
        step.done(Some(match added {
            0 => "nothing added".to_string(),
            1 => "1 passage added".to_string(),
            added => format!("{added} passages added"),
        }));
        if !written.is_empty() {
            let note = if cut_short { CUT_SHORT_NOTE } else { "" };
            let shown = format!("{PARAGRAPH_BREAK}{written}{note}");
            emitter.content(&shown)?;
            answer.push_str(&shown);
        }
        request.input.append(&mut response.replay);
        if added == 0 || answer.ends_with(CUT_SHORT_NOTE) {
            break;
        }
        left = left_out(&notes.noted, &cited_labels(answer));
    }
    Ok(())
}

/// The lines a reading round wrote that can be trusted whole. A reply cut at
/// the output limit loses its last line, which may stop mid-sentence; a sweep
/// asks for that label again. A tool call written as text is not a note.
pub(super) fn whole_lines<'t>(text: &'t str, finish_reason: &str) -> &'t str {
    let text = match written_call_start(text) {
        Some(start) => &text[..start],
        None => text,
    };
    if matches!(finish_reason, "length" | "max_tokens" | "incomplete") {
        text.rfind('\n').map_or("", |end| &text[..end])
    } else {
        text
    }
}

/// The passages the notes quote that the answer does not cite.
pub(super) fn left_out(noted: &BTreeSet<u32>, cited: &BTreeSet<u32>) -> BTreeSet<u32> {
    noted.difference(cited).copied().collect()
}

/// Where the lines of a reading that ends at `last` end: at the next label,
/// bracketed or numbering a line. Nothing written after it is one of them.
fn past(last: u32) -> Vec<String> {
    let next = last.saturating_add(1);
    vec![format!("[{next}]"), format!("\n{next}.")]
}

fn sweep_instruction(labels: &[u32]) -> String {
    SWEEP_INSTRUCTION.replace("{labels}", &label_list(labels))
}

/// `[1], [2], [3]`.
fn label_list(labels: &[u32]) -> String {
    labels
        .iter()
        .map(|label| format!("[{label}]"))
        .collect::<Vec<_>>()
        .join(", ")
}

fn continue_instruction(left: &BTreeSet<u32>, notes: &Notes) -> String {
    CONTINUE_INSTRUCTION.replace("{quotes}", &quotes(left, notes))
}

/// Each left-out label with its note, as a continuation is asked to restate.
fn quotes(left: &BTreeSet<u32>, notes: &Notes) -> String {
    left.iter()
        .filter_map(|label| {
            let note = notes.lines.get(label)?.as_deref()?;
            Some(format!("[{label}] \"{note}\""))
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// The output ceiling for a round with `room` left in the window: the
/// reservation, or the room when that is less.
fn within(reserved: Option<u32>, room: usize) -> u32 {
    let room = u32::try_from(room).unwrap_or(u32::MAX);
    reserved.map_or(room, |reserved| reserved.min(room))
}

/// What the window the turn was planned against has left once `request`'s
/// prompt is in it.
pub(super) async fn room_left(llm: &dyn LLMPort, request: &CompletionRequest) -> usize {
    llm.max_context_tokens()
        .saturating_sub(prompt_tokens(llm, request).await)
}

/// `request`'s prompt in tokens, as the send-time gates measure one: every
/// message with its role, every call and result and the tool schemas, plus
/// the chat template's framing and each picture's declared cost.
pub(super) async fn prompt_tokens(llm: &dyn LLMPort, request: &CompletionRequest) -> usize {
    let text = request
        .input
        .iter()
        .flat_map(|item| match item {
            CompletionInput::Native { value } => vec![Cow::Owned(value.to_string())],
            CompletionInput::Message { role, .. } | CompletionInput::Multimodal { role, .. } => {
                vec![Cow::Borrowed(role.as_str()), item.text()]
            }
            _ => vec![item.text()],
        })
        .chain((!request.tools.is_empty()).then(|| Cow::Owned(tool_schema_text(&request.tools))))
        .collect::<Vec<_>>()
        .join("\n");
    tokens(llm, &text).await
        + TEMPLATE_TOKENS_PER_MESSAGE * request.input.len()
        + TEMPLATE_TOKENS_FOR_ANSWER
        + request.image_tokens()
}

/// Tokens in `text` as the backend's own tokenizer counts them where it has
/// one, by its estimate otherwise or when the count fails.
pub(super) async fn tokens(llm: &dyn LLMPort, text: &str) -> usize {
    if !llm.counts_tokens_exactly() {
        return llm.count_tokens(text);
    }
    match llm.count_tokens_exact(text).await {
        Ok(tokens) => tokens,
        Err(error) => {
            debug!(%error, "The backend could not count the text; measuring by the estimate");
            llm.count_tokens(text)
        }
    }
}

/// A request of the reading round on `input`. It carries the planned
/// request's tools, forbidden: without them the prompt would differ from its
/// first token and the server would read it all again. It is a
/// transcription, so the same passages should read the same way every time.
fn reading_request(
    request: &CompletionRequest,
    input: Vec<CompletionInput>,
    max_output_tokens: Option<u32>,
    stop: &[String],
    time_budget: Option<Duration>,
) -> CompletionRequest {
    CompletionRequest {
        input,
        tools: request.tools.clone(),
        tool_choice: (!request.tools.is_empty()).then_some(ToolChoice::None),
        sampling: Some(SamplingOverride::deterministic()),
        max_output_tokens,
        stop: stop.to_vec(),
        priority: request.priority,
        cancel: request.cancel.clone(),
        cache_key: request.cache_key.clone(),
        time_budget,
        thinking: ThinkingMode::Off,
        ..Default::default()
    }
}

pub(super) fn assistant(text: &str) -> CompletionInput {
    CompletionInput::Message {
        role: "assistant".into(),
        content: text.into(),
    }
}

#[cfg(test)]
mod tests;
