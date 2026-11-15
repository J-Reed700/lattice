//! Semantic review of a validated patch (design:
//! `docs/design/2026-09-19-conversation-memory.md` §6.3). Validation proves a
//! quote is real, not whose instruction it is or what it is about, so every
//! new item and proposed transition gets a structured look at the original
//! passages. When review cannot run, or leaves a change unanswered, the
//! verdict is [`SemanticVerdict::Ambiguous`], never `Supported`.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tracing::warn;

use lattice_core::application::ports::LLMPort;
use lattice_core::application::services::model_contract::ask::{
    ask_each, Alone, Answered, Metered, Plain, Refusal, Says, Settled,
};
use lattice_core::application::services::model_contract::shape::{
    Authored, Each, Fields, Form, Vocabulary,
};
use lattice_core::application::services::model_contract::shown::{
    Drawn, Internal, Kind, Scope, Showing, Told,
};
use lattice_core::application::services::model_contract::{Contract, ContractId};
use lattice_core::domain::conversation::memory::{
    EvidenceSpan, MemoryItem, SemanticVerdict, SourceMessage, ValidatedPatch,
    ValidatedTransitionTarget,
};
use lattice_core::shared::error::Result;

use super::prompts::{
    render_verifier_prompt, ContextPassage, ReviewPassage, ReviewRequest, VERIFIER_SYSTEM,
};
use super::segment::clip_chars;
use super::{CompactionError, Deadline};

/// Proposals per review call. Small batches on purpose: a reviewer asked about
/// a dozen unrelated changes at once starts leaving changes out.
pub const MAX_REVIEWS_PER_CALL: usize = 6;

/// Verdicts for one patch, keyed the way [`apply_patch`] wants them.
///
/// [`apply_patch`]: lattice_core::domain::conversation::memory::apply_patch
#[derive(Debug, Clone, Default)]
pub struct VerificationOutcome {
    /// Keyed by `candidate_id`. Every addition appears: optional facts,
    /// preferences, goals, and questions can still mislead later turns if they
    /// bypass semantic review.
    pub addition_verdicts: HashMap<String, SemanticVerdict>,
    /// Keyed by existing item id. Every transition appears.
    pub transition_verdicts: HashMap<String, SemanticVerdict>,
    pub calls: usize,
    /// True when the reviewer could not be consulted and its verdicts were
    /// filled in as `Ambiguous`.
    pub degraded: bool,
}

/// Exact source text for the spans a review needs to read.
///
/// An old item's evidence can point at a message far outside this attempt's
/// window, so the caller resolves those through the port and hands them over
/// here. A span that no longer resolves is stored as absent and rendered as
/// absent — never as a remembered copy of the quotation (invariant 15).
#[derive(Debug, Clone, Default)]
pub struct EvidenceText {
    resolved: HashMap<String, String>,
}

impl EvidenceText {
    pub fn new() -> Self {
        Self::default()
    }

    /// Resolve every span that reads from one of `messages`.
    pub fn learn_from<'a>(
        &mut self,
        messages: &[SourceMessage],
        spans: impl Iterator<Item = &'a EvidenceSpan>,
    ) {
        let by_id: HashMap<&str, &SourceMessage> = messages
            .iter()
            .map(|message| (message.id.as_str(), message))
            .collect();
        for span in spans {
            if self.resolved.contains_key(&span_key(span)) {
                continue;
            }
            if let Some(text) = by_id
                .get(span.message_id.as_str())
                .and_then(|message| span.resolve(&message.content))
            {
                self.resolved.insert(span_key(span), text.to_string());
            }
        }
    }

    /// Record text the caller resolved elsewhere, e.g. through the port.
    pub fn insert(&mut self, span: &EvidenceSpan, text: impl Into<String>) {
        self.resolved.insert(span_key(span), text.into());
    }

    pub fn text(&self, span: &EvidenceSpan) -> Option<&str> {
        self.resolved.get(&span_key(span)).map(String::as_str)
    }
}

/// Stable key for one span. Offsets are part of it: two spans in the same
/// message are different evidence.
pub fn span_key(span: &EvidenceSpan) -> String {
    format!("{}:{}:{}", span.message_id, span.start_byte, span.end_byte)
}

/// Review every new addition and every proposed transition.
///
/// Returns immediately, with no model call, when the patch proposes neither.
///
/// # Errors
///
/// Only the job's own limits propagate: [`CompactionError::DeadlineExceeded`]
/// and [`CompactionError::Cancelled`]. A model or parse failure degrades to
/// `Ambiguous` verdicts instead, because an unreviewed change is uncertain, not
/// fatal.
pub async fn review_patch(
    llm: &dyn LLMPort,
    deadline: &Deadline,
    patch: &ValidatedPatch,
    active_items: &[MemoryItem],
    window: &[SourceMessage],
    evidence: &EvidenceText,
) -> std::result::Result<VerificationOutcome, CompactionError> {
    let mut outcome = VerificationOutcome::default();

    let additions: Vec<&lattice_core::domain::conversation::memory::ValidatedAddition> =
        patch.additions.iter().collect();
    if additions.is_empty() && patch.transitions.is_empty() {
        return Ok(outcome);
    }

    // Conservative defaults first, so an early return from any later failure
    // still carries a verdict for every id.
    for addition in &additions {
        outcome
            .addition_verdicts
            .insert(addition.candidate_id.clone(), SemanticVerdict::Ambiguous);
    }
    for transition in &patch.transitions {
        outcome.transition_verdicts.insert(
            transition.target.as_str().to_string(),
            SemanticVerdict::Ambiguous,
        );
    }

    let by_id: HashMap<&str, &MemoryItem> = active_items
        .iter()
        .map(|item| (item.id.as_str(), item))
        .collect();
    let messages: HashMap<&str, &SourceMessage> = window
        .iter()
        .map(|message| (message.id.as_str(), message))
        .collect();

    let mut requests: Vec<Review> = Vec::new();
    for addition in &additions {
        requests.push(Review {
            subject: Subject::Addition(addition.candidate_id.clone()),
            change: Told("add_mandatory_item".to_owned()),
            kind: Told(addition.kind.as_str().to_owned()),
            label: Some(Told(addition.label.clone())),
            proposed: passages(&addition.evidence, evidence),
            existing: Vec::new(),
            existing_label: None,
            context: surrounding(&addition.evidence, &[], &messages),
        });
    }
    for transition in &patch.transitions {
        let candidate = match &transition.target {
            ValidatedTransitionTarget::Candidate(id) => patch
                .additions
                .iter()
                .find(|addition| addition.candidate_id == *id),
            ValidatedTransitionTarget::Existing(_) => None,
        };
        let existing = match &transition.target {
            ValidatedTransitionTarget::Existing(id) => by_id.get(id.as_str()).copied(),
            ValidatedTransitionTarget::Candidate(_) => None,
        };
        let replacement = transition
            .replacement_candidate_id
            .as_deref()
            .and_then(|id| {
                patch
                    .additions
                    .iter()
                    .find(|addition| addition.candidate_id == id)
            });
        let existing_spans = existing
            .map(|item| item.evidence.clone())
            .or_else(|| candidate.map(|item| item.evidence.clone()))
            .unwrap_or_default();
        requests.push(Review {
            subject: Subject::Transition(transition.target.as_str().to_string()),
            change: Told(
                match transition.state {
                    lattice_core::domain::conversation::memory::MemoryState::Resolved => {
                        "resolve_item"
                    }
                    _ => "supersede_item",
                }
                .to_owned(),
            ),
            kind: Told(
                existing
                    .map(|item| item.kind.as_str())
                    .or_else(|| candidate.map(|item| item.kind.as_str()))
                    .unwrap_or("constraint")
                    .to_owned(),
            ),
            label: replacement.map(|addition| Told(addition.label.clone())),
            proposed: passages(&transition.evidence, evidence),
            existing: passages(&existing_spans, evidence),
            existing_label: existing
                .map(|item| Told(item.label.clone()))
                .or_else(|| candidate.map(|item| Told(item.label.clone()))),
            context: surrounding(&transition.evidence, &existing_spans, &messages),
        });
    }

    let chars_per_token = llm.chars_per_token();
    for batch in requests.chunks(MAX_REVIEWS_PER_CALL) {
        let options = deadline.call_options()?;
        let transport = Metered::new(&Plain);
        let asked = ask_each::<MemoryReview>(
            llm,
            &transport,
            &mut Scope::for_question(),
            batch,
            |open, _| Input::of(open, chars_per_token),
            &options,
        )
        .await;
        outcome.calls += usize::try_from(transport.used().sends).unwrap_or(usize::MAX);
        let settled = match asked {
            Ok(settled) => settled,
            Err(error) => {
                // A hard limit stops the job. A provider failure does not: the
                // reviews keep their `Ambiguous` defaults and the pass continues.
                deadline.check()?;
                warn!(
                    reviews = batch.len(),
                    code = CompactionError::Model(error.to_string()).code(),
                    "Memory review call failed — those changes stay ambiguous"
                );
                outcome.degraded = true;
                continue;
            }
        };
        let mut left = 0;
        for (review, settled) in batch.iter().zip(settled) {
            match settled {
                Settled::Answered(reading) => {
                    let verdict = SemanticVerdict::from(reading.verdict);
                    match &review.subject {
                        Subject::Addition(id) => {
                            outcome.addition_verdicts.insert(id.clone(), verdict);
                        }
                        Subject::Transition(id) => {
                            outcome.transition_verdicts.insert(id.clone(), verdict);
                        }
                    }
                }
                Settled::Unanswered(_) => left += 1,
            }
        }
        if left > 0 {
            warn!(
                reviews = batch.len(),
                left, "Memory review left changes unread — they stay ambiguous"
            );
            outcome.degraded = true;
        }
    }

    Ok(outcome)
}

/// The reviewer's call: one verdict for each change.
pub(super) struct MemoryReview;

impl Contract for MemoryReview {
    const ID: ContractId = ContractId::new("memory-review");
    type Input = Input;
}

/// What names a change to the app: the candidate it adds, or the item it retires.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(super) enum Subject {
    Addition(String),
    Transition(String),
}

/// One quoted span offered to the reviewer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct Passage {
    pub(super) message_id: Told<String>,
    pub(super) sequence: Told<i64>,
    pub(super) role: Told<String>,
    pub(super) text: Option<Told<String>>,
}

/// An original message beside the quotes of a review; `quoted` is the byte range they cover.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct Excerpt {
    pub(super) message_id: Told<String>,
    pub(super) sequence: Told<i64>,
    pub(super) role: Told<String>,
    pub(super) text: Told<String>,
    pub(super) quoted: Internal<Option<(usize, usize)>>,
}

/// One change the reviewer must judge.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct Review {
    pub(super) subject: Subject,
    pub(super) change: Told<String>,
    pub(super) kind: Told<String>,
    pub(super) label: Option<Told<String>>,
    pub(super) proposed: Vec<Passage>,
    pub(super) existing: Vec<Passage>,
    pub(super) existing_label: Option<Told<String>>,
    pub(super) context: Vec<Excerpt>,
}

impl Kind for Review {
    const PREFIX: &'static str = "r";
    const DRAWN: Drawn = Drawn::Itself;
    type Ref = Subject;

    fn reference(&self) -> Subject {
        self.subject.clone()
    }
}

/// How much of each surrounding message a request shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(super) enum Share {
    Whole,
    Omitted,
    Chars(usize),
}

/// The changes one review call judges.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct Input {
    pub(super) reviews: Vec<Review>,
    pub(super) share: Internal<Share>,
    pub(super) chars_per_token: Internal<f64>,
}

impl Input {
    pub(super) fn of(open: &[&Review], chars_per_token: f64) -> Self {
        Self {
            reviews: open.iter().map(|review| (*review).clone()).collect(),
            share: Internal::of(Share::Whole),
            chars_per_token: Internal::of(chars_per_token),
        }
    }

    /// The input whose surrounding text holds what `room` tokens hold, shared between its messages.
    fn cut_to(&self, room: usize) -> Self {
        let share = if room == 0 {
            Share::Omitted
        } else {
            let messages: usize = self.reviews.iter().map(|review| review.context.len()).sum();
            let chars = lattice_core::application::ports::llm_port::chars_within_tokens(
                room,
                *self.chars_per_token.value(),
            );
            Share::Chars(chars / messages.max(1))
        };
        Self {
            share: Internal::of(share),
            ..self.clone()
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Judgement {
    Supported,
    Ambiguous,
    Unsupported,
}

impl Vocabulary for Judgement {
    const WORDS: &'static [(&'static str, Self)] = &[
        ("supported", Judgement::Supported),
        ("ambiguous", Judgement::Ambiguous),
        ("unsupported", Judgement::Unsupported),
    ];
}

impl From<Judgement> for SemanticVerdict {
    fn from(judgement: Judgement) -> Self {
        match judgement {
            Judgement::Supported => Self::Supported,
            Judgement::Ambiguous => Self::Ambiguous,
            Judgement::Unsupported => Self::Unsupported,
        }
    }
}

/// What the reviewer says of one change.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct Reading {
    pub(super) reason: String,
    pub(super) verdict: Judgement,
}

impl Authored for Reading {
    fn form() -> Form {
        Form::new().text("reason").word::<Judgement>("verdict")
    }

    fn read(fields: &mut Fields<'_, '_>) -> Option<Self> {
        let reason = fields.text("reason")?;
        let verdict = fields.word::<Judgement>("verdict")?;
        Some(Self { reason, verdict })
    }
}

const HOW_TO_ANSWER: &str = "Answer with an entries list holding one entry for each review, naming the review in of by its id, and giving its reason and then its verdict.";

impl Answered for MemoryReview {
    type Answer = Each<Review, Reading>;
    const ALONE: Alone = Alone::Pending;

    fn says(input: &Input, showing: &mut Showing<'_>) -> Result<Says> {
        let shown = showing.show(&input.reviews)?;
        let ids: Vec<String> = shown.iter().map(|(handle, _)| handle.written()).collect();
        let requests: Vec<ReviewRequest<'_>> = shown
            .iter()
            .zip(&ids)
            .map(|((_, review), id)| ReviewRequest {
                id: id.as_str(),
                change: review.change.0.as_str(),
                kind: review.kind.0.as_str(),
                label: review.label.as_ref().map(|label| label.0.as_str()),
                proposed_evidence: shown_passages(&review.proposed),
                existing_evidence: shown_passages(&review.existing),
                existing_label: review.existing_label.as_ref().map(|label| label.0.as_str()),
                surrounding: shown_context(&review.context, *input.share.value()),
            })
            .collect();
        let mut says = Says::new(
            format!("{VERIFIER_SYSTEM} {HOW_TO_ANSWER}"),
            render_verifier_prompt(&requests),
        );
        says.output_tokens = super::VERIFIER_OUTPUT_TOKENS;
        Ok(says)
    }

    fn accept(_input: &Input, reading: &Reading) -> std::result::Result<(), Refusal> {
        if gives_a_reason(&reading.reason) {
            Ok(())
        } else {
            Err(Refusal(
                "Give this review a reason of one short sentence.".into(),
            ))
        }
    }

    fn within(input: &Input, room: usize) -> Option<Input> {
        Some(input.cut_to(room))
    }
}

fn passages(spans: &[EvidenceSpan], evidence: &EvidenceText) -> Vec<Passage> {
    spans
        .iter()
        .map(|span| Passage {
            message_id: Told(span.message_id.clone()),
            sequence: Told(span.sequence),
            role: Told(span.role.as_str().to_owned()),
            text: evidence.text(span).map(|text| Told(text.to_owned())),
        })
        .collect()
}

fn shown_passages(passages: &[Passage]) -> Vec<ReviewPassage<'_>> {
    passages
        .iter()
        .map(|passage| ReviewPassage {
            message_id: passage.message_id.0.as_str(),
            sequence: passage.sequence.0,
            role: passage.role.0.as_str(),
            text: passage.text.as_ref().map(|text| text.0.as_str()),
        })
        .collect()
}

/// Each message whole when `share` holds it, centred on its quotes and flagged when not.
fn shown_context(context: &[Excerpt], share: Share) -> Vec<ContextPassage<'_>> {
    let max_chars = match share {
        Share::Omitted => return Vec::new(),
        Share::Whole => usize::MAX,
        Share::Chars(chars) => chars,
    };
    context
        .iter()
        .map(|excerpt| {
            let text = excerpt.text.0.as_str();
            let (text, truncated) = match excerpt.quoted.value() {
                Some((start, end)) => clip_around(text, *start, *end, max_chars),
                None => clip_chars(text, max_chars),
            };
            ContextPassage {
                message_id: excerpt.message_id.0.as_str(),
                sequence: excerpt.sequence.0,
                role: excerpt.role.0.as_str(),
                text,
                truncated,
            }
        })
        .collect()
}

/// The original messages the spans sit in, once each, in sequence order.
///
/// §6.3 requires the reviewer see surrounding context rather than the extracted
/// span alone: a sentence that reverses the one before it is invisible from the
/// span on its own.
fn surrounding<'a>(
    proposed: &[EvidenceSpan],
    existing: &[EvidenceSpan],
    messages: &HashMap<&'a str, &'a SourceMessage>,
) -> Vec<Excerpt> {
    let spans: Vec<&EvidenceSpan> = proposed.iter().chain(existing).collect();
    let mut out: Vec<Excerpt> = Vec::new();
    let mut current_ids: Vec<&str> = Vec::new();
    for span in &spans {
        let Some(message) = messages.get(span.message_id.as_str()) else {
            continue;
        };
        if current_ids.contains(&message.id.as_str()) {
            continue;
        }
        current_ids.push(message.id.as_str());

        // A prefix is actively misleading for a long message when the quoted
        // rule sits near the end: the reviewer sees unrelated opening prose and
        // may claim the exact proposed evidence is absent. Centre the excerpt
        // on every span from this message, so the quote and its local qualifiers
        // are always visible.
        let in_message: Vec<&EvidenceSpan> = spans
            .iter()
            .copied()
            .filter(|candidate| candidate.message_id == message.id)
            .collect();
        let start = in_message
            .iter()
            .map(|candidate| candidate.start_byte as usize)
            .min()
            .unwrap_or_default();
        let end = in_message
            .iter()
            .map(|candidate| candidate.end_byte as usize)
            .max()
            .unwrap_or(message.content.len());
        out.push(Excerpt {
            message_id: Told(message.id.clone()),
            sequence: Told(message.sequence),
            role: Told(message.role.as_str().to_owned()),
            text: Told(message.content.clone()),
            quoted: Internal::of(Some((start, end))),
        });
    }

    // Immediate neighbours disambiguate short replies and let the reviewer see
    // that a one-shot request was already completed by the following assistant
    // passage. They are context only and never become evidence.
    let mut ordered: Vec<&SourceMessage> = messages.values().copied().collect();
    ordered.sort_by_key(|message| message.sequence);
    let mut adjacent_seen: Vec<&str> = current_ids.clone();
    for current_id in current_ids {
        let Some(index) = ordered.iter().position(|message| message.id == current_id) else {
            continue;
        };
        for neighbour in [index.checked_sub(1), index.checked_add(1)]
            .into_iter()
            .flatten()
            .filter_map(|position| ordered.get(position).copied())
        {
            if adjacent_seen.contains(&neighbour.id.as_str()) {
                continue;
            }
            adjacent_seen.push(neighbour.id.as_str());
            out.push(Excerpt {
                message_id: Told(neighbour.id.clone()),
                sequence: Told(neighbour.sequence),
                role: Told(neighbour.role.as_str().to_owned()),
                text: Told(neighbour.content.clone()),
                quoted: Internal::of(None),
            });
        }
    }
    out.sort_by_key(|passage| passage.sequence.0);
    out
}

/// Return a character-bounded excerpt that contains the supplied byte range.
fn clip_around(text: &str, start: usize, end: usize, max_chars: usize) -> (&str, bool) {
    if text.chars().count() <= max_chars {
        return (text, false);
    }
    let start = start.min(text.len());
    let end = end.min(text.len()).max(start);
    let quote_chars = text[start..end].chars().count();
    let remaining = max_chars.saturating_sub(quote_chars);
    let before_chars = remaining / 2;
    let after_chars = remaining.saturating_sub(before_chars);

    let mut clipped_start = start;
    for (offset, _) in text[..start].char_indices().rev().take(before_chars) {
        clipped_start = offset;
    }
    let mut clipped_end = end;
    for (offset, character) in text[end..].char_indices().take(after_chars) {
        clipped_end = end + offset + character.len_utf8();
    }
    (&text[clipped_start..clipped_end], true)
}

/// Whether a verdict says why. The reason is the only sign the passages were
/// read, and a model can answer with an empty one or a single filler word. A
/// script written without spaces is not held to words.
fn gives_a_reason(reason: &str) -> bool {
    let reason = reason.trim();
    reason.contains(char::is_whitespace) || (!reason.is_empty() && !reason.is_ascii())
}
