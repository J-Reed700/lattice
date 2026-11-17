//! Condensing what a turn has read, so it can keep reading. When tool results
//! fill the window, the model chooses the sentences of everything the loop has
//! added that matter, and the app writes them, unchanged, into one notes
//! message. The latest round is condensed too, so a hybrid model's cache is not
//! cut mid-way. Sources, citation numbers and step cards live outside the
//! transcript, so an answer written after condensing cites as one written
//! before. Done at most twice a turn.

use super::choosing::choosing_call;
use super::clock::TurnClock;
use crate::features::conversation::chat::cancellation::cancelled_error;
use crate::features::conversation::chat::retrieval::{Legend, Passage};
use crate::features::conversation::chat::turn_record::{TurnRecorder, TurnStepKind};
use lattice_core::application::ports::llm_port::{CompletionInput, CompletionRequest, ContentPart};
use lattice_core::application::ports::LLMPort;
use lattice_core::application::services::model_contract::ask::{
    ask_each, Alone, Answered, Plain, Says, Settled,
};
use lattice_core::application::services::model_contract::legacy;
use lattice_core::application::services::model_contract::shape::Keeps;
use lattice_core::application::services::model_contract::shown::{
    Drawn, Kind, Picked, Scope, Showing, Told,
};
use lattice_core::application::services::model_contract::{Contract, ContractId};
use lattice_core::shared::error::Result;
use lattice_core::shared::text::sentences;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::ops::Range;
use tokio::time::timeout;
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};

/// Share of the window the loop's results may fill before they are condensed.
/// Condensing before the limit leaves the next round room for whole pages.
pub(super) const CONDENSE_AT_WINDOW_SHARE: f64 = 0.6;

/// Each condensing makes the next round re-read the transcript after the
/// planned request. Twice is room for a long investigation; more would spend
/// the turn re-reading its own notes.
pub(super) const MAX_CONDENSES_PER_TURN: usize = 2;

/// What the notes request tells the model.
pub(super) const CONDENSE_INSTRUCTION: &str = "Your context window is nearly full. Before you continue, choose from all the tool results you have so far the sentences that make up compact research notes. Keep every sentence with a fact, number, date, name, short quotation or URL that could matter to my question. Drop navigation text, boilerplate, repetition and anything that does not bear on my question. Do not answer my question yet. Answer with one line for each result that has sentences to keep: the result's handle, a colon, then the handle of each of those sentences, written out in full and separated by spaces. Leave out a result with nothing to keep. If nothing is worth keeping, answer `none`. Write nothing else.";

/// Opens the message the notes replace the results with. Also how a later
/// condensing recognises the notes as results.
pub(super) const NOTES_HEADER: &str = "Research notes so far. These condense the tool results you gathered earlier in this turn, to make room in your context window. Every citation label in them still refers to the same source. Continue the research from them, or answer if they are enough.";

/// Written under a result of which the model kept nothing.
const NOTHING_KEPT: &str = "Nothing here bears on the question.";

/// What one condensing did.
pub(super) struct Condensed {
    /// The tool results it replaced, by call id.
    pub(super) result_ids: HashSet<String>,
    pub(super) chars_before: usize,
    pub(super) chars_after: usize,
}

/// What the app wrote as a label in the results of the transcript, by the id
/// of the result, recorded when it wrote the text; and what the notes message
/// in the transcript holds, as the app wrote it.
#[derive(Debug, Default)]
pub(super) struct Labels {
    by_result: HashMap<String, Marked>,
    notes: Vec<Source>,
}

#[derive(Debug)]
struct Marked {
    passages: Vec<Passage>,
    legend: Vec<Legend>,
}

impl Labels {
    pub(super) fn keep(&mut self, id: &str, passages: Vec<Passage>, legend: Vec<Legend>) {
        if passages.is_empty() && legend.is_empty() {
            self.by_result.remove(id);
        } else {
            self.by_result
                .insert(id.to_owned(), Marked { passages, legend });
        }
    }

    /// The results with these ids are no longer in the transcript.
    pub(super) fn forget(&mut self, result_ids: &HashSet<String>) {
        self.by_result.retain(|id, _| !result_ids.contains(id));
    }
}

/// Characters of results the loop holds: every tool result after the planned
/// request, and notes standing in for earlier ones.
pub(super) fn loop_result_chars(input: &[CompletionInput], loop_start: usize) -> usize {
    input
        .get(loop_start..)
        .unwrap_or_default()
        .iter()
        .map(result_chars)
        .sum()
}

fn result_chars(item: &CompletionInput) -> usize {
    match item {
        CompletionInput::ToolResult { output, .. } => output.chars().count(),
        CompletionInput::Message { content, .. } if content.starts_with(NOTES_HEADER) => {
            content.chars().count()
        }
        _ => 0,
    }
}

fn holds_results(item: &CompletionInput) -> bool {
    match item {
        CompletionInput::ToolResult { .. } => true,
        CompletionInput::Message { content, .. } => content.starts_with(NOTES_HEADER),
        _ => false,
    }
}

/// Whether the loop's results have outgrown their share of the window, with
/// a condensing left to spend. A provider that reports no window is never
/// full.
pub(super) fn should_condense(
    window_chars: usize,
    input: &[CompletionInput],
    loop_start: usize,
    condensed_so_far: usize,
) -> bool {
    window_chars > 0
        && condensed_so_far < MAX_CONDENSES_PER_TURN
        && loop_result_chars(input, loop_start) as f64
            > window_chars as f64 * CONDENSE_AT_WINDOW_SHARE
}

/// What to condense: everything after the planned request, if it holds any
/// results.
fn condensable_span(input: &[CompletionInput], loop_start: usize) -> Option<Range<usize>> {
    let span = loop_start..input.len();
    input
        .get(span.clone())
        .is_some_and(|items| items.iter().any(holds_results))
        .then_some(span)
}

/// The call that chooses which sentences of the loop's results the notes keep.
struct ResearchNotes;

impl Contract for ResearchNotes {
    const ID: ContractId = ContractId::new("research-notes");
    type Input = Input;
}

/// One result of the loop, as the model names it.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Source {
    id: String,
    heading: Told<String>,
    sentences: Vec<Sentence>,
    /// The app's lines that label text of this result shown elsewhere in it.
    #[serde(skip)]
    legend: Vec<String>,
}

impl Kind for Source {
    const PREFIX: &'static str = "r";
    const DRAWN: Drawn = Drawn::Itself;
    type Ref = String;

    fn reference(&self) -> String {
        self.id.clone()
    }
}

/// One sentence of a result, named within it.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Sentence {
    at: usize,
    text: Told<String>,
}

impl Kind for Sentence {
    const PREFIX: &'static str = "u";
    const DRAWN: Drawn = Drawn::Itself;
    type Ref = usize;

    fn reference(&self) -> usize {
        self.at
    }
}

/// What is asked about the results still open.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Input {
    question: Told<String>,
    results: Vec<Source>,
}

impl Answered for ResearchNotes {
    type Answer = Keeps<Source, Sentence>;
    const ALONE: Alone = Alone::Pending;

    fn says(input: &Input, showing: &mut Showing<'_>) -> Result<Says> {
        let results = showing.show(&input.results)?;
        let mut task = format!("My question: {}\n\nThe tool results:", input.question.0);
        for (handle, source) in results.iter() {
            task.push_str(&format!("\n\n{} {}", handle.bracketed(), source.heading.0));
            let own = showing.under(handle, &source.sentences)?;
            for (sentence, thing) in own.iter() {
                task.push_str(&format!("\n{} {}", sentence.bracketed(), thing.text.0));
            }
        }
        Ok(Says::new(CONDENSE_INSTRUCTION, task))
    }
}

fn one_line(text: &str) -> String {
    text.replace(['\r', '\n'], " ")
}

impl Source {
    fn of(id: &str, heading: &str, text: &str) -> Self {
        Self {
            id: id.to_owned(),
            heading: Told(one_line(heading)),
            sentences: sentences(text)
                .into_iter()
                .filter_map(|(start, stop)| text.get(start..stop))
                .enumerate()
                .map(|(at, sentence)| Sentence {
                    at,
                    text: Told(one_line(sentence)),
                })
                .collect(),
            legend: Vec::new(),
        }
    }
}

/// The parts of a result's text the app labelled, and the rest.
struct Cut<'a> {
    passages: Vec<(&'a Passage, &'a str)>,
    legend: Vec<&'a str>,
    rest: String,
}

impl<'a> Cut<'a> {
    /// `text` cut at the ranges the app recorded. `None` when a recorded range
    /// does not lie in `text`, in order and apart.
    fn of(text: &'a str, marked: &'a Marked) -> Option<Self> {
        let mut cuts: Vec<Range<usize>> = Vec::new();
        for passage in &marked.passages {
            text.get(passage.line.clone())?;
            text.get(passage.body.clone())?;
            if passage.body.start < passage.line.end {
                return None;
            }
            cuts.push(passage.line.start..passage.body.end);
        }
        for legend in &marked.legend {
            text.get(legend.at.clone())?;
            cuts.push(legend.at.clone());
        }
        cuts.sort_by_key(|range| range.start);
        let mut cursor = 0;
        let mut pieces: Vec<&str> = Vec::new();
        for range in &cuts {
            if range.start < cursor {
                return None;
            }
            pieces.push(text.get(cursor..range.start)?);
            cursor = range.end;
        }
        pieces.push(text.get(cursor..)?);
        let rest = pieces
            .into_iter()
            .filter(|piece| !piece.trim().is_empty())
            .collect::<Vec<_>>()
            .join("\n\n");
        let passages = marked
            .passages
            .iter()
            .filter_map(|passage| Some((passage, text.get(passage.body.clone())?)))
            .collect();
        let legend = marked
            .legend
            .iter()
            .map(|legend| legend.line.as_str())
            .collect();
        Some(Self {
            passages,
            legend,
            rest,
        })
    }
}

/// A result as sources: each labelled source's text under the app's heading
/// for it, and what is left once every label the app wrote is taken out. A
/// result with nothing recorded is one source, as it stands.
fn sources_in(id: &str, heading: &str, text: &str, marked: Option<&Marked>) -> Vec<Source> {
    let Some(cut) = marked.and_then(|marked| Cut::of(text, marked)) else {
        if marked.is_some() {
            warn!("A result's recorded labels do not fit its text; offering it whole");
        }
        return vec![Source::of(id, heading, text)];
    };
    let mut sources = Vec::new();
    let mut rest = Source::of(id, heading, &cut.rest);
    if !rest.sentences.is_empty() {
        rest.legend = cut.legend.iter().map(|line| (*line).to_owned()).collect();
        sources.push(rest);
    }
    for (at, (passage, body)) in cut.passages.into_iter().enumerate() {
        let source = Source::of(&format!("{id}#{at}"), &passage.heading, body);
        if !source.sentences.is_empty() {
            sources.push(source);
        }
    }
    sources
}

/// The results in `span`: each tool result under the call that made it, and
/// the notes of an earlier condensing.
fn sources_of(input: &[CompletionInput], span: Range<usize>, labels: &Labels) -> Vec<Source> {
    input
        .get(span)
        .unwrap_or_default()
        .iter()
        .flat_map(|item| match item {
            CompletionInput::ToolResult { id, output } => {
                sources_in(id, &heading_of(input, id), output, labels.by_result.get(id))
            }
            CompletionInput::Message { content, .. } if content.starts_with(NOTES_HEADER) => {
                labels.notes.clone()
            }
            _ => Vec::new(),
        })
        .collect()
}

fn heading_of(input: &[CompletionInput], result_id: &str) -> String {
    input
        .iter()
        .find_map(|item| match item {
            CompletionInput::ToolCall {
                id,
                name,
                arguments,
            } if id == result_id => Some(format!("Result of {name} {arguments}")),
            _ => None,
        })
        .unwrap_or_else(|| "Tool result".to_owned())
}

/// What the user last wrote before the loop began.
fn question_of(input: &[CompletionInput], loop_start: usize) -> String {
    input
        .get(..loop_start)
        .unwrap_or_default()
        .iter()
        .rev()
        .find_map(|item| match item {
            CompletionInput::Message { role, content } if role == "user" => Some(content.clone()),
            CompletionInput::Multimodal { role, parts } if role == "user" => Some(
                parts
                    .iter()
                    .filter_map(|part| match part {
                        ContentPart::Text { text } => Some(text.as_str()),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
                    .join("\n"),
            ),
            _ => None,
        })
        .unwrap_or_default()
}

/// One result's part of the notes. A result the model was not asked about, or
/// whose answer did not settle, is kept whole. Each block is a passage of the
/// notes, and the source's legend lines follow what is kept of it. With the
/// block comes the source as the notes now hold it, when it kept a sentence.
fn block(
    source: &Source,
    kept: Option<&Settled<Vec<Picked<Sentence>>>>,
) -> (String, Option<Source>) {
    let lines: Vec<&str> = match kept {
        Some(Settled::Answered(kept)) => {
            let mut at: Vec<usize> = kept.iter().map(|picked| picked.reference).collect();
            at.sort_unstable();
            at.dedup();
            at.into_iter()
                .filter_map(|at| source.sentences.get(at))
                .map(|sentence| sentence.text.0.as_str())
                .collect()
        }
        _ => source
            .sentences
            .iter()
            .map(|sentence| sentence.text.0.as_str())
            .collect(),
    };
    let entries: Vec<String> = if lines.is_empty() && matches!(kept, Some(Settled::Answered(_))) {
        vec![NOTHING_KEPT.to_owned()]
    } else {
        lines.iter().map(|line| format!("- {line}")).collect()
    };
    let mut text = format!("{}:", source.heading.0);
    for entry in &entries {
        text.push('\n');
        text.push_str(entry);
    }
    if lines.is_empty() {
        return (text, None);
    }
    for line in &source.legend {
        text.push('\n');
        text.push_str(line);
    }
    let written = Source {
        id: source.id.clone(),
        heading: source.heading.clone(),
        sentences: lines
            .iter()
            .enumerate()
            .map(|(at, line)| Sentence {
                at,
                text: Told((*line).to_owned()),
            })
            .collect(),
        legend: source.legend.clone(),
    };
    (text, Some(written))
}

/// The notes: each result's kept sentences, in the order of the results, and
/// the sources the notes now hold.
fn notes_of(
    sources: &[Source],
    settled: Vec<Settled<Vec<Picked<Sentence>>>>,
) -> (String, Vec<Source>) {
    let mut settled = settled.into_iter();
    let mut blocks = Vec::new();
    let mut written = Vec::new();
    for source in sources {
        let kept = (!source.sentences.is_empty())
            .then(|| settled.next())
            .flatten();
        let (text, held) = block(source, kept.as_ref());
        blocks.push(text);
        written.extend(held);
    }
    (blocks.join("\n\n"), written)
}

/// Condenses the loop's results so far into one notes message. `Ok(None)` when
/// the notes could not be made or are no shorter than what they replace; only
/// Stop and the turn's deadline are errors.
pub(super) async fn condense(
    llm: &dyn LLMPort,
    request: &mut CompletionRequest,
    loop_start: usize,
    recorder: &TurnRecorder,
    cancel: &CancellationToken,
    clock: &TurnClock,
    labels: &mut Labels,
) -> Result<Option<Condensed>> {
    let Some(span) = condensable_span(request.input(), loop_start) else {
        return Ok(None);
    };
    let condensable = request.input().get(span.clone()).unwrap_or_default();
    let chars_before = condensable.iter().map(result_chars).sum();
    let step = recorder.begin_guarded(
        TurnStepKind::Generate,
        "Condensing what it has read",
        Some(format!("{chars_before} characters of results")),
    );
    let sources = sources_of(request.input(), span.clone(), &*labels);
    let question = Told(question_of(request.input(), loop_start));
    let Some(settled) = choose(llm, request, &sources, &question, cancel, clock).await? else {
        step.failed(Some("The model could not write notes".into()));
        return Ok(None);
    };
    let (notes, held) = notes_of(&sources, settled);
    let message = format!("{NOTES_HEADER}\n\n{notes}");
    let chars_after = message.chars().count();
    if chars_after >= chars_before {
        warn!(
            chars_before,
            chars_after, "Notes were no shorter than the results; carrying on without condensing"
        );
        step.failed(Some("The notes saved no room".into()));
        return Ok(None);
    }
    let result_ids = request
        .input()
        .get(span.clone())
        .unwrap_or_default()
        .iter()
        .filter_map(|item| match item {
            CompletionInput::ToolResult { id, .. } => Some(id.clone()),
            _ => None,
        })
        .collect();
    legacy::says(request).input.splice(
        span,
        [CompletionInput::Message {
            role: "user".into(),
            content: message,
        }],
    );
    labels.notes = held;
    info!(
        chars_before,
        chars_after, "Condensed the turn's tool results into notes"
    );
    step.done(Some(format!("{chars_before} → {chars_after} characters")));
    Ok(Some(Condensed {
        result_ids,
        chars_before,
        chars_after,
    }))
}

/// What the model kept of each of `sources` that has sentences, within the
/// turn's time; `None` when the provider fails it. Only Stop and the turn's
/// deadline are errors.
async fn choose(
    llm: &dyn LLMPort,
    request: &CompletionRequest,
    sources: &[Source],
    question: &Told<String>,
    cancel: &CancellationToken,
    clock: &TurnClock,
) -> Result<Option<Vec<Settled<Vec<Picked<Sentence>>>>>> {
    let remaining = clock.remaining()?;
    let asked: Vec<Source> = sources
        .iter()
        .filter(|source| !source.sentences.is_empty())
        .cloned()
        .collect();
    let call = choosing_call(request, remaining);
    let mut scope = Scope::for_question();
    let settled = tokio::select! {
        biased;
        _ = cancel.cancelled() => return Err(cancelled_error()),
        result = timeout(
            remaining,
            ask_each::<ResearchNotes>(
                llm,
                &Plain,
                &mut scope,
                &asked,
                |open, _| Input {
                    question: question.clone(),
                    results: open.iter().map(|source| (*source).clone()).collect(),
                },
                &call,
            ),
        ) => result.map_err(|_| clock.exhausted())?,
    };
    match settled {
        Ok(settled) => Ok(Some(settled)),
        Err(_) if cancel.is_cancelled() => Err(cancelled_error()),
        Err(error) => {
            warn!(%error, "Could not condense the turn's tool results; carrying on without");
            Ok(None)
        }
    }
}

/// The text a notes request shows the model.
#[cfg(test)]
pub(super) fn shown_in(request: &CompletionRequest) -> String {
    request
        .input()
        .iter()
        .filter_map(|item| match item {
            CompletionInput::Message { content, .. } => Some(content.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// A result's handle, its heading, and its sentences as handle and text.
#[cfg(test)]
type ShownResult = (String, String, Vec<(String, String)>);

/// An answer for the results the prompt shows, as lines: one for each result
/// whose heading `answered` takes and that keeps a sentence `keep` takes. A
/// result `answered` does not take gets a line naming a sentence one past its
/// last, which the reading cannot place; `none` when no result has a line.
#[cfg(test)]
pub(super) fn scripted_answer(
    prompt: &str,
    answered: impl Fn(&str) -> bool,
    keep: impl Fn(&str) -> bool,
) -> String {
    let mut shown: Vec<ShownResult> = Vec::new();
    for line in prompt.lines() {
        let Some((handle, rest)) = line.strip_prefix('[').and_then(|l| l.split_once("] ")) else {
            continue;
        };
        if handle.starts_with('r') {
            shown.push((handle.into(), rest.into(), Vec::new()));
        } else if handle.starts_with('u') {
            if let Some(last) = shown.last_mut() {
                last.2.push((handle.into(), rest.into()));
            }
        }
    }
    let lines: Vec<String> = shown
        .iter()
        .filter_map(|(handle, heading, sentences)| {
            if !answered(heading) {
                return Some(format!("{handle}: u{}", sentences.len() + 1));
            }
            let kept: Vec<&str> = sentences
                .iter()
                .filter(|(_, text)| keep(text))
                .map(|(handle, _)| handle.as_str())
                .collect();
            (!kept.is_empty()).then(|| format!("{handle}: {}", kept.join(" ")))
        })
        .collect();
    if lines.is_empty() {
        "none".to_owned()
    } else {
        lines.join("\n")
    }
}

#[cfg(test)]
mod tests;
