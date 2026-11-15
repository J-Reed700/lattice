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

use lattice_core::application::ports::LLMPort;
use lattice_core::application::services::model_contract::ask::{
    ask_new, Alone, Answered, Metered, Plain, Refusal, Says, Why,
};
use lattice_core::application::services::model_contract::shape::{
    Authored, Fields, Form, Found, New, Union, Vocabulary,
};
use lattice_core::application::services::model_contract::shown::{
    Drawn, Internal, Kind, Picked, Scope, Showing, Shown, Told,
};
use lattice_core::application::services::model_contract::{Contract, ContractId};
use lattice_core::domain::conversation::memory::{
    validate_patch, MemoryId, MemoryItem, MemoryPatch, MemoryValidationError, ProposedEvidence,
    ProposedItem, ProposedTransition, SourceIndex, SourceMessage, ValidatedPatch,
    MEMORY_SCHEMA_VERSION,
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

/// A quotation of an addition: the sentences it quotes and what they do.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct Quote {
    pub(super) purpose: Purpose,
    pub(super) places: Vec<Place>,
}

impl Authored for Quote {
    fn form() -> Form {
        Form::new()
            .word::<Purpose>("purpose")
            .cite::<Sentence>("sentences")
    }

    fn read(fields: &mut Fields<'_, '_>) -> Option<Self> {
        let purpose = fields.word::<Purpose>("purpose")?;
        let places = places(fields.cite::<Sentence>("sentences"));
        Some(Self { purpose, places })
    }
}

/// The sentences that justify a change.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct Grounds {
    pub(super) places: Vec<Place>,
}

impl Authored for Grounds {
    fn form() -> Form {
        Form::new().cite::<Sentence>("sentences")
    }

    fn read(fields: &mut Fields<'_, '_>) -> Option<Self> {
        Some(Self {
            places: places(fields.cite::<Sentence>("sentences")),
        })
    }
}

/// A new memory the model proposes.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct Addition {
    pub(super) evidence: Vec<Quote>,
    pub(super) kind: Sort,
    pub(super) label: String,
}

impl Authored for Addition {
    fn form() -> Form {
        Form::new()
            .list::<Quote>("evidence")
            .word::<Sort>("kind")
            .text("label")
    }

    fn read(fields: &mut Fields<'_, '_>) -> Option<Self> {
        let evidence = fields.list::<Quote>("evidence")?;
        let kind = fields.word::<Sort>("kind")?;
        let label = fields.text("label")?;
        Some(Self {
            evidence,
            kind,
            label,
        })
    }
}

/// One change the model proposes.
#[derive(Debug, Clone, PartialEq)]
pub(super) enum Proposal {
    Add(Addition),
    Supersede {
        grounds: Vec<Grounds>,
        item: MemoryId,
        replacement: Addition,
    },
    Resolve {
        grounds: Vec<Grounds>,
        item: MemoryId,
    },
    Correct {
        grounds: Vec<Grounds>,
        earlier: Addition,
        later: Addition,
    },
}

impl Union for Proposal {
    fn forms() -> Vec<(&'static str, Form)> {
        vec![
            ("add", Form::new().group::<Addition>("addition")),
            (
                "supersede",
                Form::new()
                    .list::<Grounds>("evidence")
                    .point::<Memory>("item_id")
                    .group::<Addition>("replacement"),
            ),
            (
                "resolve",
                Form::new()
                    .list::<Grounds>("evidence")
                    .point::<Memory>("item_id"),
            ),
            (
                "correct",
                Form::new()
                    .list::<Grounds>("evidence")
                    .group::<Addition>("earlier")
                    .group::<Addition>("later"),
            ),
        ]
    }

    fn read(kind: &str, fields: &mut Fields<'_, '_>) -> Option<Self> {
        match kind {
            "add" => Some(Self::Add(fields.group::<Addition>("addition")?)),
            "supersede" => {
                let grounds = fields.list::<Grounds>("evidence")?;
                let item = fields.point::<Memory>("item_id")?.reference;
                let replacement = fields.group::<Addition>("replacement")?;
                Some(Self::Supersede {
                    grounds,
                    item,
                    replacement,
                })
            }
            "resolve" => {
                let grounds = fields.list::<Grounds>("evidence")?;
                let item = fields.point::<Memory>("item_id")?.reference;
                Some(Self::Resolve { grounds, item })
            }
            "correct" => {
                let grounds = fields.list::<Grounds>("evidence")?;
                let earlier = fields.group::<Addition>("earlier")?;
                let later = fields.group::<Addition>("later")?;
                Some(Self::Correct {
                    grounds,
                    earlier,
                    later,
                })
            }
            _ => None,
        }
    }
}

/// One item of the answer.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct Entry {
    pub(super) proposal: Proposal,
}

impl Authored for Entry {
    fn form() -> Form {
        Form::new().one_of::<Proposal>("proposal")
    }

    fn read(fields: &mut Fields<'_, '_>) -> Option<Self> {
        Some(Self {
            proposal: fields.one_of::<Proposal>("proposal")?,
        })
    }
}

const HOW_TO_ANSWER: &str = "Answer with an items list holding one item for each change you propose. Each item holds a proposal whose kind is add, supersede, resolve or correct. An add proposal holds one addition. A supersede proposal names an existing item in item_id and holds the complete replacement for it. A resolve proposal names an existing item in item_id. A correct proposal holds the earlier addition and the later addition that corrects it. What the instructions call an addition or add is an add proposal, or the replacement or the two additions inside a supersede or correct proposal. An addition holds its evidence, then its kind and its label; each piece of evidence cites in sentences the labelled sentences it quotes and gives their purpose. A supersede, resolve or correct proposal also holds evidence: the labelled sentences of the later user passage that justify the change.";

impl Answered for MemoryExtraction {
    type Answer = New<Entry>;
    const ALONE: Alone = Alone::Pending;

    fn says(input: &Input, showing: &mut Showing<'_>) -> Result<Says> {
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
        let mut says = Says::new(
            format!("{EXTRACTOR_SYSTEM} {HOW_TO_ANSWER}"),
            render_extractor_prompt(&segments, &context, &existing),
        );
        says.output_tokens = EXTRACTOR_OUTPUT_TOKENS;
        Ok(says)
    }

    fn accept(input: &Input, entry: &Entry) -> std::result::Result<(), Refusal> {
        let part = Fragment::of(&entry.proposal, 0, input.messages.value());
        match check(
            part.add,
            part.transitions,
            Vec::new(),
            input.messages.value(),
            input.active.value(),
        ) {
            Ok(_) => Ok(()),
            Err(error) => {
                debug!(code = error.code(), "Memory proposal refused");
                Err(Refusal(
                    "Leave this item out, or give it evidence that follows the rules for its kind."
                        .into(),
                ))
            }
        }
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

/// The additions and transitions of one or more proposals, in the domain's own form.
#[derive(Debug, Clone, Default)]
struct Fragment {
    add: Vec<ProposedItem>,
    transitions: Vec<ProposedTransition>,
}

impl Fragment {
    /// The patch parts of `proposal`; its additions are named after the `first` already minted.
    fn of(proposal: &Proposal, first: usize, messages: &[SourceMessage]) -> Self {
        let candidate = |offset: usize| format!("c{}", first + offset + 1);
        let item = |addition: &Addition, id: String| ProposedItem {
            candidate_id: id,
            kind: addition.kind.0.to_owned(),
            label: addition.label.clone(),
            evidence: addition
                .evidence
                .iter()
                .flat_map(|quote| evidence(&quote.places, quote.purpose.0, messages))
                .collect(),
        };
        let grounds = |grounds: &[Grounds]| -> Vec<ProposedEvidence> {
            grounds
                .iter()
                .flat_map(|grounds| evidence(&grounds.places, "transition", messages))
                .collect()
        };
        match proposal {
            Proposal::Add(addition) => Self {
                add: vec![item(addition, candidate(0))],
                transitions: Vec::new(),
            },
            Proposal::Supersede {
                grounds: because,
                item: target,
                replacement,
            } => Self {
                add: vec![item(replacement, candidate(0))],
                transitions: vec![ProposedTransition {
                    item_id: target.as_str().to_owned(),
                    state: "superseded".to_owned(),
                    replacement_candidate_id: Some(candidate(0)),
                    evidence: grounds(because),
                }],
            },
            Proposal::Resolve {
                grounds: because,
                item: target,
            } => Self {
                add: Vec::new(),
                transitions: vec![ProposedTransition {
                    item_id: target.as_str().to_owned(),
                    state: "resolved".to_owned(),
                    replacement_candidate_id: None,
                    evidence: grounds(because),
                }],
            },
            Proposal::Correct {
                grounds: because,
                earlier,
                later,
            } => Self {
                add: vec![item(earlier, candidate(0)), item(later, candidate(1))],
                transitions: vec![ProposedTransition {
                    item_id: candidate(0),
                    state: "superseded".to_owned(),
                    replacement_candidate_id: Some(candidate(1)),
                    evidence: grounds(because),
                }],
            },
        }
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

/// Why an answer that did not end leaves the batch unprocessed.
fn unread(why: &Why) -> MemoryValidationError {
    MemoryValidationError::Malformed(
        match why {
            Why::CutOff => "the answer was cut off before it ended",
            Why::Declined => "the model declined to answer",
            _ => "the answer could not be read",
        }
        .to_owned(),
    )
}

/// Extract memory from one batch of segments.
///
/// `quotable` is every message the batch is allowed to cite: the segments' own
/// messages plus the bounded adjacent context. A proposal the domain's rules
/// refuse is left out and the others stand.
///
/// # Errors
///
/// * [`CompactionError::Rejected`] when the answer did not end or could not be
///   read. The caller must not advance the watermark past these segments (§6.2).
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
    let options = deadline.call_options()?;
    let transport = Metered::new(&Plain);
    let asked = ask_new::<MemoryExtraction>(
        llm,
        &transport,
        &mut Scope::for_question(),
        input,
        |_| None,
        &options,
    )
    .await;
    let attempts = usize::try_from(transport.used().sends).unwrap_or(usize::MAX);
    let Found { items, rest } = match asked {
        Ok(found) => found,
        Err(error) => {
            // A hard limit stops the job; anything else is the model failing.
            deadline.check()?;
            return Err(CompactionError::Model(error.to_string()));
        }
    };
    if let Some(why) = rest {
        let error = unread(&why);
        warn!(
            code = error.code(),
            segments = segments.len(),
            "Memory extraction answer did not end — these segments stay unprocessed"
        );
        return Err(CompactionError::Rejected(error));
    }

    let mut add: Vec<ProposedItem> = Vec::new();
    let mut transitions: Vec<ProposedTransition> = Vec::new();
    for entry in &items {
        let part = Fragment::of(&entry.proposal, add.len(), quotable);
        let mut with_add = add.clone();
        with_add.extend(part.add);
        let mut with_transitions = transitions.clone();
        with_transitions.extend(part.transitions);
        match check(
            with_add.clone(),
            with_transitions.clone(),
            Vec::new(),
            quotable,
            existing,
        ) {
            Ok(_) => {
                add = with_add;
                transitions = with_transitions;
            }
            Err(error) => warn!(
                code = error.code(),
                segments = segments.len(),
                "Memory extraction proposal refused — left out"
            ),
        }
    }
    let patch = check(add, transitions, manifest, quotable, existing).map_err(|error| {
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
