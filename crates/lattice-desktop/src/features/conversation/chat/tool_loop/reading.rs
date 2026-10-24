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
//! answer. Every loop ends when a finite set of labels stops shrinking, each
//! round is held to the round's own output reservation, and the turn's clock
//! and Stop bound all of it.
//!
//! `scripts/replay_answer_round.py` sends these same instructions to measure
//! the rounds; docs/design/2026-10-23-answer-coverage.md has the numbers.

use super::clock::TurnClock;
use super::final_round::{say_last, written_call_start};
use super::generation::{generate, Generation};
use super::written_calls::WrittenCalls;
use super::{StreamEmitter, CUT_SHORT_NOTE};
use crate::features::conversation::chat::cancellation::cancelled_error;
use crate::features::conversation::chat::turn_record::{TurnRecorder, TurnStepKind};
use crate::features::conversation::chat::verification::cited_labels;
use lattice_core::application::contracts::sources::SourceDto;
use lattice_core::application::ports::llm_port::{
    CompletionInput, CompletionRequest, ContentPart, SamplingOverride, ThinkingMode, ToolChoice,
};
use lattice_core::application::ports::LLMPort;
use lattice_core::shared::error::Result;
use lattice_core::shared::text::normalize_whitespace;
use std::collections::{BTreeMap, BTreeSet};
use tokio::time::timeout;
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};

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
}

/// The passages to read, when the turn's message asks for a reading: it
/// closes with [`READING_INSTRUCTION`], which the prompt builder adds only to
/// a message that shows numbered passages.
pub(super) fn asked(input: &[CompletionInput], sources: &[SourceDto]) -> Option<Passages> {
    let message = input.iter().rev().find(|item| item.is_user())?.text();
    message
        .contains(READING_INSTRUCTION)
        .then(|| Passages::shown(sources, &message))
}

/// The reading round: one line per passage, then a sweep for the labels the
/// notes left out, asked for again while each sweep accounts for at least one
/// of them.
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
    let mut text = String::new();
    let mut lines: BTreeMap<u32, Option<String>> = BTreeMap::new();
    let mut sweeps = 0usize;
    loop {
        let missing = unaccounted(passages, &lines);
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
            max_output_tokens: request.max_output_tokens,
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
        let written = whole_lines(&response.text, &response.finish_reason);
        let mut accounted = false;
        for (label, note) in parse_notes(written) {
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
        text.push_str(written.trim());
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
        "Read the passages before answering"
    );
    step.set_reasoning(Some(&text));
    step.done(Some(format!(
        "{quoted} of {} passages bear on the question",
        passages.0.len()
    )));
    Ok(Some(Notes { text, lines, noted }))
}

/// Give the writing round the notes, as the model's own message, and the
/// instruction to write, said last. Everything before them is the reading
/// round's request unchanged, so the server reads only these.
pub(super) fn append_notes(request: &mut CompletionRequest, notes: &Notes) {
    request.input.push(assistant(&notes.text));
    say_last(&mut request.input, WRITE_INSTRUCTION);
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
/// cite, streamed after the answer, for as long as each continuation cites at
/// least one of them. The set shrinks every time round, so it ends.
///
/// An answer stopped at its output limit is left as it is: it already says
/// why it ends there. The answer stands as written when the turn's time runs
/// out or the provider fails; only Stop is an error.
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
    while !left.is_empty() && clock.remaining().is_ok() {
        info!(left_out = ?left, "The answer left out passages its notes quote; asking for them");
        say_last(&mut request.input, &continue_instruction(&left, notes));
        emitter.content(PARAGRAPH_BREAK)?;
        let round = generate(
            llm,
            request,
            emitter,
            recorder,
            cancel,
            clock,
            "Adding what was left out",
            WrittenCalls::Cut,
        )
        .await;
        let Generation { mut response, step } = match round {
            Ok(round) => round,
            Err(error) if cancel.is_cancelled() => return Err(error),
            Err(error) => {
                warn!(%error, "Could not add what the answer left out; keeping the answer");
                break;
            }
        };
        let added = cited_labels(&response.text).intersection(&left).count();
        step.done(Some(match added {
            0 => "nothing added".to_string(),
            1 => "1 passage added".to_string(),
            added => format!("{added} passages added"),
        }));
        let text = response.text.trim();
        if !text.is_empty() {
            answer.push_str(PARAGRAPH_BREAK);
            answer.push_str(text);
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
/// with a label, bracketed or bare, then an optional `:`, `.` or dash. A note
/// with no letter or digit in it — a hyphen — is `None`. Lines without a label
/// are not notes.
pub(super) fn parse_notes(text: &str) -> Vec<(u32, Option<String>)> {
    text.lines().filter_map(parse_line).collect()
}

fn parse_line(line: &str) -> Option<(u32, Option<String>)> {
    let rest = line.trim_start();
    let rest = rest.strip_prefix('[').unwrap_or(rest);
    let digits = rest
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(rest.len());
    let label = rest.get(..digits).filter(|digits| !digits.is_empty())?;
    let label = label.parse().ok()?;
    let rest = &rest[digits..];
    let rest = rest.strip_prefix(']').unwrap_or(rest).trim_start();
    let note = rest
        .strip_prefix([':', '.', '-', '–', '—'])
        .unwrap_or(rest)
        .trim();
    let says_something = note.chars().any(|c| c.is_alphanumeric() || c == '_');
    Some((label, says_something.then(|| note.to_string())))
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

/// A note split after each `.`, `!` or `?` followed by whitespace, so the
/// sentences it copied need not have stood side by side in the passage.
fn sentences(note: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut start = 0;
    let mut previous = None;
    for (index, c) in note.char_indices() {
        if c.is_whitespace() && matches!(previous, Some('.' | '!' | '?')) {
            out.push(&note[start..index]);
            start = index;
        }
        previous = Some(c);
    }
    out.push(&note[start..]);
    out
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
    let quotes = left
        .iter()
        .filter_map(|label| {
            let note = notes.lines.get(label)?.as_deref()?;
            Some(format!("[{label}] \"{note}\""))
        })
        .collect::<Vec<_>>()
        .join(" ");
    CONTINUE_INSTRUCTION.replace("{quotes}", &quotes)
}

fn assistant(text: &str) -> CompletionInput {
    CompletionInput::Message {
        role: "assistant".into(),
        content: text.into(),
    }
}

#[cfg(test)]
mod tests;
