//! The extraction round trip: ask, assemble, validate.
//!
//! Design: `docs/design/2026-09-19-conversation-memory.md` §6.2, §7.3.
//!
//! Everything the model returns is untrusted. This module does not decide
//! whether a quote is real, whether a source belongs to this conversation, or
//! whether a transition is ordered correctly — `domain::conversation::memory`
//! does. What lives here is the plumbing around that: what the model is shown
//! and points at, how its proposals become a patch, and which proposals the
//! domain's rules leave out. The model points at labelled sentences; the quote,
//! its message and its occurrence are the app's own.

use serde::{Deserialize, Serialize};
use tracing::{debug, warn};

use lattice_core::application::ports::llm_port::ThinkingMode;
use lattice_core::application::ports::LLMPort;
use lattice_core::application::services::model_contract::ask::{
    ask, Alone, Answered, Metered, Plain, Refusal, Says, Settled, Why,
};
use lattice_core::application::services::model_contract::shape::{
    Authored, Count, Fields, Form, Union, Value, Vocabulary,
};
use lattice_core::application::services::model_contract::shown::{
    Drawn, Internal, Kind, Picked, Scope, Showing, Shown, Told,
};
use lattice_core::application::services::model_contract::{Contract, ContractId};
use lattice_core::domain::conversation::memory::{
    validate_patch, MemoryId, MemoryItem, MemoryPatch, MemoryValidationError, ProposedEvidence,
    ProposedItem, ProposedTransition, SourceIndex, SourceMessage, ValidatedPatch,
    MAX_EVIDENCE_PER_ITEM, MAX_OPERATIONS_PER_RESPONSE, MEMORY_SCHEMA_VERSION,
};
use lattice_core::shared::error::Result;
use lattice_core::shared::text::sentences;

use super::prompts::{
    render_extractor_prompt, ContextPassage, ContextRows, ExistingRow, SegmentRows,
    EXTRACTOR_SYSTEM,
};
use super::segment::SourceSegment;
use super::{CompactionError, Deadline, EXTRACTOR_OUTPUT_TOKENS};

/// One accepted patch, with what it cost to get it.
#[derive(Debug, Clone)]
pub struct ExtractionOutcome {
    pub patch: ValidatedPatch,
    /// Model calls spent, including any the driver repeated.
    pub attempts: usize,
    /// True when more than one call was spent.
    pub repaired: bool,
}

pub(super) struct MemoryExtraction;

impl Contract for MemoryExtraction {
    const ID: ContractId = ContractId::new("memory-extraction");
    type Input = Input;
}

/// Where a sentence lies: its passage in the request, its place in that
/// passage and its byte range in the original message.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(super) struct Place {
    pub(super) message_id: String,
    pub(super) passage: usize,
    pub(super) index: usize,
    pub(super) start: usize,
    pub(super) end: usize,
}

/// One sentence of a passage, the thing a quotation points at.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct Sentence {
    pub(super) place: Internal<Place>,
    pub(super) text: Told<String>,
}

impl Kind for Sentence {
    const PREFIX: &'static str = "s";
    const DRAWN: Drawn = Drawn::Itself;
    type Ref = Place;

    fn reference(&self) -> Place {
        self.place.value().clone()
    }
}

/// A segment of the batch, as sentences.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct Segment {
    pub(super) sequence: Told<i64>,
    pub(super) role: Told<String>,
    pub(super) whole_message: Told<bool>,
    pub(super) context_before: Told<String>,
    pub(super) context_after: Told<String>,
    pub(super) sentences: Vec<Sentence>,
}

/// A message beside the batch, as sentences.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct Context {
    pub(super) sequence: Told<i64>,
    pub(super) role: Told<String>,
    pub(super) truncated: Told<bool>,
    pub(super) sentences: Vec<Sentence>,
}

/// A memory the conversation already holds.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct Memory {
    pub(super) id: Internal<MemoryId>,
    pub(super) kind: Told<String>,
    pub(super) label: Told<String>,
    pub(super) first_stated_at_sequence: Told<i64>,
    pub(super) last_changed_at_sequence: Told<i64>,
}

impl Kind for Memory {
    const PREFIX: &'static str = "m";
    const DRAWN: Drawn = Drawn::Itself;
    type Ref = MemoryId;

    fn reference(&self) -> MemoryId {
        self.id.value().clone()
    }
}

/// What one extraction call is shown.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct Input {
    pub(super) segments: Vec<Segment>,
    pub(super) context: Vec<Context>,
    pub(super) memories: Vec<Memory>,
    pub(super) messages: Internal<Vec<SourceMessage>>,
    pub(super) active: Internal<Vec<MemoryItem>>,
}

impl Input {
    pub(super) fn of(
        segments: &[SourceSegment],
        quotable: &[SourceMessage],
        context: &[ContextPassage<'_>],
        existing: &[MemoryItem],
    ) -> Self {
        let mut passage = 0;
        let segments = segments
            .iter()
            .map(|segment| {
                let sentences = sentences_of(
                    &segment.text,
                    &segment.message_id,
                    passage,
                    segment.start_byte,
                );
                passage += 1;
                Segment {
                    sequence: Told(segment.sequence),
                    role: Told(segment.role.as_str().to_owned()),
                    whole_message: Told(segment.whole_message),
                    context_before: Told(segment.leading_overlap.clone()),
                    context_after: Told(segment.trailing_overlap.clone()),
                    sentences,
                }
            })
            .collect();
        let context = context
            .iter()
            .map(|message| {
                let sentences = sentences_of(message.text, message.message_id, passage, 0);
                passage += 1;
                Context {
                    sequence: Told(message.sequence),
                    role: Told(message.role.to_owned()),
                    truncated: Told(message.truncated),
                    sentences,
                }
            })
            .collect();
        let memories = existing
            .iter()
            .map(|item| Memory {
                id: Internal::of(item.id.clone()),
                kind: Told(item.kind.as_str().to_owned()),
                label: Told(item.label.clone()),
                first_stated_at_sequence: Told(item.created_at_sequence),
                last_changed_at_sequence: Told(item.changed_at_sequence),
            })
            .collect();
        Self {
            segments,
            context,
            memories,
            messages: Internal::of(quotable.to_vec()),
            active: Internal::of(existing.to_vec()),
        }
    }
}

/// The sentences of `text`, which starts `base` bytes into its message.
fn sentences_of(text: &str, message_id: &str, passage: usize, base: usize) -> Vec<Sentence> {
    sentences(text)
        .into_iter()
        .enumerate()
        .filter_map(|(index, (start, end))| {
            let words = text.get(start..end)?;
            Some(Sentence {
                place: Internal::of(Place {
                    message_id: message_id.to_owned(),
                    passage,
                    index,
                    start: base + start,
                    end: base + end,
                }),
                text: Told(words.to_owned()),
            })
        })
        .collect()
}

/// The kinds of memory the model may propose.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Sort(&'static str);

impl Vocabulary for Sort {
    const WORDS: &'static [(&'static str, Self)] = &[
        ("constraint", Sort("constraint")),
        ("goal", Sort("goal")),
        ("decision", Sort("decision")),
        ("user_fact", Sort("user_fact")),
        ("preference", Sort("preference")),
        ("open_question", Sort("open_question")),
        ("established_fact", Sort("established_fact")),
    ];
}

/// What a quoted passage does for an addition.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Purpose(&'static str);

impl Vocabulary for Purpose {
    const WORDS: &'static [(&'static str, Self)] = &[
        ("assertion", Purpose("assertion")),
        ("antecedent", Purpose("antecedent")),
    ];
}

fn places(cited: Vec<Picked<Sentence>>) -> Vec<Place> {
    cited.into_iter().map(|picked| picked.reference).collect()
}

const ADDITIONS: Count = Count("add");
const TRANSITIONS: Count = Count("transitions");
const QUOTES: Count = Count("quotes");

/// Closed candidate handle (`c1`..`c32`) naming an addition inside one batch so
/// `transitions` can reference an earlier or replacement addition without
/// free-text ids or lookahead.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct CandidateCode(&'static str);

impl Vocabulary for CandidateCode {
    const WORDS: &'static [(&'static str, Self)] = &[
        ("c1", Self("c1")),
        ("c2", Self("c2")),
        ("c3", Self("c3")),
        ("c4", Self("c4")),
        ("c5", Self("c5")),
        ("c6", Self("c6")),
        ("c7", Self("c7")),
        ("c8", Self("c8")),
        ("c9", Self("c9")),
        ("c10", Self("c10")),
        ("c11", Self("c11")),
        ("c12", Self("c12")),
        ("c13", Self("c13")),
        ("c14", Self("c14")),
        ("c15", Self("c15")),
        ("c16", Self("c16")),
        ("c17", Self("c17")),
        ("c18", Self("c18")),
        ("c19", Self("c19")),
        ("c20", Self("c20")),
        ("c21", Self("c21")),
        ("c22", Self("c22")),
        ("c23", Self("c23")),
        ("c24", Self("c24")),
        ("c25", Self("c25")),
        ("c26", Self("c26")),
        ("c27", Self("c27")),
        ("c28", Self("c28")),
        ("c29", Self("c29")),
        ("c30", Self("c30")),
        ("c31", Self("c31")),
        ("c32", Self("c32")),
    ];
}

/// A quotation of an addition: its purpose, then the sentences it quotes.
/// Keys sort alphabetically in natural writing order (`purpose` -> `sentences`).
#[derive(Debug, Clone, PartialEq)]
pub(super) struct Quote {
    pub(super) purpose: Purpose,
    pub(super) places: Vec<Place>,
}

impl Authored for Quote {
    fn form() -> Form {
        Form::new()
            .word::<Purpose>("purpose")
            .cite_some::<Sentence>("sentences")
    }

    fn read(fields: &mut Fields<'_, '_>) -> Option<Self> {
        let purpose = fields.word::<Purpose>("purpose")?;
        let places = places(fields.cite_some::<Sentence>("sentences")?);
        Some(Self { purpose, places })
    }
}

/// A new memory the model proposes.
/// Keys sort alphabetically in natural writing order (`code` -> `kind` -> `label` -> `quotes`),
/// so grammar compilers that sort object properties alphabetically ask for the item's
/// kind and label before its cited sentences.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct Addition {
    pub(super) code: CandidateCode,
    pub(super) kind: Sort,
    pub(super) label: String,
    pub(super) quotes: Vec<Quote>,
}

impl Authored for Addition {
    fn form() -> Form {
        Form::new()
            .word::<CandidateCode>("code")
            .word::<Sort>("kind")
            .text("label")
            .counted::<Quote>("quotes", &QUOTES)
    }

    fn read(fields: &mut Fields<'_, '_>) -> Option<Self> {
        let code = fields.word::<CandidateCode>("code")?;
        let kind = fields.word::<Sort>("kind")?;
        let label = fields.text("label")?;
        let quotes = fields.list::<Quote>("quotes")?;
        Some(Self {
            code,
            kind,
            label,
            quotes,
        })
    }
}

/// One state transition proposed after `add` has been written.
/// Every branch's keys sort alphabetically after `kind` (`kind` -> `memory`/`older` ->
/// `replacement` -> `sentences`), so the discriminator is always generated first.
#[derive(Debug, Clone, PartialEq)]
pub(super) enum Transition {
    SupersedeExisting {
        memory: MemoryId,
        replacement: CandidateCode,
        sentences: Vec<Place>,
    },
    ResolveExisting {
        memory: MemoryId,
        sentences: Vec<Place>,
    },
    SupersedeCandidate {
        older: CandidateCode,
        replacement: CandidateCode,
        sentences: Vec<Place>,
    },
}

impl Union for Transition {
    fn forms() -> Vec<(&'static str, Form)> {
        vec![
            (
                "supersede_existing",
                Form::new()
                    .point::<Memory>("memory")
                    .word::<CandidateCode>("replacement")
                    .cite_some::<Sentence>("sentences"),
            ),
            (
                "resolve_existing",
                Form::new()
                    .point::<Memory>("memory")
                    .cite_some::<Sentence>("sentences"),
            ),
            (
                "supersede_candidate",
                Form::new()
                    .word::<CandidateCode>("older")
                    .word::<CandidateCode>("replacement")
                    .cite_some::<Sentence>("sentences"),
            ),
        ]
    }

    fn read(kind: &str, fields: &mut Fields<'_, '_>) -> Option<Self> {
        match kind {
            "supersede_existing" => {
                let memory = fields.point::<Memory>("memory")?.reference;
                let replacement = fields.word::<CandidateCode>("replacement")?;
                let sentences = places(fields.cite_some::<Sentence>("sentences")?);
                Some(Self::SupersedeExisting {
                    memory,
                    replacement,
                    sentences,
                })
            }
            "resolve_existing" => {
                let memory = fields.point::<Memory>("memory")?.reference;
                let sentences = places(fields.cite_some::<Sentence>("sentences")?);
                Some(Self::ResolveExisting { memory, sentences })
            }
            "supersede_candidate" => {
                let older = fields.word::<CandidateCode>("older")?;
                let replacement = fields.word::<CandidateCode>("replacement")?;
                let sentences = places(fields.cite_some::<Sentence>("sentences")?);
                Some(Self::SupersedeCandidate {
                    older,
                    replacement,
                    sentences,
                })
            }
            _ => None,
        }
    }
}

/// One transition entry in `transitions`.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct TransitionEntry {
    pub(super) transition: Transition,
}

impl Authored for TransitionEntry {
    fn form() -> Form {
        Form::new().one_of::<Transition>("transition")
    }

    fn read(fields: &mut Fields<'_, '_>) -> Option<Self> {
        Some(Self {
            transition: fields.one_of::<Transition>("transition")?,
        })
    }
}

/// One extraction answer: chronological additions first (`add`), then transitions (`transitions`).
#[derive(Debug, Clone, PartialEq)]
pub(super) struct Patch {
    pub(super) add: Vec<Addition>,
    pub(super) transitions: Vec<TransitionEntry>,
}

impl Authored for Patch {
    fn form() -> Form {
        Form::new()
            .counted::<Addition>("add", &ADDITIONS)
            .counted::<TransitionEntry>("transitions", &TRANSITIONS)
    }

    fn read(fields: &mut Fields<'_, '_>) -> Option<Self> {
        let add = fields.list::<Addition>("add")?;
        let transitions = fields.list::<TransitionEntry>("transitions")?;
        Some(Self { add, transitions })
    }
}

fn how_to_answer(has_existing: bool) -> &'static str {
    if has_existing {
        "Answer with an object holding add (new items in chronological order) and transitions (changes to earlier items). Each entry in add holds code (c1, c2, ...), kind, label, and quotes (each with purpose and non-empty sentences citing the labelled sentence handles shown in the passages). Each entry in transitions holds a transition object whose kind is supersede_existing (naming an existing item handle in memory, the new item's code in replacement, and the later user sentences in sentences), resolve_existing (naming memory and sentences), or supersede_candidate (naming an earlier add item's code in older, the later add item's code in replacement, and the later user sentences in sentences). Every transition's sentences must cite only the later user passage that justifies the change."
    } else {
        "Answer with an object holding add (new items in chronological order) and transitions (changes to earlier items in add). Each entry in add holds code (c1, c2, ...), kind, label, and quotes (each with purpose and non-empty sentences citing the labelled sentence handles shown in the passages). Because existing_items is empty, every entry in transitions must use kind supersede_candidate (naming an earlier add item's code in older, the later add item's code in replacement, and only the later user passage's sentences in sentences)."
    }
}

impl Answered for MemoryExtraction {
    type Answer = Value<Patch>;
    const ALONE: Alone = Alone::Pending;

    fn says(input: &Input, showing: &mut Showing<'_>) -> Result<Says> {
        showing.asks(&ADDITIONS, 0..=MAX_OPERATIONS_PER_RESPONSE);
        showing.asks(&TRANSITIONS, 0..=MAX_OPERATIONS_PER_RESPONSE);
        showing.asks(&QUOTES, 1..=MAX_EVIDENCE_PER_ITEM);
        let mut segments = Vec::with_capacity(input.segments.len());
        for segment in &input.segments {
            let shown = showing.show(&segment.sentences)?;
            segments.push(SegmentRows {
                sequence: segment.sequence.0,
                role: segment.role.0.as_str(),
                whole_message: segment.whole_message.0,
                rows: rows(&shown),
                context_before: segment.context_before.0.as_str(),
                context_after: segment.context_after.0.as_str(),
            });
        }
        let mut context = Vec::with_capacity(input.context.len());
        for message in &input.context {
            let shown = showing.show(&message.sentences)?;
            context.push(ContextRows {
                sequence: message.sequence.0,
                role: message.role.0.as_str(),
                rows: rows(&shown),
                truncated: message.truncated.0,
            });
        }
        let shown = showing.show(&input.memories)?;
        let ids: Vec<String> = shown.iter().map(|(handle, _)| handle.written()).collect();
        let existing: Vec<ExistingRow<'_>> = shown
            .iter()
            .zip(&ids)
            .map(|((_, memory), id)| ExistingRow {
                id: id.as_str(),
                kind: memory.kind.0.as_str(),
                label: memory.label.0.as_str(),
                first_stated_at_sequence: memory.first_stated_at_sequence.0,
                last_changed_at_sequence: memory.last_changed_at_sequence.0,
            })
            .collect();
        let answer_guide = how_to_answer(!existing.is_empty());
        let mut says = Says::new(
            format!("{EXTRACTOR_SYSTEM} {answer_guide}"),
            render_extractor_prompt(&segments, &context, &existing),
        );
        says.output_tokens = EXTRACTOR_OUTPUT_TOKENS;
        Ok(says)
    }

    fn accept(input: &Input, patch: &Patch) -> std::result::Result<(), Refusal> {
        let filtered = filter_patch(patch, input.messages.value(), input.active.value());
        if let Some(error) = filtered.transition_error {
            debug!(code = error.code(), "Memory extraction transition refused");
            return Err(Refusal(refusal_reason(&error)));
        }
        if filtered.add.is_empty() && filtered.transitions.is_empty() && filtered.refused > 0 {
            if let Some(error) = filtered.first_error {
                debug!(code = error.code(), "Memory extraction patch refused");
                return Err(Refusal(refusal_reason(&error)));
            }
            return Err(Refusal(
                "Leave invalid entries out, or give them evidence that follows the rules for their kind."
                    .into(),
            ));
        }
        Ok(())
    }
}

fn refusal_reason(error: &MemoryValidationError) -> String {
    match error {
        MemoryValidationError::NoEvidence(_) => {
            "Each item needs at least one quote citing the sentences that support it, or leave it out."
                .to_owned()
        }
        MemoryValidationError::TooMuchEvidence(_, _) => {
            format!("Give at most {MAX_EVIDENCE_PER_ITEM} quotes per item, or leave it out.")
        }
        MemoryValidationError::NoUserAssertion(_) | MemoryValidationError::NonUserAuthority(_) => {
            "Only established_fact may rest on an assistant assertion; every other kind needs at least one user sentence cited with purpose assertion, or leave it out."
                .to_owned()
        }
        MemoryValidationError::NoAssertion(_) => {
            "An established_fact needs at least one sentence cited with purpose assertion, or leave it out."
                .to_owned()
        }
        MemoryValidationError::TransitionNotLater(_) => {
            "A transition's sentences must cite a later user passage whose sequence is strictly after the earlier assertion."
                .to_owned()
        }
        MemoryValidationError::DuplicateCandidate(_) => {
            "Give each entry in add a unique code (c1, c2, c3, ...).".to_owned()
        }
        MemoryValidationError::UnknownTransitionTarget(_)
        | MemoryValidationError::UnknownReplacement(_, _) => {
            "A transition must name a listed existing item handle from existing_items or a code from add (c1, c2, ...)."
                .to_owned()
        }
        MemoryValidationError::QuoteTooLong(_, _) => {
            "Cite only the sentences that state the item; the combined quote exceeded the byte limit."
                .to_owned()
        }
        _ => "Leave invalid entries out, or give them evidence that follows the rules for their kind."
            .to_owned(),
    }
}

/// The sentences of a passage as rows: the label, then the text.
fn rows(shown: &Shown<'_, Sentence>) -> String {
    shown
        .iter()
        .map(|(handle, sentence)| {
            format!(
                "{} {}",
                handle.bracketed(),
                sentence.text.0.replace(['\r', '\n'], " ")
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Filtered additions and transitions that satisfy domain rules.
struct FilteredPatch {
    add: Vec<ProposedItem>,
    transitions: Vec<ProposedTransition>,
    refused: usize,
    first_error: Option<MemoryValidationError>,
    transition_error: Option<MemoryValidationError>,
}

fn to_proposed_item(addition: &Addition, messages: &[SourceMessage]) -> ProposedItem {
    ProposedItem {
        candidate_id: addition.code.0.to_owned(),
        kind: addition.kind.0.to_owned(),
        label: addition.label.clone(),
        evidence: addition
            .quotes
            .iter()
            .flat_map(|quote| evidence(&quote.places, quote.purpose.0, messages))
            .collect(),
    }
}

fn to_proposed_transition(
    transition: &Transition,
    messages: &[SourceMessage],
) -> ProposedTransition {
    match transition {
        Transition::SupersedeExisting {
            memory,
            replacement,
            sentences,
        } => ProposedTransition {
            item_id: memory.as_str().to_owned(),
            state: "superseded".to_owned(),
            replacement_candidate_id: Some(replacement.0.to_owned()),
            evidence: evidence(sentences, "transition", messages),
        },
        Transition::ResolveExisting { memory, sentences } => ProposedTransition {
            item_id: memory.as_str().to_owned(),
            state: "resolved".to_owned(),
            replacement_candidate_id: None,
            evidence: evidence(sentences, "transition", messages),
        },
        Transition::SupersedeCandidate {
            older,
            replacement,
            sentences,
        } => ProposedTransition {
            item_id: older.0.to_owned(),
            state: "superseded".to_owned(),
            replacement_candidate_id: Some(replacement.0.to_owned()),
            evidence: evidence(sentences, "transition", messages),
        },
    }
}

/// Keep every addition and transition that passes domain validation; leave out
/// any single entry that violates a domain rule so valid entries still stand.
fn filter_patch(
    patch: &Patch,
    quotable: &[SourceMessage],
    existing: &[MemoryItem],
) -> FilteredPatch {
    let mut add: Vec<ProposedItem> = Vec::new();
    let mut transitions: Vec<ProposedTransition> = Vec::new();
    let mut refused = 0_usize;
    let mut first_error: Option<MemoryValidationError> = None;
    let mut transition_error: Option<MemoryValidationError> = None;

    for addition in &patch.add {
        let item = to_proposed_item(addition, quotable);
        let mut candidate_add = add.clone();
        candidate_add.push(item);
        match check(
            candidate_add.clone(),
            Vec::new(),
            Vec::new(),
            quotable,
            existing,
        ) {
            Ok(_) => {
                add = candidate_add;
            }
            Err(error) => {
                refused += 1;
                if first_error.is_none() {
                    first_error = Some(error);
                }
            }
        }
    }

    for entry in &patch.transitions {
        let transition = to_proposed_transition(&entry.transition, quotable);
        let mut candidate_transitions = transitions.clone();
        candidate_transitions.push(transition);
        match check(
            add.clone(),
            candidate_transitions.clone(),
            Vec::new(),
            quotable,
            existing,
        ) {
            Ok(_) => {
                transitions = candidate_transitions;
            }
            Err(error) => {
                refused += 1;
                if transition_error.is_none()
                    && matches!(
                        error,
                        MemoryValidationError::UnknownTransitionTarget(_)
                            | MemoryValidationError::UnknownReplacement(_, _)
                            | MemoryValidationError::TransitionNotLater(_)
                    )
                {
                    transition_error = Some(error.clone());
                }
                if first_error.is_none() {
                    first_error = Some(error);
                }
            }
        }
    }

    FilteredPatch {
        add,
        transitions,
        refused,
        first_error,
        transition_error,
    }
}

/// A run of consecutive sentences of one passage: the span of the message it quotes.
struct Run<'a> {
    message_id: &'a str,
    passage: usize,
    last: usize,
    start: usize,
    end: usize,
}

/// The quotations the cited sentences make: each run of consecutive sentences is one quote.
fn evidence(cited: &[Place], purpose: &str, messages: &[SourceMessage]) -> Vec<ProposedEvidence> {
    let mut ordered: Vec<&Place> = cited.iter().collect();
    ordered.sort_by_key(|place| (place.passage, place.index));
    ordered.dedup_by_key(|place| (place.passage, place.index));
    let mut runs: Vec<Run<'_>> = Vec::new();
    for place in ordered {
        match runs.last_mut() {
            Some(run) if run.passage == place.passage && run.last + 1 == place.index => {
                run.last = place.index;
                run.end = place.end;
            }
            _ => runs.push(Run {
                message_id: place.message_id.as_str(),
                passage: place.passage,
                last: place.index,
                start: place.start,
                end: place.end,
            }),
        }
    }
    runs.into_iter()
        .filter_map(|run| {
            let message = messages
                .iter()
                .find(|message| message.id == run.message_id)?;
            let quote = message.content.get(run.start..run.end)?;
            Some(ProposedEvidence {
                message_id: run.message_id.to_owned(),
                quote: quote.to_owned(),
                occurrence: occurrence(&message.content, quote, run.start),
                purpose: purpose.to_owned(),
            })
        })
        .collect()
}

/// Which match of `quote` in `content` starts at `at`; `None` when it is the only one.
fn occurrence(content: &str, quote: &str, at: usize) -> Option<u32> {
    let mut before = 0u32;
    let mut total = 0u32;
    for (position, _) in content.char_indices() {
        if content
            .get(position..)
            .is_some_and(|rest| rest.starts_with(quote))
        {
            total += 1;
            if position < at {
                before += 1;
            }
        }
    }
    (total > 1).then_some(before)
}

/// `add` and `transitions` as one patch, checked by the domain's rules.
fn check(
    add: Vec<ProposedItem>,
    transitions: Vec<ProposedTransition>,
    segment_ids: Vec<String>,
    messages: &[SourceMessage],
    active: &[MemoryItem],
) -> std::result::Result<ValidatedPatch, MemoryValidationError> {
    let patch = MemoryPatch {
        schema_version: MEMORY_SCHEMA_VERSION,
        add,
        transitions,
        processed_segment_ids: segment_ids.clone(),
    };
    validate_patch(&patch, &SourceIndex::new(messages), active, &segment_ids)
}

const ALL_LEFT_OUT: &str = "every proposal of the answer was left out";

/// Why an unanswered extraction leaves the batch unprocessed.
fn unread(why: &Why) -> MemoryValidationError {
    MemoryValidationError::Malformed(
        match why {
            Why::CutOff => "the answer was cut off before it ended",
            Why::Declined => "the model declined to answer",
            Why::Refused(_) => ALL_LEFT_OUT,
            _ => "the answer could not be read",
        }
        .to_owned(),
    )
}

/// Extract memory from one batch of segments.
///
/// `quotable` is every message the batch is allowed to cite: the segments' own
/// messages plus the bounded adjacent context. An addition or transition the
/// domain's rules refuse is left out and the others stand.
///
/// # Errors
///
/// * [`CompactionError::Rejected`] when the answer did not end or could not be
///   read, or held proposals and none of them stands. The caller must not
///   advance the watermark past these segments (§6.2).
/// * [`CompactionError::DeadlineExceeded`] / [`CompactionError::Cancelled`]
///   when the job's shared deadline ran out or the caller cancelled.
/// * [`CompactionError::Model`] when the utility model itself failed.
pub async fn extract_segments(
    llm: &dyn LLMPort,
    deadline: &Deadline,
    segments: &[SourceSegment],
    quotable: &[SourceMessage],
    context: &[ContextPassage<'_>],
    existing: &[MemoryItem],
) -> std::result::Result<ExtractionOutcome, CompactionError> {
    let manifest = segment_ids(segments);
    let input = Input::of(segments, quotable, context, existing);
    let mut options = deadline.call_options()?;
    options.thinking = ThinkingMode::On;
    // Thinking models enter repetition loops under greedy decoding; let the
    // backend's thinking profile supply sampling while non-thinking backends
    // fall back to deterministic decoding instead of chat temperature.
    options.sampling = None;
    options.non_thinking_sampling =
        Some(lattice_core::application::ports::llm_port::SamplingOverride::deterministic());
    options.reasoning_effort = Some("low".to_owned());
    let transport = Metered::new(&Plain);
    let asked = ask::<MemoryExtraction>(
        llm,
        &transport,
        &mut Scope::for_question(),
        &input,
        &options,
    )
    .await;
    let attempts = usize::try_from(transport.used().sends).unwrap_or(usize::MAX);
    let settled = match asked {
        Ok(settled) => settled,
        Err(error) => {
            // A hard limit stops the job; anything else is the model failing.
            deadline.check()?;
            return Err(CompactionError::Model(error.to_string()));
        }
    };
    let raw_patch = match settled {
        Settled::Answered(patch) => patch,
        Settled::Unanswered(why) => {
            let error = unread(&why);
            warn!(
                code = error.code(),
                segments = segments.len(),
                "Memory extraction answer did not settle — these segments stay unprocessed"
            );
            return Err(CompactionError::Rejected(error));
        }
    };

    let filtered = filter_patch(&raw_patch, quotable, existing);
    if filtered.refused > 0 {
        warn!(
            refused = filtered.refused,
            segments = segments.len(),
            "Memory extraction left out invalid proposal(s)"
        );
    }
    if filtered.add.is_empty() && filtered.transitions.is_empty() && filtered.refused > 0 {
        let error = MemoryValidationError::Malformed(ALL_LEFT_OUT.to_owned());
        warn!(
            code = error.code(),
            segments = segments.len(),
            "Every proposal of the answer was left out — these segments stay unprocessed"
        );
        return Err(CompactionError::Rejected(error));
    }
    let patch = check(
        filtered.add,
        filtered.transitions,
        manifest,
        quotable,
        existing,
    )
    .map_err(|error| {
        warn!(
            code = error.code(),
            segments = segments.len(),
            "Memory extraction patch rejected — these segments stay unprocessed"
        );
        CompactionError::Rejected(error)
    })?;
    debug!(
        segments = segments.len(),
        additions = patch.additions.len(),
        transitions = patch.transitions.len(),
        attempts,
        "Memory extraction accepted"
    );
    Ok(ExtractionOutcome {
        patch,
        attempts,
        repaired: attempts > 1,
    })
}

/// Segment ids in a batch, for reporting which source a rejection left behind.
pub fn segment_ids(segments: &[SourceSegment]) -> Vec<String> {
    segments.iter().map(|segment| segment.id.clone()).collect()
}
