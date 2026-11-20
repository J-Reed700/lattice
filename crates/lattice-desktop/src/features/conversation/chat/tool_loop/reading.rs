//! Reads the numbered passages into notes before the answer is written, then
//! asks once more for the passages the notes quote that the answer does not
//! cite. The model chooses the sentences of each passage that bear on the
//! question (see `reading::contract`); the app writes each note from its own
//! copy of the passage. See docs/design/2026-10-23-answer-coverage.md.

use super::clock::TurnClock;
use super::final_round::{say_last, written_call_start};
use super::{answer_was_cut_short, tool_schema_text, StreamEmitter, CUT_SHORT_NOTE, DECLINED_NOTE};
use crate::features::conversation::chat::cancellation::cancelled_error;
use crate::features::conversation::chat::reply_text::ReplyText;
use crate::features::conversation::chat::retrieval::source_names;
use crate::features::conversation::chat::turn_record::{TurnRecorder, TurnStepKind};
use lattice_core::application::contracts::sources::SourceDto;
use lattice_core::application::ports::llm_port::{
    CompletionInput, CompletionRequest, ContentPart, ThinkingMode, ToolChoice,
    TEMPLATE_TOKENS_FOR_ANSWER, TEMPLATE_TOKENS_PER_MESSAGE,
};
use lattice_core::application::ports::LLMPort;
use lattice_core::application::services::model_contract::legacy;
use lattice_core::shared::error::Result;
use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet};
use tokio::time::timeout;
use tokio_util::sync::CancellationToken;
use tracing::{debug, info, warn};

mod contract;
mod notes;
use notes::as_kept;

/// Closes a message that shows numbered passages. It is part of the message so
/// the reading and writing rounds share the cached prompt.
pub(in crate::features::conversation::chat) const READING_INSTRUCTION: &str = "Before answering, go through every labelled passage above in order and write one line for each: its label, then the sentences of that passage that bear on my question, including any exception or condition, copied exactly as written and in the passage's own language. For a passage that says nothing bearing on my question, write its label and a single hyphen. When my question names a document, a passage from any other document gets a hyphen. Account for every label. Write only these lines; the answer comes after.";

/// Said after the notes, last, to turn the reading into the answer in strict mode.
pub(super) const WRITE_INSTRUCTION: &str = "Now answer my question directly, in the passages' own words, keeping their dates, figures and names as given. Cover every passage your notes quote and cite each with its label. Do not describe the notes or the passages, and say nothing about passages that do not bear on my question. Write in the language of my question, translating any passage in another.";

/// [`WRITE_INSTRUCTION`] in hybrid mode: cover and cite every quoted passage,
/// and allow uncited general knowledge when the question asks beyond what the
/// passages cover.
pub(super) const HYBRID_WRITE_INSTRUCTION: &str = "Now answer my question directly. Cover every passage your notes quote and cite each with its label, keeping their dates, figures and names as given. If my question asks for general or conceptual information beyond what the passages cover, answer that part naturally from general knowledge with no citations. Never guess private facts about my own files or add citations to uncited claims. Do not describe the notes or the passages, and say nothing about passages that do not bear on my question. Write in the language of my question, translating any passage in another.";

/// [`WRITE_INSTRUCTION`] for a round that can still search, where a part of the
/// question the passages leave open is for a search to answer.
pub(super) const SEARCH_OR_WRITE_INSTRUCTION: &str = "If the passages leave any part of my question unanswered, do not answer yet: search for that part now. Otherwise, start straight with the answer, in the passages' own words, keeping their dates, figures and names as given. Cover every passage your notes quote and cite each with its label. Do not describe the notes or the passages, do not say whether they are enough, and say nothing about passages that do not bear on my question. Write in the language of my question, translating any passage in another.";

/// [`SEARCH_OR_WRITE_INSTRUCTION`] in hybrid mode: search first when the
/// question asks about missing document details, or answer directly combining
/// cited passages with uncited general knowledge for general/conceptual gaps.
pub(super) const HYBRID_SEARCH_OR_WRITE_INSTRUCTION: &str = "If my question asks for missing details from my documents, search for that part now. Otherwise, start straight with the answer: cover every passage your notes quote and cite each with its label, keeping their dates, figures and names as given, and answer any general or conceptual part the passages do not cover naturally from general knowledge with no citations. Do not describe the notes or the passages, and say nothing about passages that do not bear on my question. Write in the language of my question, translating any passage in another.";

/// Closes the turn's message in place of [`READING_INSTRUCTION`] when no passage
/// bore on the question and the round can still search.
pub(super) const NOTHING_BEARS_INSTRUCTION: &str = "None of these passages bears on my question, so do not answer yet: search for it now, spelling out in full what it asks about.";

/// [`NOTHING_BEARS_INSTRUCTION`] in hybrid mode: search only when the question
/// asks about the user's documents or notes, otherwise answer directly from
/// general knowledge.
pub(super) const HYBRID_NOTHING_BEARS_INSTRUCTION: &str = "None of these passages bears on my question. If my question asks about something in my documents or notes, search for it now, spelling out in full what it asks about. Otherwise, answer my question directly and naturally from general knowledge with no citations, and do not mention these unrelated passages.";

/// Said after an answer that left out passages its notes quote. `{quotes}`
/// gives each one's label and note.
pub(super) const CONTINUE_INSTRUCTION: &str = "Your answer does not use these passages, which your notes quote as bearing on my question: {quotes}. Add only what my question still needs from them, continuing your answer in its own form and citing each with its label; do not repeat what you have written. If your answer already covers my question, reply with a single hyphen. Do not mention your notes or correct them.";

/// What a continuation replies when the answer needs nothing more.
const NOTHING_TO_ADD: &str = "-";

/// Between the answer and what a continuation adds to it.
const PARAGRAPH_BREAK: &str = "\n\n";

/// A labelled passage: the label the message shows it under, the document it
/// came from and its text.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct Passage {
    pub(super) label: String,
    pub(super) document: String,
    pub(super) text: String,
}

impl Passage {
    pub(super) fn new(
        label: impl Into<String>,
        document: impl Into<String>,
        text: impl Into<String>,
    ) -> Self {
        Self {
            label: label.into(),
            document: document.into(),
            text: text.into().trim().to_string(),
        }
    }
}

/// The numbered passages the message shows, by label.
#[derive(Debug, Default)]
pub(super) struct Passages(pub(super) BTreeMap<u32, Passage>);

impl Passages {
    /// The sources whose label the message shows. Sources sharing a label (an
    /// attachment in chunks) read as one text.
    fn shown(sources: &[SourceDto], message: &str) -> Self {
        let mut shown: BTreeMap<u32, (String, String, String)> = BTreeMap::new();
        for source in sources {
            let Some(number) = source.citation_id else {
                continue;
            };
            let label = source_names::label(source);
            if message.contains(&label) {
                let (written, document, text) = shown.entry(number).or_default();
                if document.is_empty() {
                    written.clone_from(&label);
                    document.clone_from(&source.file_name);
                }
                text.push(' ');
                text.push_str(&source.content);
            }
        }
        Self(
            shown
                .into_iter()
                .map(|(number, (label, document, text))| {
                    (number, Passage::new(label, document, text))
                })
                .collect(),
        )
    }

    pub(super) fn labels(&self) -> impl Iterator<Item = u32> + '_ {
        self.0.keys().copied()
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
    /// The note lines as kept (see [`notes::as_kept`]), replayed and shown on the step.
    text: String,
    /// Each label's note, the sentences the model chose from that passage;
    /// `None` when it chose none.
    lines: BTreeMap<u32, Option<String>>,
    /// `text` in tokens, as the backend counts them where it can.
    tokens: usize,
    /// How the message writes each noted passage's label.
    labels: BTreeMap<u32, String>,
}

impl Notes {
    /// The labels that have a note.
    fn noted(&self) -> BTreeSet<u32> {
        self.lines
            .iter()
            .filter(|(_, note)| note.is_some())
            .map(|(label, _)| *label)
            .collect()
    }
}

/// What came of reading the passages.
#[derive(Debug)]
pub(super) enum Reading {
    /// Notes of the sentences that bear on the question.
    Noted(Notes),
    /// Every passage was read, and the model chose no sentence of any.
    NothingBears,
    /// Nothing to go on (see [`withdraw`]).
    NoNotes,
}

/// The passages to read when the turn's message closes with
/// [`READING_INSTRUCTION`]; `None` when none of its labels is a source the turn holds.
pub(super) fn asked(input: &[CompletionInput], sources: &[SourceDto]) -> Option<Passages> {
    let message = input.iter().rev().find(|item| item.is_user())?.text();
    message
        .contains(READING_INSTRUCTION)
        .then(|| Passages::shown(sources, &message))
        .filter(|passages| !passages.is_empty())
}

/// Has the model choose the sentences of every passage that bear on the
/// question and writes each note from the app's copy of its passage. Only Stop
/// and the turn's deadline are errors.
pub(super) async fn read_passages(
    llm: &dyn LLMPort,
    request: &CompletionRequest,
    passages: &Passages,
    question: &str,
    recorder: &TurnRecorder,
    cancel: &CancellationToken,
    clock: &TurnClock,
) -> Result<Reading> {
    let step = recorder.begin_guarded(
        TurnStepKind::Generate,
        "Reading the passages",
        Some(format!("{} passages", passages.len())),
    );
    let lines = contract::read(llm, request, passages, question, cancel, clock).await?;
    if lines.is_empty() {
        warn!("The reading wrote no notes; answering without them");
        step.failed(Some("No notes".into()));
        return Ok(Reading::NoNotes);
    }
    if lines.values().all(Option::is_none) && passages.labels().all(|l| lines.contains_key(&l)) {
        info!(
            passages = passages.len(),
            "No passage bears on the question"
        );
        step.done(Some("No passage bears on the question".into()));
        return Ok(Reading::NothingBears);
    }
    let quoted = lines.values().filter(|note| note.is_some()).count();
    if quoted == 0 {
        info!(
            passages = passages.len(),
            accounted = lines.len(),
            "The reading chose no sentence of the passages it read; answering without it"
        );
        step.failed(Some("No passage noted".into()));
        return Ok(Reading::NoNotes);
    }
    let text = as_kept(&lines, passages);
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
        tokens = written,
        "Read the passages before answering"
    );
    Ok(Reading::Noted(Notes {
        text,
        lines,
        tokens: written,
        labels: passages
            .0
            .iter()
            .map(|(number, passage)| (*number, passage.label.clone()))
            .collect(),
    }))
}

/// Gives the writing round the notes, as the model's own message, then the
/// instruction to write. The answer is held to the room left in the planned
/// window; when that is smaller than the notes, nothing is added and this
/// returns `false`.
pub(super) async fn append_notes(
    llm: &dyn LLMPort,
    request: &mut CompletionRequest,
    notes: &Notes,
    strict_sources: bool,
) -> bool {
    let planned = request.input().len();
    legacy::says(request).input.push(assistant(&notes.text));
    let instruction = match (request.tools().is_empty(), strict_sources) {
        (true, true) => WRITE_INSTRUCTION,
        (true, false) => HYBRID_WRITE_INSTRUCTION,
        (false, true) => SEARCH_OR_WRITE_INSTRUCTION,
        (false, false) => HYBRID_SEARCH_OR_WRITE_INSTRUCTION,
    };
    say_last(legacy::says(request).input, instruction);
    let room = room_left(llm, request).await;
    if room < notes.tokens {
        info!(
            room,
            notes = notes.tokens,
            "The notes would leave the answer less room than they take; answering without them"
        );
        legacy::says(request).input.truncate(planned);
        return false;
    }
    *legacy::says(request).max_output_tokens = Some(within(request.max_output_tokens(), room));
    true
}

/// Whether a research turn's saved rounds hold its reading: the notes, and
/// the instruction to write from them said after.
pub(super) fn noted_in(transcript: &[CompletionInput]) -> bool {
    transcript.iter().any(|item| {
        let text = item.text();
        text.ends_with(WRITE_INSTRUCTION)
            || text.ends_with(HYBRID_WRITE_INSTRUCTION)
            || text.ends_with(SEARCH_OR_WRITE_INSTRUCTION)
            || text.ends_with(HYBRID_SEARCH_OR_WRITE_INSTRUCTION)
    })
}

/// Have a round that can search do so, in place of the reading that found
/// nothing. Without tools the reading is [`withdraw`]n like any other.
pub(super) fn search_instead(request: &mut CompletionRequest, strict_sources: bool) {
    if !request.tools().is_empty() {
        let instruction = if strict_sources {
            NOTHING_BEARS_INSTRUCTION
        } else {
            HYBRID_NOTHING_BEARS_INSTRUCTION
        };
        replace_reading(legacy::says(request).input, instruction);
    }
}

/// Take the reading instruction back out of the turn's message when no notes
/// came of it, so the answer is asked for on its passages and question alone.
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

/// Asks once for what the answer still needs from the passages its notes
/// quote and the answer does not cite; whatever comes back settles it. A
/// continuation is shown only once written whole, so a failed one, or one that
/// says nothing is needed, leaves the answer as it was. An answer cut at its
/// output limit is left alone. Only Stop is an error.
pub(super) async fn add_what_was_left_out(
    llm: &dyn LLMPort,
    request: &mut CompletionRequest,
    notes: &Notes,
    answer: &mut String,
    answered: &ReplyText,
    emitter: &StreamEmitter<'_>,
    recorder: &TurnRecorder,
    cancel: &CancellationToken,
    clock: &TurnClock,
) -> Result<()> {
    if answer.ends_with(CUT_SHORT_NOTE) || answer.ends_with(DECLINED_NOTE) {
        return Ok(());
    }
    let left = left_out(&notes.noted(), &answered.cited().into_iter().collect());
    if left.is_empty() {
        return Ok(());
    }
    let Ok(remaining) = clock.remaining() else {
        return Ok(());
    };
    // The turn ends after this, so the request is changed in place: calls
    // forbidden and no thinking, as for any round that may only write.
    *legacy::says(request).tool_choice = (!request.tools().is_empty()).then_some(ToolChoice::None);
    request.thinking = ThinkingMode::Off;
    say_last(
        legacy::says(request).input,
        &continue_instruction(&left, notes),
    );
    let room = room_left(llm, request).await;
    if room == 0 {
        info!(left_out = ?left, "No room left in the window to add what the answer left out; keeping the answer");
        return Ok(());
    }
    info!(left_out = ?left, "The answer left out passages its notes quote; asking for them");
    *legacy::says(request).max_output_tokens = Some(within(request.max_output_tokens(), room));
    request.time_budget = Some(remaining);
    let step = recorder.begin_guarded(TurnStepKind::Generate, "Adding what was left out", None);
    let completion = timeout(remaining, llm.complete(request));
    let response = tokio::select! {
        biased;
        _ = cancel.cancelled() => return Err(cancelled_error()),
        result = completion => result,
    };
    let response = match response {
        Ok(Ok(response)) => response,
        Ok(Err(_)) if cancel.is_cancelled() => return Err(cancelled_error()),
        Ok(Err(error)) => {
            warn!(%error, "Could not add what the answer left out; keeping the answer");
            step.failed(Some("nothing added".into()));
            return Ok(());
        }
        Err(_) => {
            warn!("The turn's time ran out adding what the answer left out; keeping the answer");
            step.failed(Some("nothing added".into()));
            return Ok(());
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
            return Ok(());
        }
    };
    if written.is_empty() || written == NOTHING_TO_ADD {
        step.done(Some("nothing added".into()));
        return Ok(());
    }
    step.done(Some("added".into()));
    let note = if cut_short { CUT_SHORT_NOTE } else { "" };
    let shown = format!("{PARAGRAPH_BREAK}{written}{note}");
    emitter.content(&shown)?;
    answer.push_str(&shown);
    Ok(())
}

/// The passages the notes quote that the answer does not cite.
pub(super) fn left_out(noted: &BTreeSet<u32>, cited: &BTreeSet<u32>) -> BTreeSet<u32> {
    noted.difference(cited).copied().collect()
}

fn continue_instruction(left: &BTreeSet<u32>, notes: &Notes) -> String {
    CONTINUE_INSTRUCTION.replace("{quotes}", &quotes(left, notes))
}

/// Each left-out label with its note, as a continuation is asked to restate.
fn quotes(left: &BTreeSet<u32>, notes: &Notes) -> String {
    left.iter()
        .filter_map(|number| {
            let note = notes.lines.get(number)?.as_deref()?;
            let label = notes.labels.get(number)?;
            Some(format!("{label} \"{note}\""))
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

/// `request`'s prompt in tokens, counted as the send-time gates count it.
pub(super) async fn prompt_tokens(llm: &dyn LLMPort, request: &CompletionRequest) -> usize {
    let text = request
        .input()
        .iter()
        .flat_map(|item| match item {
            CompletionInput::Native { value } => vec![Cow::Owned(value.to_string())],
            CompletionInput::Message { role, .. } | CompletionInput::Multimodal { role, .. } => {
                vec![Cow::Borrowed(role.as_str()), item.text()]
            }
            _ => vec![item.text()],
        })
        .chain((!request.tools().is_empty()).then(|| Cow::Owned(tool_schema_text(request.tools()))))
        .collect::<Vec<_>>()
        .join("\n");
    tokens(llm, &text).await
        + TEMPLATE_TOKENS_PER_MESSAGE * request.input().len()
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

pub(super) fn assistant(text: &str) -> CompletionInput {
    CompletionInput::Message {
        role: "assistant".into(),
        content: text.into(),
    }
}

#[cfg(test)]
mod tests;
