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
//! So a grounded answer is written in two rounds on one cached prompt. The
//! model first reads (Chain-of-Note, with the notes copied rather than
//! paraphrased): one line per numbered passage, the sentences of it that bear
//! on the question as written, or a hyphen. It then writes the answer from its
//! own notes. Last, the app compares the two: a passage the notes copy word
//! for word that the answer never cites was skipped, and the model is asked
//! to add it. Grounding asks whether every cited claim is in a passage; this
//! asks whether every passage the model found is in the answer.
//!
//! Nothing here looks at the question or depends on its language. The inputs
//! are the labels the message shows, the model's notes and the labels in its
//! answer. Every loop ends when a finite set of labels stops shrinking. The
//! notes are written out of the answer's output reservation, all their sweeps
//! together; the answer is held to what the window has left after them, and
//! goes without them when they would take more of it than they leave. The
//! turn's clock and Stop bound all of it.
//!
//! `scripts/replay_answer_round.py` sends these same instructions to measure
//! the rounds; docs/design/2026-10-23-answer-coverage.md has the numbers.

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
use tokio::time::timeout;
use tokio_util::sync::CancellationToken;
use tracing::{debug, info, warn};
use unicode_segmentation::UnicodeSegmentation;

/// Closes a message that shows numbered passages. It is part of the message,
/// so the round that reads and the round that writes share the prompt to its
/// last token, and the turn's budget prices it with the rest.
pub(in crate::features::conversation::chat) const READING_INSTRUCTION: &str = "Before answering, go through every numbered passage above in order and write one line for each: its label, then the sentences of that passage that bear on my question, copied exactly as written and in the passage's own language. For a passage that says nothing bearing on my question, write its label and a single hyphen. Account for every label. Write only these lines; the answer comes after.";

/// Said after notes that left labels out. `{labels}` lists them.
pub(super) const SWEEP_INSTRUCTION: &str =
    "You did not account for {labels}. Write one line for each of them, in the same form.";

/// Said after the notes, last, to turn the reading into the answer.
pub(super) const WRITE_INSTRUCTION: &str = "Now answer my question. Your reading notes above say what each passage states about it: cover every passage you noted, cite each with its label, and take the wording from the passages themselves. Leave out a note that was mistaken.";

/// Said after an answer that left out passages its notes quote. `{quotes}`
/// gives each one's label and note.
pub(super) const CONTINUE_INSTRUCTION: &str = "Your answer does not use these passages, which your notes quote as bearing on my question: {quotes}. Continue your answer with what each of them says, in the same form as above, citing each with its label; do not repeat what you have written. If one of those notes was mistaken, write nothing for it.";

/// Between the answer and what a continuation adds to it.
const PARAGRAPH_BREAK: &str = "\n\n";

/// The numbered passages the message shows, by label, each as the text a note
/// has to be copied from to count.
#[derive(Debug, Default)]
pub(super) struct Passages(BTreeMap<u32, String>);

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
                .map(|(label, text)| (label, normalize_whitespace(&text)))
                .collect(),
        )
    }

    fn labels(&self) -> impl Iterator<Item = u32> + '_ {
        self.0.keys().copied()
    }
}

/// What the reading round wrote.
#[derive(Debug)]
pub(super) struct Notes {
    /// The notes, whole lines only: replayed to the model as its own message,
    /// and shown on the reading step.
    text: String,
    /// Each label's note, `None` for a hyphen.
    lines: BTreeMap<u32, Option<String>>,
    /// The labels whose note is copied word for word from that passage. Only
    /// these can find an answer incomplete: a note that is a paraphrase, a
    /// "none", or a quote from another passage proves nothing.
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
        .filter(|passages| !passages.0.is_empty())
}

/// The reading round: one line per passage, then a sweep for the labels the
/// notes left out, asked for again while each sweep accounts for at least one
/// of them.
///
/// The writing round reads the notes back out of the room the answer was
/// given, so all the sweeps together write no more than the answer may: each
/// is asked for what the reservation has left after the notes kept so far,
/// and the reading ends when nothing is left.
///
/// `Ok(None)` when no notes came of it — the provider failed, or the model
/// wrote no line under a label — and the answer is then asked for as it was
/// before reading existed (see [`withdraw`]). Only Stop and the turn's
/// deadline are errors.
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
        Some(format!("{} passages", passages.0.len())),
    );
    let reserved = request.max_output_tokens;
    let mut text = String::new();
    let mut lines: BTreeMap<u32, Option<String>> = BTreeMap::new();
    // Tokens of `text`, measured each time it grows.
    let mut written = 0usize;
    let mut sweeps = 0usize;
    loop {
        let missing = unaccounted(passages, &lines);
        let ceiling = reserved
            .map(|reserved| reserved.saturating_sub(u32::try_from(written).unwrap_or(u32::MAX)));
        if ceiling == Some(0) {
            break;
        }
        let mut input = request.input.clone();
        if !text.is_empty() {
            if missing.is_empty() {
                break;
            }
            input.push(assistant(&text));
            say_last(&mut input, &sweep_instruction(&missing));
            sweeps += 1;
        }
        let remaining = clock.remaining()?;
        let reading = CompletionRequest {
            input,
            // The same tools, forbidden: without them the prompt would differ
            // from its first token and the server would read it all again.
            tools: request.tools.clone(),
            tool_choice: (!request.tools.is_empty()).then_some(ToolChoice::None),
            // A transcription: the same passages should read the same way
            // every time.
            sampling: Some(SamplingOverride::deterministic()),
            max_output_tokens: ceiling,
            priority: request.priority,
            cancel: request.cancel.clone(),
            cache_key: request.cache_key.clone(),
            time_budget: Some(remaining),
            thinking: ThinkingMode::Off,
            ..Default::default()
        };
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
                warn!(%error, "The model could not read the passages; keeping what it had read");
                break;
            }
        };
        let whole = whole_lines(&response.text, &response.finish_reason);
        let mut accounted = false;
        for (label, note) in parse_notes(whole) {
            // The first reading takes every line it wrote; a sweep only the
            // labels it was asked for, so it cannot overwrite a note.
            let wanted = if text.is_empty() {
                passages.0.contains_key(&label)
            } else {
                missing.contains(&label)
            };
            if wanted {
                accounted |= missing.contains(&label);
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
    if lines.is_empty() {
        warn!("The reading round wrote no notes; answering without them");
        step.failed(Some("No notes".into()));
        return Ok(None);
    }
    let noted = lines
        .iter()
        .filter_map(|(label, note)| {
            let note = note.as_deref()?;
            verbatim(note, passages.0.get(label)?).then_some(*label)
        })
        .collect::<BTreeSet<u32>>();
    let quoted = lines.values().filter(|note| note.is_some()).count();
    info!(
        passages = passages.0.len(),
        accounted = lines.len(),
        quoted,
        verbatim = noted.len(),
        sweeps,
        tokens = written,
        "Read the passages before answering"
    );
    step.set_reasoning(Some(&text));
    step.done(Some(format!(
        "{quoted} of {} passages bear on the question",
        passages.0.len()
    )));
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
    let said = format!("\n\n{READING_INSTRUCTION}");
    let Some(item) = input.iter_mut().rev().find(|item| item.is_user()) else {
        return;
    };
    match item {
        CompletionInput::Message { content, .. } => *content = content.replacen(&said, "", 1),
        CompletionInput::Multimodal { parts, .. } => {
            for part in parts {
                if let ContentPart::Text { text } = part {
                    *text = text.replacen(&said, "", 1);
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
fn whole_lines<'t>(text: &'t str, finish_reason: &str) -> &'t str {
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

/// The notes as `(label, note)`, in the order written: a line that starts
/// with a label, bracketed or bare, then an optional `:`, `.` or dash, in
/// their full-width forms too. A note with no letter or digit in it — a
/// hyphen — is `None`. Lines without a label are not notes.
pub(super) fn parse_notes(text: &str) -> Vec<(u32, Option<String>)> {
    text.lines().filter_map(parse_line).collect()
}

fn parse_line(line: &str) -> Option<(u32, Option<String>)> {
    let rest = line.trim_start();
    let rest = rest
        .strip_prefix(|c: char| narrow(c) == '[')
        .unwrap_or(rest);
    let digits = rest
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(rest.len());
    let label = rest.get(..digits).filter(|digits| !digits.is_empty())?;
    let label = label.parse().ok()?;
    let rest = &rest[digits..];
    let rest = rest
        .strip_prefix(|c: char| narrow(c) == ']')
        .unwrap_or(rest)
        .trim_start();
    let note = rest
        .strip_prefix(|c: char| matches!(narrow(c), ':' | '.' | '-' | '–' | '—'))
        .unwrap_or(rest)
        .trim();
    let says_something = note.chars().any(|c| c.is_alphanumeric() || c == '_');
    Some((label, says_something.then(|| note.to_string())))
}

/// A full-width form (`：`, `．`, `［` …) read as the ASCII character it is
/// the wide form of, so a label is set off the same way in any script.
fn narrow(c: char) -> char {
    match c {
        '\u{FF01}'..='\u{FF5E}' => char::from_u32(u32::from(c) - 0xFEE0).unwrap_or(c),
        _ => c,
    }
}

/// Whether a note is copied from the passage: each of its sentences, with
/// whitespace runs read as one space and any quotation marks around it
/// dropped, occurs in the passage as written. Exact text, so it holds for any
/// language and needs no threshold. `passage` is already normalised.
pub(super) fn verbatim(note: &str, passage: &str) -> bool {
    let sentences: Vec<String> = sentences(note)
        .into_iter()
        .map(|sentence| {
            normalize_whitespace(sentence)
                .trim_matches(['"', '“', '”', '„', '«', '»'])
                .to_string()
        })
        .filter(|sentence| !sentence.is_empty())
        .collect();
    !sentences.is_empty()
        && sentences
            .iter()
            .all(|sentence| passage.contains(sentence.as_str()))
}

/// A note split into its sentences where Unicode puts their boundaries
/// (UAX #29: after `.`, `!` or `?` and a space, after `。`, `！`, `？` and
/// every other script's terminators), so the sentences it copied need not
/// have stood side by side in the passage.
fn sentences(note: &str) -> Vec<&str> {
    note.split_sentence_bounds().collect()
}

/// The labels the notes have not accounted for yet, in label order.
pub(super) fn unaccounted(passages: &Passages, lines: &BTreeMap<u32, Option<String>>) -> Vec<u32> {
    passages
        .labels()
        .filter(|label| !lines.contains_key(label))
        .collect()
}

/// The passages the notes quote that the answer does not cite.
pub(super) fn left_out(noted: &BTreeSet<u32>, cited: &BTreeSet<u32>) -> BTreeSet<u32> {
    noted.difference(cited).copied().collect()
}

fn sweep_instruction(labels: &[u32]) -> String {
    let labels = labels
        .iter()
        .map(|label| format!("[{label}]"))
        .collect::<Vec<_>>()
        .join(", ");
    SWEEP_INSTRUCTION.replace("{labels}", &labels)
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
async fn room_left(llm: &dyn LLMPort, request: &CompletionRequest) -> usize {
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
async fn tokens(llm: &dyn LLMPort, text: &str) -> usize {
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

fn assistant(text: &str) -> CompletionInput {
    CompletionInput::Message {
        role: "assistant".into(),
        content: text.into(),
    }
}

#[cfg(test)]
mod tests;
