//! Condensing what a turn has read, so it can keep reading. When tool results
//! fill the window, the model chooses the sentences of everything the loop has
//! added that matter, and the app writes them, unchanged, into one notes
//! message. The latest round is condensed too, so a hybrid model's cache is not
//! cut mid-way. Sources, citation numbers and step cards live outside the
//! transcript, so an answer written after condensing cites as one written
//! before. Done at most twice a turn.

use super::clock::TurnClock;
use crate::features::conversation::chat::cancellation::cancelled_error;
use crate::features::conversation::chat::retrieval::{Legend, Passage, Rendered};
use crate::features::conversation::chat::turn_record::{TurnRecorder, TurnStepKind};
use lattice_core::application::ports::llm_port::{
    CompletionInput, CompletionRequest, ContentPart, SamplingOverride,
};
use lattice_core::application::ports::LLMPort;
use lattice_core::application::services::grounded_generation::CallOptions;
use lattice_core::application::services::model_contract::ask::{
    ask_each, Alone, Answered, Plain, Says, Settled,
};
use lattice_core::application::services::model_contract::legacy;
use lattice_core::application::services::model_contract::shape::{Authored, Each, Fields, Form};
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
pub(super) const CONDENSE_INSTRUCTION: &str = "Your context window is nearly full. Before you continue, choose from all the tool results you have so far the sentences that make up compact research notes. Keep every sentence with a fact, number, date, name, short quotation or URL that could matter to my question. Drop navigation text, boilerplate, repetition and anything that does not bear on my question. Do not answer my question yet. Answer with an entries list: each entry names a result in `of` by the handle shown beside it, and lists in `keep` the handles of that result's sentences to keep, which may be none.";

/// Opens the message the notes replace the results with. Also how a later
/// condensing recognises the notes as results.
pub(super) const NOTES_HEADER: &str = "Research notes so far. These condense the tool results you gathered earlier in this turn, to make room in your context window. Every citation label in them still refers to the same source. Continue the research from them, or answer if they are enough.";

/// Written under a result of which the model kept nothing.
const NOTHING_KEPT: &str = "Nothing here bears on the question.";

/// Names the notes of an earlier condensing among the results.
const EARLIER_NOTES: &str = "earlier-notes";

/// What one condensing did.
pub(super) struct Condensed {
    /// The tool results it replaced, by call id.
    pub(super) result_ids: HashSet<String>,
    pub(super) chars_before: usize,
    pub(super) chars_after: usize,
    /// The notes message as the model now reads it, with where its labelled text is.
    pub(super) notes: Rendered,
}

/// What the app wrote as a label in the results of the transcript, by the id
/// of the result (or of the earlier notes), recorded when it wrote the text.
#[derive(Debug, Default)]
pub(super) struct Labels {
    by_result: HashMap<String, Marked>,
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

    /// The notes that now stand where the earlier ones did.
    pub(super) fn keep_notes(&mut self, notes: Rendered) {
        self.keep(EARLIER_NOTES, notes.passages, notes.legend);
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

/// The sentences of one result the notes keep.
#[derive(Debug, Clone, PartialEq)]
struct Kept {
    sentences: Vec<Picked<Sentence>>,
}

impl Authored for Kept {
    fn form() -> Form {
        Form::new().cite_own::<Sentence>("keep")
    }

    fn read(fields: &mut Fields<'_, '_>) -> Option<Self> {
        Some(Self {
            sentences: fields.cite_own("keep"),
        })
    }
}

impl Answered for ResearchNotes {
    type Answer = Each<Source, Kept>;
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
    /// `text` from `from` on, cut at the ranges the app recorded. `None` when
    /// a recorded range does not lie in `text`, in order and apart.
    fn of(text: &'a str, from: usize, marked: &'a Marked) -> Option<Self> {
        let mut cuts: Vec<Range<usize>> = Vec::new();
        for passage in &marked.passages {
            text.get(passage.line.clone())?;
            text.get(passage.body.clone())?;
            if passage.body.start < passage.line.end {
                return None;
            }
            let mut end = passage.body.end;
            for legend in &passage.legend {
                text.get(legend.at.clone())?;
                if legend.at.start < passage.body.end {
                    return None;
                }
                end = end.max(legend.at.end);
            }
            cuts.push(passage.line.start..end);
        }
        for legend in &marked.legend {
            text.get(legend.at.clone())?;
            cuts.push(legend.at.clone());
        }
        cuts.sort_by_key(|range| range.start);
        let mut cursor = from;
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
        let legend = marked.legend.iter().map(|legend| legend.line.as_str()).collect();
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
fn sources_in(
    id: &str,
    heading: &str,
    text: &str,
    from: usize,
    marked: Option<&Marked>,
) -> Vec<Source> {
    let Some(cut) = marked.and_then(|marked| Cut::of(text, from, marked)) else {
        return vec![Source::of(id, heading, text.get(from..).unwrap_or_default())];
    };
    let mut sources = Vec::new();
    let mut rest = Source::of(id, heading, &cut.rest);
    if !rest.sentences.is_empty() {
        rest.legend = cut.legend.iter().map(|line| (*line).to_owned()).collect();
        sources.push(rest);
    }
    for (at, (passage, body)) in cut.passages.into_iter().enumerate() {
        let mut source = Source::of(&format!("{id}#{at}"), &passage.heading, body);
        if !source.sentences.is_empty() {
            source.legend = passage
                .legend
                .iter()
                .map(|legend| legend.line.clone())
                .collect();
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
            CompletionInput::ToolResult { id, output } => sources_in(
                id,
                &heading_of(input, id),
                output,
                0,
                labels.by_result.get(id),
            ),
            CompletionInput::Message { content, .. } if content.starts_with(NOTES_HEADER) => {
                sources_in(
                    EARLIER_NOTES,
                    "Earlier notes",
                    content,
                    NOTES_HEADER.len(),
                    labels.by_result.get(EARLIER_NOTES),
                )
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
/// notes, and the source's legend lines follow what is kept of it.
fn block(source: &Source, kept: Option<&Settled<Kept>>) -> Rendered {
    let lines: Vec<&str> = match kept {
        Some(Settled::Answered(kept)) => {
            let mut at: Vec<usize> = kept
                .sentences
                .iter()
                .map(|picked| picked.reference)
                .collect();
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
    let line = 0..text.len();
    for entry in &entries {
        text.push('\n');
        text.push_str(entry);
    }
    let mut rendered = Rendered::from(text);
    let body = if entries.is_empty() {
        line.end..line.end
    } else {
        line.end + 1..rendered.text.len()
    };
    let mut legend = Vec::new();
    if !lines.is_empty() {
        for line in &source.legend {
            rendered.text.push('\n');
            let at = rendered.text.len()..rendered.text.len() + line.len();
            rendered.text.push_str(line);
            legend.push(Legend {
                line: line.clone(),
                at,
            });
        }
    }
    rendered.passages.push(Passage {
        heading: source.heading.0.clone(),
        line,
        body,
        legend,
    });
    rendered
}

/// The notes: each result's kept sentences, in the order of the results.
fn notes_of(sources: &[Source], settled: Vec<Settled<Kept>>) -> Rendered {
    let mut settled = settled.into_iter();
    let mut notes = Rendered::default();
    for (at, source) in sources.iter().enumerate() {
        let kept = (!source.sentences.is_empty())
            .then(|| settled.next())
            .flatten();
        if at > 0 {
            notes.append("\n\n".to_owned().into());
        }
        notes.append(block(source, kept.as_ref()));
    }
    notes
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
    labels: &Labels,
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
    let sources = sources_of(request.input(), span.clone(), labels);
    let question = Told(question_of(request.input(), loop_start));
    let Some(settled) = choose(llm, request, &sources, &question, cancel, clock).await? else {
        step.failed(Some("The model could not write notes".into()));
        return Ok(None);
    };
    let notes = notes_of(&sources, settled).wrapped(&format!("{NOTES_HEADER}\n\n"), "");
    let message = notes.text.clone();
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
    info!(
        chars_before,
        chars_after, "Condensed the turn's tool results into notes"
    );
    step.done(Some(format!("{chars_before} → {chars_after} characters")));
    Ok(Some(Condensed {
        result_ids,
        chars_before,
        chars_after,
        notes,
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
) -> Result<Option<Vec<Settled<Kept>>>> {
    let remaining = clock.remaining()?;
    let asked: Vec<Source> = sources
        .iter()
        .filter(|source| !source.sentences.is_empty())
        .cloned()
        .collect();
    let call = CallOptions {
        priority: request.priority,
        cancel: request.cancel.clone(),
        cache_key: request.cache_key.clone(),
        // Notes are a selection, not a composition: the same results should
        // condense the same way every time.
        sampling: Some(SamplingOverride::deterministic()),
        time_budget: Some(remaining),
        ..Default::default()
    };
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

/// An answer for the results the prompt shows: an entry for each whose
/// heading `answered` takes, keeping the sentences `keep` takes.
#[cfg(test)]
pub(super) fn scripted_answer(
    prompt: &str,
    answered: impl Fn(&str) -> bool,
    keep: impl Fn(&str) -> bool,
) -> String {
    let mut shown: Vec<(String, String, Vec<(String, String)>)> = Vec::new();
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
    let entries: Vec<serde_json::Value> = shown
        .iter()
        .filter(|(_, heading, _)| answered(heading))
        .map(|(handle, _, sentences)| {
            let kept: Vec<&String> = sentences
                .iter()
                .filter(|(_, text)| keep(text))
                .map(|(handle, _)| handle)
                .collect();
            serde_json::json!({ "of": handle, "keep": kept })
        })
        .collect();
    serde_json::json!({ "entries": entries }).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::future::BoxFuture;
    use lattice_core::application::ports::llm_port::CompletionResponse;
    use lattice_core::application::services::model_contract::test_support::snapshot::assert_requests;
    use lattice_core::application::services::model_contract::test_support::taint::{holds, Marks};
    use lattice_core::application::services::model_contract::test_support::{ScriptedModel, Sent};

    fn message(role: &str, content: &str) -> CompletionInput {
        CompletionInput::Message {
            role: role.into(),
            content: content.into(),
        }
    }

    fn call(id: &str) -> CompletionInput {
        CompletionInput::ToolCall {
            id: id.into(),
            name: "fetch_url_content".into(),
            arguments: serde_json::json!({ "url": format!("https://example.test/{id}") }),
        }
    }

    fn result(id: &str, chars: usize) -> CompletionInput {
        CompletionInput::ToolResult {
            id: id.into(),
            output: "r".repeat(chars),
        }
    }

    fn transcript(rounds: &[&[usize]]) -> Vec<CompletionInput> {
        let mut input = vec![message("system", "policy"), message("user", "question")];
        for (round, results) in rounds.iter().enumerate() {
            for index in 0..results.len() {
                input.push(call(&format!("r{round}c{index}")));
            }
            for (index, chars) in results.iter().enumerate() {
                input.push(result(&format!("r{round}c{index}"), *chars));
            }
        }
        input
    }

    #[test]
    fn the_results_count_and_the_planned_request_does_not() {
        let input = transcript(&[&[100, 200], &[300]]);
        assert_eq!(loop_result_chars(&input, 2), 600);
        let mut with_notes = input.clone();
        with_notes.insert(2, message("user", &format!("{NOTES_HEADER}\n\nfacts")));
        assert_eq!(
            loop_result_chars(&with_notes, 2),
            600 + NOTES_HEADER.len() + 7
        );
    }

    #[test]
    fn it_condenses_past_the_share_and_at_most_twice() {
        let input = transcript(&[&[7_000]]);
        assert!(should_condense(10_000, &input, 2, 0));
        assert!(should_condense(10_000, &input, 2, 1));
        assert!(!should_condense(10_000, &input, 2, MAX_CONDENSES_PER_TURN));
        assert!(!should_condense(20_000, &input, 2, 0), "under the share");
        assert!(!should_condense(0, &input, 2, 0), "no window reported");
    }

    #[test]
    fn everything_after_the_planned_request_is_condensed_earlier_notes_included() {
        let mut input = transcript(&[&[3_000], &[2_000, 1_000]]);
        assert_eq!(condensable_span(&input, 2), Some(2..input.len()));
        input.splice(
            2..4,
            [message(
                "user",
                &format!("{NOTES_HEADER}\n\n{}", "n".repeat(400)),
            )],
        );
        assert_eq!(condensable_span(&input, 2), Some(2..input.len()));
    }

    #[test]
    fn nothing_to_condense_without_results() {
        let input = transcript(&[]);
        assert_eq!(condensable_span(&input, 2), None);
    }

    const QUESTION: &str = "When did the harbour bridge open?";
    const OPENED: &str = "The harbour bridge opened in 1932 [1].";
    const WIDENED: &str = "Its deck was widened in 1960 [2].";
    const NOISE: &str =
        "Sign up for the newsletter to hear about tide tables and kiln rotas every single week.";

    fn noise() -> String {
        vec![NOISE; 10].join(" ")
    }

    fn request_of(items: Vec<CompletionInput>) -> CompletionRequest {
        legacy::completion(legacy::Completion {
            input: items,
            ..Default::default()
        })
    }

    /// The pages the harbour tools returned: a call and its result for each.
    fn pages(pages: &[(&str, String)]) -> Vec<CompletionInput> {
        let mut input = vec![message("system", "policy"), message("user", QUESTION)];
        input.extend(pages.iter().map(|(id, _)| call(id)));
        input.extend(pages.iter().map(|(id, text)| CompletionInput::ToolResult {
            id: (*id).into(),
            output: text.clone(),
        }));
        input
    }

    fn two_pages() -> Vec<CompletionInput> {
        pages(&[
            ("a", format!("{OPENED} {}", noise())),
            ("b", format!("{WIDENED} {}", noise())),
        ])
    }

    fn prompt_of(sent: &Sent) -> String {
        shown_in(&sent.request)
    }

    fn reply(text: String) -> BoxFuture<'static, Result<CompletionResponse>> {
        Box::pin(std::future::ready(Ok(CompletionResponse::from_text(text))))
    }

    fn cites(text: &str) -> bool {
        text.contains('[')
    }

    async fn condensed(
        model: &ScriptedModel,
        request: &mut CompletionRequest,
    ) -> Option<Condensed> {
        condensed_with(model, request, &Labels::default()).await
    }

    async fn condensed_with(
        model: &ScriptedModel,
        request: &mut CompletionRequest,
        labels: &Labels,
    ) -> Option<Condensed> {
        condense(
            model,
            request,
            2,
            &TurnRecorder::silent("conversation", "request"),
            &CancellationToken::new(),
            &TurnClock::new(
                std::time::Duration::from_secs(60),
                std::time::Duration::ZERO,
            ),
            labels,
        )
        .await
        .unwrap()
    }

    fn notes_in(request: &CompletionRequest) -> String {
        match request.input().last() {
            Some(CompletionInput::Message { content, .. }) => content.clone(),
            other => panic!("the notes should end the transcript: {other:?}"),
        }
    }

    #[tokio::test]
    async fn the_chosen_sentences_are_written_as_they_stand() {
        let model = ScriptedModel::new().on_request::<ResearchNotes>(|sent, _| {
            reply(scripted_answer(&prompt_of(sent), |_| true, cites))
        });
        let mut request = request_of(two_pages());
        let condensed = condensed(&model, &mut request).await.expect("notes");
        assert!(condensed.chars_after < condensed.chars_before);
        assert_eq!(
            condensed.result_ids,
            HashSet::from(["a".to_string(), "b".to_string()])
        );
        let notes = notes_in(&request);
        assert!(notes.starts_with(NOTES_HEADER));
        for kept in [OPENED, WIDENED, "example.test/a", "example.test/b"] {
            assert!(notes.contains(kept), "{kept}");
        }
        assert!(!notes.contains(NOISE));
        assert_eq!(request.input().len(), 3, "the notes replace the results");
        assert!(prompt_of(&model.sent()[0]).contains(QUESTION));
    }

    #[tokio::test]
    async fn a_result_left_out_is_asked_for_again_alone_and_the_others_stand() {
        let model = ScriptedModel::new().on_request::<ResearchNotes>(|sent, nth| {
            let first = nth == 0;
            reply(scripted_answer(
                &prompt_of(sent),
                |heading| !first || !heading.contains("example.test/b"),
                cites,
            ))
        });
        let mut request = request_of(two_pages());
        condensed(&model, &mut request).await.expect("notes");
        let sent = model.sent();
        assert_eq!(sent.len(), 2);
        let second = prompt_of(&sent[1]);
        assert!(second.contains(WIDENED) && !second.contains(OPENED));
        let notes = notes_in(&request);
        assert!(notes.contains(OPENED) && notes.contains(WIDENED));
    }

    #[tokio::test]
    async fn a_result_that_stays_unanswered_is_kept_whole() {
        let model = ScriptedModel::new().on_request::<ResearchNotes>(|sent, _| {
            reply(scripted_answer(
                &prompt_of(sent),
                |heading| !heading.contains("example.test/b"),
                cites,
            ))
        });
        let mut request = request_of(two_pages());
        condensed(&model, &mut request).await.expect("notes");
        let notes = notes_in(&request);
        let (first, second) = notes.split_once("example.test/b").expect("both results");
        assert!(first.contains(OPENED) && !first.contains(NOISE));
        assert!(second.contains(WIDENED) && second.contains(NOISE));
    }

    #[tokio::test]
    async fn a_result_of_which_nothing_is_kept_says_so() {
        let model = ScriptedModel::new().on_request::<ResearchNotes>(|sent, _| {
            reply(scripted_answer(
                &prompt_of(sent),
                |_| true,
                |text| text == OPENED,
            ))
        });
        let mut request = request_of(two_pages());
        condensed(&model, &mut request).await.expect("notes");
        let notes = notes_in(&request);
        let (_, second) = notes.split_once("example.test/b").expect("both results");
        assert!(second.contains(NOTHING_KEPT) && !second.contains(WIDENED));
    }

    #[tokio::test]
    async fn notes_no_shorter_than_the_results_are_not_used() {
        let model = ScriptedModel::new().on_request::<ResearchNotes>(|sent, _| {
            reply(scripted_answer(&prompt_of(sent), |_| true, |_| true))
        });
        let mut request = request_of(two_pages());
        assert!(condensed(&model, &mut request).await.is_none());
        assert_eq!(request.input().len(), two_pages().len());
    }

    #[tokio::test]
    async fn earlier_notes_are_condensed_with_the_results() {
        let earlier = format!(
            "{NOTES_HEADER}\n\n{}",
            vec![format!("- {OPENED}"), format!("- {NOISE}")]
                .into_iter()
                .cycle()
                .take(12)
                .collect::<Vec<_>>()
                .join("\n")
        );
        let mut items = vec![
            message("system", "policy"),
            message("user", QUESTION),
            message("user", &earlier),
        ];
        items.push(call("b"));
        items.push(CompletionInput::ToolResult {
            id: "b".into(),
            output: format!("{WIDENED} {}", noise()),
        });
        let model = ScriptedModel::new().on_request::<ResearchNotes>(|sent, _| {
            reply(scripted_answer(&prompt_of(sent), |_| true, cites))
        });
        let mut request = request_of(items);
        condensed(&model, &mut request).await.expect("notes");
        assert!(prompt_of(&model.sent()[0]).contains("Earlier notes"));
        let notes = notes_in(&request);
        assert!(notes.contains("Earlier notes:") && notes.contains(WIDENED));
        assert_eq!(request.input().len(), 3);
    }

    #[test]
    fn the_notes_request_shows_what_the_model_is_told_and_nothing_internal() {
        let mut marks = Marks::new();
        let input = Input {
            question: marks.told(QUESTION.to_owned()),
            results: vec![Source {
                id: marks.reference("call-0123456789".to_owned()),
                heading: marks.told("Result of fetch_url_content".to_owned()),
                sentences: vec![
                    Sentence {
                        at: 0,
                        text: marks.told(OPENED.to_owned()),
                    },
                    Sentence {
                        at: 1,
                        text: marks.told(NOISE.to_owned()),
                    },
                ],
                legend: Vec::new(),
            }],
        };
        holds::<ResearchNotes>(&input, &marks);
    }

    const BRIDGE: &str = "The harbour bridge opened in 1932.";
    const DECK: &str = "Its deck was widened in 1960.";
    const TIDES: &str = "Tides rise twice a day.";
    const HEADING: &str = "Citable passage [s3] — Page B";
    const LEGEND_A: &str =
        "Citable passage [s1] — Page A (https://example.test/a): its text is in this result above.";
    const LEGEND_B: &str =
        "Citable passage [s2] — Page B (https://example.test/b): its text is in this result above.";

    fn passage_of(heading: &str, body: &str) -> Rendered {
        let text = format!("{heading}\n{body}");
        Rendered {
            passages: vec![Passage {
                heading: heading.to_owned(),
                line: 0..heading.len(),
                body: heading.len() + 1..text.len(),
                legend: Vec::new(),
            }],
            text,
            legend: Vec::new(),
        }
    }

    fn legend_of(line: &str) -> Rendered {
        Rendered {
            text: line.to_owned(),
            passages: Vec::new(),
            legend: vec![Legend {
                line: line.to_owned(),
                at: 0..line.len(),
            }],
        }
    }

    fn labelled_result(id: &str, rendered: &Rendered, labels: &mut Labels) -> CompletionInput {
        labels.keep(id, rendered.passages.clone(), rendered.legend.clone());
        CompletionInput::ToolResult {
            id: id.into(),
            output: rendered.text.clone(),
        }
    }

    fn no_label_is_a_sentence(prompt: &str) {
        for line in prompt.lines().filter(|line| line.starts_with("[u")) {
            assert!(!line.contains("Citable passage"), "{line}");
        }
    }

    fn with_passage_and_noise() -> (Vec<CompletionInput>, Labels) {
        let mut result = Rendered::from(format!("Page A\n{}\n\n", noise()));
        result.append(passage_of(HEADING, &format!("{TIDES} {BRIDGE} {}", noise())));
        let mut labels = Labels::default();
        let output = labelled_result("a", &result, &mut labels);
        let items = vec![
            message("system", "policy"),
            message("user", QUESTION),
            call("a"),
            output,
        ];
        (items, labels)
    }

    #[tokio::test]
    async fn a_kept_sentence_stands_under_its_passage_s_label_without_the_model_keeping_it() {
        let (items, labels) = with_passage_and_noise();
        let model = ScriptedModel::new().on_request::<ResearchNotes>(|sent, _| {
            reply(scripted_answer(
                &prompt_of(sent),
                |_| true,
                |text| text == BRIDGE,
            ))
        });
        let mut request = request_of(items);
        condensed_with(&model, &mut request, &labels)
            .await
            .expect("notes");
        let notes = notes_in(&request);
        assert!(
            notes.contains(&format!("{HEADING}:\n- {BRIDGE}")),
            "{notes}"
        );
        assert!(!notes.contains(TIDES), "{notes}");
        let prompt = prompt_of(&model.sent()[0]);
        no_label_is_a_sentence(&prompt);
        assert!(
            prompt
                .lines()
                .any(|line| line.starts_with("[r") && line.ends_with(HEADING)),
            "{prompt}"
        );
    }

    #[tokio::test]
    async fn a_page_labelled_by_a_legend_line_keeps_it_with_a_kept_sentence_only() {
        let mut labels = Labels::default();
        let mut first = Rendered::from(format!("{BRIDGE} {}\n\n", noise()));
        first.append(legend_of(LEGEND_A));
        let mut second = Rendered::from(format!("{DECK} {}\n\n", noise()));
        second.append(legend_of(LEGEND_B));
        let mut items = vec![
            message("system", "policy"),
            message("user", QUESTION),
            call("a"),
            call("b"),
        ];
        items.push(labelled_result("a", &first, &mut labels));
        items.push(labelled_result("b", &second, &mut labels));
        let model = ScriptedModel::new().on_request::<ResearchNotes>(|sent, _| {
            reply(scripted_answer(
                &prompt_of(sent),
                |_| true,
                |text| text == BRIDGE,
            ))
        });
        let mut request = request_of(items);
        condensed_with(&model, &mut request, &labels)
            .await
            .expect("notes");
        let notes = notes_in(&request);
        assert!(notes.contains(&format!("- {BRIDGE}\n{LEGEND_A}")), "{notes}");
        assert!(!notes.contains(LEGEND_B), "{notes}");
        assert!(notes.contains(NOTHING_KEPT), "{notes}");
        no_label_is_a_sentence(&prompt_of(&model.sent()[0]));
    }

    #[tokio::test]
    async fn a_second_condensing_keeps_the_labels_of_the_first() {
        let (items, mut labels) = with_passage_and_noise();
        let model = ScriptedModel::new().on_request::<ResearchNotes>(|sent, _| {
            reply(scripted_answer(
                &prompt_of(sent),
                |_| true,
                |text| text.contains("1932"),
            ))
        });
        let mut request = request_of(items);
        let first = condensed_with(&model, &mut request, &labels)
            .await
            .expect("notes");
        labels.keep_notes(first.notes);
        let mut later = request.input().to_vec();
        later.push(call("c"));
        later.push(result("c", 1_200));
        let mut request = request_of(later);
        condensed_with(&model, &mut request, &labels)
            .await
            .expect("notes");
        let notes = notes_in(&request);
        assert!(notes.contains(&format!("{HEADING}:\n- ")), "{notes}");
        assert!(notes.contains(BRIDGE), "{notes}");
        assert!(!notes.contains(TIDES), "{notes}");
        let prompt = prompt_of(&model.sent()[1]);
        no_label_is_a_sentence(&prompt);
        assert!(
            !prompt.contains("Earlier notes"),
            "every earlier block is offered under its own heading: {prompt}"
        );
    }

    fn heading_for(id: &str) -> String {
        format!(
            "Result of fetch_url_content {}",
            serde_json::json!({ "url": format!("https://example.test/{id}") })
        )
    }

    #[tokio::test]
    async fn each_block_keeps_its_own_legend_line_through_two_condensings() {
        let mut labels = Labels::default();
        let mut first = Rendered::from(format!("{BRIDGE} {}\n\n", noise()));
        first.append(legend_of(LEGEND_A));
        let mut second = Rendered::from(format!("{DECK} {}\n\n", noise()));
        second.append(legend_of(LEGEND_B));
        let mut items = vec![
            message("system", "policy"),
            message("user", QUESTION),
            call("a"),
            call("b"),
        ];
        items.push(labelled_result("a", &first, &mut labels));
        items.push(labelled_result("b", &second, &mut labels));
        let model = ScriptedModel::new().on_request::<ResearchNotes>(|sent, _| {
            reply(scripted_answer(
                &prompt_of(sent),
                |_| true,
                |text| text.contains("1932") || text.contains("1960"),
            ))
        });
        let mut request = request_of(items);
        let first = condensed_with(&model, &mut request, &labels)
            .await
            .expect("notes");
        labels.keep_notes(first.notes);
        let mut later = request.input().to_vec();
        later.push(call("c"));
        later.push(result("c", 1_200));
        let mut request = request_of(later);
        condensed_with(&model, &mut request, &labels)
            .await
            .expect("notes");
        let notes = notes_in(&request);
        let (head_a, head_b) = (heading_for("a"), heading_for("b"));
        let at = |needle: &str| notes.find(needle).unwrap_or(usize::MAX);
        assert!(at(&head_a) < at(BRIDGE), "{notes}");
        assert!(at(BRIDGE) < at(LEGEND_A), "{notes}");
        assert!(at(LEGEND_A) < at(&head_b), "{notes}");
        assert!(at(&head_b) < at(DECK), "{notes}");
        assert!(at(DECK) < at(LEGEND_B), "{notes}");
        assert_eq!(notes.matches(LEGEND_A).count(), 1, "{notes}");
        assert_eq!(notes.matches(LEGEND_B).count(), 1, "{notes}");
        let prompt = prompt_of(&model.sent()[1]);
        no_label_is_a_sentence(&prompt);
        assert!(!prompt.contains("Earlier notes"), "{prompt}");
    }

    #[test]
    fn a_result_is_cut_where_the_app_recorded_its_labels() {
        let result = passage_of(HEADING, &format!("{TIDES} {BRIDGE}"));
        let marked = Marked {
            passages: result.passages.clone(),
            legend: Vec::new(),
        };
        let sources = sources_in("a", "Result of x", &result.text, 0, Some(&marked));
        assert_eq!(sources.len(), 1, "no text is left outside the passage");
        assert_eq!(sources[0].heading.0, HEADING);
        let texts: Vec<&str> = sources[0]
            .sentences
            .iter()
            .map(|sentence| sentence.text.0.as_str())
            .collect();
        assert_eq!(texts, [TIDES, BRIDGE]);
    }

    #[test]
    fn a_range_that_is_not_in_the_result_is_ignored_and_the_result_stands_whole() {
        let text = format!("{TIDES} {BRIDGE}");
        let marked = Marked {
            passages: vec![Passage {
                heading: HEADING.to_owned(),
                line: 0..5,
                body: 6..text.len() + 40,
                legend: Vec::new(),
            }],
            legend: Vec::new(),
        };
        let sources = sources_in("a", "Result of x", &text, 0, Some(&marked));
        assert_eq!(sources.len(), 1);
        assert_eq!(sources[0].heading.0, "Result of x");
        assert_eq!(sources[0].sentences.len(), 2);
    }

    #[test]
    fn results_that_were_condensed_lose_their_labels() {
        let mut labels = Labels::default();
        let result = passage_of(HEADING, TIDES);
        labels.keep("a", result.passages.clone(), Vec::new());
        labels.keep("b", result.passages, Vec::new());
        labels.forget(&HashSet::from(["a".to_owned()]));
        assert!(!labels.by_result.contains_key("a"));
        assert!(labels.by_result.contains_key("b"));
    }

    #[tokio::test]
    async fn prompt_snapshot_research_notes() {
        let model = ScriptedModel::new().on_request::<ResearchNotes>(|sent, _| {
            reply(scripted_answer(&prompt_of(sent), |_| true, cites))
        });
        let mut request = request_of(two_pages());
        condensed(&model, &mut request).await.expect("notes");
        let requests: Vec<_> = model.sent().into_iter().map(|sent| sent.request).collect();
        assert_requests(env!("CARGO_MANIFEST_DIR"), "research-notes", &requests);
    }
}
