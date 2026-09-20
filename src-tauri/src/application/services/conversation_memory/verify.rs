//! Semantic review of a validated patch (§6.3).
//!
//! Design: `docs/design/2026-09-19-conversation-memory.md` §6.3.
//!
//! Deterministic validation proves a quote is real. It cannot tell a user's
//! instruction from the user pasting someone else's instruction, and it cannot
//! tell whether "use Rust instead" is about the same project as the earlier
//! "use Python". So every new mandatory item and every proposed transition gets
//! a second, structured look at the original passages.
//!
//! The direction of failure is fixed: when this step cannot be run, or answers
//! something unparseable, the result is [`SemanticVerdict::Ambiguous`], never
//! `Supported`. `Ambiguous` keeps a new mandatory requirement with its raw
//! passages and keeps an old requirement in force while recording the conflict.
//! The job refuses to commit an ambiguous optional addition, so an outage cannot
//! turn praise, pasted text, or an assistant-derived detail into memory.

use std::collections::HashMap;

use serde::Deserialize;
use tracing::warn;

use crate::application::ports::LLMPort;
use crate::domain::conversation_memory::{
    EvidenceSpan, MemoryItem, SemanticVerdict, SourceMessage, ValidatedPatch,
    ValidatedTransitionTarget,
};

use super::prompts::{
    render_verifier_prompt, verdict_schema, ContextPassage, ReviewPassage, ReviewRequest,
    VERIFIER_SYSTEM,
};
use super::segment::clip_chars;
use super::{complete_json, CompactionError, Deadline};

/// Proposals per review call. Small batches on purpose: a reviewer asked about
/// a dozen unrelated changes at once starts dropping ids, and a dropped id is a
/// silent `Ambiguous`.
pub const MAX_REVIEWS_PER_CALL: usize = 6;

/// Characters of surrounding original text shown per message.
const MAX_SURROUNDING_CHARS: usize = 1_500;

/// Verdicts for one patch, keyed the way [`apply_patch`] wants them.
///
/// [`apply_patch`]: crate::domain::conversation_memory::apply_patch
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

    let additions: Vec<&crate::domain::conversation_memory::ValidatedAddition> =
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

    let mut requests: Vec<OwnedReview> = Vec::new();
    for addition in &additions {
        requests.push(OwnedReview {
            id: format!("addition:{}", addition.candidate_id),
            change: "add_mandatory_item",
            kind: addition.kind.as_str(),
            label: addition.label.clone(),
            proposed: addition.evidence.clone(),
            existing: Vec::new(),
            existing_label: None,
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
        requests.push(OwnedReview {
            id: format!("transition:{}", transition.target.as_str()),
            change: match transition.state {
                crate::domain::conversation_memory::MemoryState::Resolved => "resolve_item",
                _ => "supersede_item",
            },
            kind: existing
                .map(|item| item.kind.as_str())
                .or_else(|| candidate.map(|item| item.kind.as_str()))
                .unwrap_or("constraint"),
            label: existing
                .map(|item| item.label.clone())
                .or_else(|| candidate.map(|item| item.label.clone()))
                .unwrap_or_default(),
            proposed: transition.evidence.clone(),
            existing: existing
                .map(|item| item.evidence.clone())
                .or_else(|| candidate.map(|item| item.evidence.clone()))
                .unwrap_or_default(),
            existing_label: existing
                .map(|item| item.label.clone())
                .or_else(|| candidate.map(|item| item.label.clone())),
        });
    }

    for batch in requests.chunks(MAX_REVIEWS_PER_CALL) {
        let rendered: Vec<ReviewRequest<'_>> = batch
            .iter()
            .map(|review| ReviewRequest {
                id: review.id.as_str(),
                change: review.change,
                kind: review.kind,
                label: review.label.as_str(),
                proposed_evidence: passages(&review.proposed, evidence),
                existing_evidence: passages(&review.existing, evidence),
                existing_label: review.existing_label.as_deref(),
                surrounding: surrounding(&review.proposed, &review.existing, &messages),
            })
            .collect();

        let prompt = render_verifier_prompt(&rendered);
        outcome.calls += 1;
        let raw = match complete_json(
            llm,
            deadline,
            VERIFIER_SYSTEM,
            prompt,
            Some(verdict_schema()),
        )
        .await
        {
            Ok(raw) => raw,
            // A hard limit stops the job. A provider failure does not: the
            // batch keeps its `Ambiguous` defaults and the pass continues.
            Err(error @ (CompactionError::DeadlineExceeded(_) | CompactionError::Cancelled)) => {
                return Err(error)
            }
            Err(error) => {
                warn!(
                    reviews = batch.len(),
                    code = error.code(),
                    "Memory review call failed — those changes stay ambiguous"
                );
                outcome.degraded = true;
                continue;
            }
        };

        let Some(parsed) = parse_verdicts(&raw) else {
            warn!(
                reviews = batch.len(),
                "Memory review response was unparseable — those changes stay ambiguous"
            );
            outcome.degraded = true;
            continue;
        };
        let mut answered = std::collections::HashSet::new();
        for verdict in parsed {
            let Some(decision) = parse_verdict(&verdict.verdict) else {
                continue;
            };
            // Only ids that were actually asked about are updated. A verdict for
            // anything else is a reviewer answering a question nobody put, and
            // accepting it would let the model name its own subject.
            if let Some(id) = verdict.id.strip_prefix("addition:") {
                if let Some(slot) = outcome.addition_verdicts.get_mut(id) {
                    *slot = decision;
                    answered.insert(verdict.id.clone());
                }
            } else if let Some(id) = verdict.id.strip_prefix("transition:") {
                if let Some(slot) = outcome.transition_verdicts.get_mut(id) {
                    *slot = decision;
                    answered.insert(verdict.id.clone());
                }
            }
        }
        if batch.iter().any(|review| !answered.contains(&review.id)) {
            warn!(
                reviews = batch.len(),
                answered = answered.len(),
                "Memory review omitted requested ids — missing changes stay ambiguous"
            );
            outcome.degraded = true;
        }
    }

    Ok(outcome)
}

/// One review, owned so batches can borrow from it without fighting lifetimes.
struct OwnedReview {
    id: String,
    change: &'static str,
    kind: &'static str,
    label: String,
    proposed: Vec<EvidenceSpan>,
    existing: Vec<EvidenceSpan>,
    existing_label: Option<String>,
}

fn passages<'a>(spans: &'a [EvidenceSpan], evidence: &'a EvidenceText) -> Vec<ReviewPassage<'a>> {
    spans
        .iter()
        .map(|span| ReviewPassage {
            message_id: span.message_id.as_str(),
            sequence: span.sequence,
            role: span.role.as_str(),
            text: evidence.text(span),
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
) -> Vec<ContextPassage<'a>> {
    let spans: Vec<&EvidenceSpan> = proposed.iter().chain(existing).collect();
    let mut out: Vec<ContextPassage<'a>> = Vec::new();
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
        let (text, truncated) = clip_around(&message.content, start, end, MAX_SURROUNDING_CHARS);
        out.push(ContextPassage {
            message_id: message.id.as_str(),
            sequence: message.sequence,
            role: message.role.as_str(),
            text,
            truncated,
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
            let (text, truncated) = clip_chars(&neighbour.content, MAX_SURROUNDING_CHARS);
            out.push(ContextPassage {
                message_id: neighbour.id.as_str(),
                sequence: neighbour.sequence,
                role: neighbour.role.as_str(),
                text,
                truncated,
            });
        }
    }
    out.sort_by_key(|passage| passage.sequence);
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

#[derive(Debug, Deserialize)]
struct VerdictResponse {
    #[serde(default)]
    verdicts: Vec<RawVerdict>,
}

#[derive(Debug, Deserialize)]
struct RawVerdict {
    id: String,
    verdict: String,
}

/// Parse the reviewer response, tolerating one markdown fence.
fn parse_verdicts(raw: &str) -> Option<Vec<RawVerdict>> {
    let trimmed = raw.trim();
    let body = match trimmed.strip_prefix("```") {
        Some(rest) => {
            let rest = rest.split_once('\n').map_or(rest, |(_, body)| body);
            rest.trim_end().strip_suffix("```")?
        }
        None => trimmed,
    };
    serde_json::from_str::<VerdictResponse>(body.trim())
        .ok()
        .map(|response| response.verdicts)
}

/// Map a verdict word. An unrecognised word is not a vote for `Supported`.
fn parse_verdict(word: &str) -> Option<SemanticVerdict> {
    match word.trim().to_ascii_lowercase().as_str() {
        "supported" => Some(SemanticVerdict::Supported),
        "ambiguous" => Some(SemanticVerdict::Ambiguous),
        "unsupported" => Some(SemanticVerdict::Unsupported),
        _ => None,
    }
}
