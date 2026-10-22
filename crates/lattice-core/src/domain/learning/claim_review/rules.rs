//! §3's rules that look across claims or at what a pass observes: new
//! information, re-litigation of a human verdict, stable identity across
//! re-extraction, and the BLOCKED end state (§2.1, §6.1).

use std::collections::BTreeSet;

use super::{
    span_of, text_sha256, Actor, ClaimId, ClaimLedgerEntry, ClaimLocation, ClaimState,
    EvidenceFingerprint, GateVerdict, HumanBinding, LessonReviewState, ReopenCause,
};

/// A change to a source version a claim relied on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SourceChange {
    /// The source's active version moved on.
    Replaced { from: String, to: String },
    /// The source was deleted; the id is the version the claim relied on.
    Deleted(String),
}

impl SourceChange {
    fn version(&self) -> &str {
        match self {
            Self::Replaced { from, .. } => from,
            Self::Deleted(version) => version,
        }
    }
}

/// What a pass sees for one claim, to compare with what it saw before.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Observation {
    pub quote: String,
    pub fingerprint: EvidenceFingerprint,
    pub checker_model: String,
    pub window_tokens: usize,
    /// Changes to the sources' active versions since the claim's last check.
    pub source_changes: Vec<SourceChange>,
    /// A research hint the user gave for this claim.
    pub hint: Option<String>,
    /// A source version the user attached to this claim.
    pub attached_source_version_id: Option<String>,
    /// What the provenance gate made of this pass's supported verdict;
    /// `None` when the gate did not weigh it.
    pub gate: Option<GateVerdict>,
}

/// Why the machine may look at a claim again.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NewInformation {
    /// The claim's quote changed; carries the new quote's SHA-256. A
    /// location that only narrows (a stored passage re-located to one of
    /// its sentences while the field still holds the passage) is not a
    /// changed sentence: the pass records the new location and nothing else
    /// (claim sources design §18.4 i).
    SentenceChanged {
        quote_sha256: String,
    },
    SourceChanged(SourceChange),
    /// A different checker model or context window.
    CheckerChanged,
    /// Different passages (or none were compared before).
    EvidenceChanged,
    /// The same evidence, but the provenance gate decides it differently:
    /// an override, a relation or a calibrated bar changed (source
    /// provenance §18.7).
    DecisionChanged,
    UserSource(String),
    UserHint(String),
}

/// §3 `new_information`: the first cause that applies, in the order of the
/// variants of [`NewInformation`]; `None` means nothing new, so the machine
/// leaves the claim as it is.
pub fn new_information(prev: &ClaimLedgerEntry, now: &Observation) -> Option<NewInformation> {
    if now.quote != prev.location().quote {
        return Some(NewInformation::SentenceChanged {
            quote_sha256: text_sha256(&now.quote),
        });
    }
    let cited = cited_sources(prev);
    if let Some(change) = now
        .source_changes
        .iter()
        .find(|change| cited.contains(change.version()))
    {
        return Some(NewInformation::SourceChanged(change.clone()));
    }
    match prev.basis() {
        Some(basis)
            if basis.checker_model != now.checker_model
                || basis.window_tokens != now.window_tokens =>
        {
            return Some(NewInformation::CheckerChanged)
        }
        Some(basis) if basis.fingerprint == now.fingerprint => {
            if let (Some(gate), Some(before)) = (&now.gate, prev.trust()) {
                if *gate != before.verdict() {
                    return Some(NewInformation::DecisionChanged);
                }
            }
        }
        // A verified claim is not re-judged on reselected evidence (claim
        // sources design §18.4 ii): only its sentence or a source it cites
        // changing reopens it, as `reopen_cause` says. The gate weighed other
        // evidence, so it is not compared either.
        _ if matches!(prev.state(), ClaimState::ResolvedByEvidence { .. }) => {}
        _ => return Some(NewInformation::EvidenceChanged),
    }
    now.attached_source_version_id
        .clone()
        .map(NewInformation::UserSource)
        .or_else(|| now.hint.clone().map(NewInformation::UserHint))
}

/// The source versions a claim relies on: what its last local check
/// compared, what resolved it and what the user bound a confirmation to.
fn cited_sources(entry: &ClaimLedgerEntry) -> BTreeSet<&str> {
    let mut cited: BTreeSet<&str> = entry
        .basis()
        .map(|basis| {
            basis
                .source_version_ids
                .iter()
                .map(String::as_str)
                .collect()
        })
        .unwrap_or_default();
    let mut state = entry.state();
    if let ClaimState::Reopened { previous, .. } = state {
        state = previous;
    }
    match state {
        ClaimState::ResolvedByEvidence {
            source_version_id, ..
        } => {
            cited.insert(source_version_id);
        }
        ClaimState::Confirmed { bound, .. } => {
            cited.extend(bound.source_version_ids.iter().map(String::as_str));
        }
        _ => {}
    }
    cited
}

/// §3 `relitigate`: whether a change reopens the user's confirmation, and
/// why. Only a changed sentence or a change to a source the user bound the
/// confirmation to does; a new checker, new research, new sources or a
/// change of trust (an override, a calibrated bar: source provenance §13)
/// never do. (Unconfirm is the user's own action,
/// [`super::UserAction::Unconfirm`].)
pub fn relitigate(bound: &HumanBinding, change: &NewInformation) -> Option<ReopenCause> {
    match change {
        NewInformation::SentenceChanged { quote_sha256 } => {
            (*quote_sha256 != bound.quote_sha256).then_some(ReopenCause::SentenceChanged)
        }
        NewInformation::SourceChanged(source) => bound
            .source_version_ids
            .iter()
            .any(|id| id == source.version())
            .then(|| source_cause(source)),
        NewInformation::DecisionChanged => None,
        _ => None,
    }
}

/// The reopen a change causes for a closed claim (§5.3): a confirmation per
/// [`relitigate`]; a machine verification when its source is deleted or
/// replaced. A machine verification whose sentence changed is checked again
/// instead, and other states have nothing to reopen.
pub fn reopen_cause(state: &ClaimState, change: &NewInformation) -> Option<ReopenCause> {
    match (state, change) {
        (ClaimState::Confirmed { bound, .. }, _) => relitigate(bound, change),
        (
            ClaimState::ResolvedByEvidence {
                source_version_id, ..
            },
            NewInformation::SourceChanged(source),
        ) if source.version() == source_version_id => Some(source_cause(source)),
        _ => None,
    }
}

fn source_cause(change: &SourceChange) -> ReopenCause {
    match change {
        SourceChange::Replaced { from, to } => ReopenCause::SourceReplaced {
            from: from.clone(),
            to: to.clone(),
        },
        SourceChange::Deleted(version) => ReopenCause::SourceDeleted(version.clone()),
    }
}

/// A claim as a re-extraction found it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtractedClaim {
    pub location: ClaimLocation,
    pub statement: String,
}

/// Which prior claim each extracted claim continues, and which prior claims
/// have no successor (they become `Edited { successor: None }`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Identities {
    /// Per extracted claim, in order: the prior id it keeps, or `None` for a
    /// new claim (which gets [`ClaimId::generate`]).
    pub assigned: Vec<Option<ClaimId>>,
    pub orphaned: Vec<ClaimId>,
}

/// §3 `identity`, applied to a whole re-extraction so no prior id is given
/// twice. A claim keeps a prior id when it is in the same unit and has the
/// same statement or the same quote; failing that, when the prior quote's
/// span in the same field (where it occurs once now, or else its stored span)
/// overlaps the new claim's (the largest overlap wins). `prior` is the live
/// claims of the re-extracted units; `field_text(unit_index, field)` is a
/// field's current text.
pub fn identities<'a>(
    prior: &[&ClaimLedgerEntry],
    extracted: &[ExtractedClaim],
    field_text: impl Fn(usize, &str) -> Option<&'a str>,
) -> Identities {
    let mut assigned: Vec<Option<ClaimId>> = vec![None; extracted.len()];
    let mut taken = vec![false; prior.len()];
    let mut give = |claim: usize, owner: usize| {
        if let (Some(slot @ None), Some(used @ false)) =
            (assigned.get_mut(claim), taken.get_mut(owner))
        {
            *used = true;
            *slot = prior.get(owner).map(|entry| entry.claim_id().clone());
        }
    };

    // Same statement or same quote: the strongest match first.
    let mut exact: Vec<(u8, usize, usize)> = Vec::new();
    for (claim, new) in extracted.iter().enumerate() {
        for (owner, old) in prior.iter().enumerate() {
            let same_unit = old.location().unit_index == new.location.unit_index;
            let same_quote = old.location().quote == new.location.quote;
            let same_statement = old.statement() == new.statement;
            if same_unit && (same_quote || same_statement) {
                exact.push((
                    u8::from(same_quote) + u8::from(same_statement),
                    claim,
                    owner,
                ));
            }
        }
    }
    exact.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)).then(a.2.cmp(&b.2)));
    for (_, claim, owner) in exact {
        give(claim, owner);
    }

    // Overlapping spans in the same field: the largest overlap first.
    let mut overlaps: Vec<(usize, usize, usize)> = Vec::new();
    for (claim, new) in extracted.iter().enumerate() {
        let Some((start, end)) = new.location.span else {
            continue;
        };
        for (owner, old) in prior.iter().enumerate() {
            let old = old.location();
            if old.unit_index != new.location.unit_index || old.field != new.location.field {
                continue;
            }
            // A quote the field no longer holds (a repaired passage) falls
            // back to where the claim last stood.
            let Some((old_start, old_end)) = field_text(old.unit_index, &old.field)
                .and_then(|text| span_of(text, &old.quote))
                .or(old.span)
            else {
                continue;
            };
            let overlap = end.min(old_end).saturating_sub(start.max(old_start));
            if overlap > 0 {
                overlaps.push((overlap, claim, owner));
            }
        }
    }
    overlaps.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)).then(a.2.cmp(&b.2)));
    for (_, claim, owner) in overlaps {
        give(claim, owner);
    }

    let orphaned = prior
        .iter()
        .zip(&taken)
        .filter(|(_, used)| !**used)
        .map(|(entry, _)| entry.claim_id().clone())
        .collect();
    Identities { assigned, orphaned }
}

/// The BLOCKED rule (§2.1, §6.1): while any claim is the machine's the lesson
/// is still verifying; once none is, it is ready only if every claim is
/// resolved, and otherwise needs the user's review.
pub fn end_state<'a>(claims: impl IntoIterator<Item = &'a ClaimLedgerEntry>) -> LessonReviewState {
    let mut needs_review = false;
    for claim in claims {
        let state = claim.state();
        if state.is_machine_owned() && !claim.held_for_standing() {
            return LessonReviewState::Verifying;
        }
        needs_review |= !state.is_resolved();
    }
    if needs_review {
        LessonReviewState::NeedsReview
    } else {
        LessonReviewState::Ready
    }
}

/// Claims per outcome, for the review header and the lesson card (§6.3).
/// An edited claim with a successor is counted through its successor. What
/// the user did and what the machine did are counted apart (claim sources
/// design §11): only the user's actions are "resolved by you".
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ClaimCounts {
    pub verified: usize,
    pub confirmed: usize,
    pub general_knowledge: usize,
    /// Waiting for the user: Queued, Reopened, or held for standing.
    pub queued: usize,
    /// The machine's: Unchecked or Open.
    pub open: usize,
    /// Removed by the user.
    pub removed_by_you: usize,
    /// Edited away by the user: no successor claim.
    pub edited_by_you: usize,
    /// Rewritten or removed by the machine's repairs: removed, or edited
    /// with no successor.
    pub rewritten: usize,
}

impl ClaimCounts {
    pub fn tally<'a>(claims: impl IntoIterator<Item = &'a ClaimLedgerEntry>) -> Self {
        let mut counts = Self::default();
        for claim in claims {
            let slot = match claim.state() {
                ClaimState::ResolvedByEvidence { .. } => &mut counts.verified,
                ClaimState::Confirmed { .. } => &mut counts.confirmed,
                ClaimState::GeneralKnowledge { .. } => &mut counts.general_knowledge,
                ClaimState::Queued { .. } | ClaimState::Reopened { .. } => &mut counts.queued,
                ClaimState::Open { .. } if claim.held_for_standing() => &mut counts.queued,
                ClaimState::Unchecked | ClaimState::Open { .. } => &mut counts.open,
                ClaimState::Removed {
                    by: Actor::User, ..
                } => &mut counts.removed_by_you,
                ClaimState::Edited {
                    successor: None,
                    by: Actor::User,
                    ..
                } => &mut counts.edited_by_you,
                ClaimState::Removed {
                    by: Actor::Machine, ..
                }
                | ClaimState::Edited {
                    successor: None,
                    by: Actor::Machine,
                    ..
                } => &mut counts.rewritten,
                ClaimState::Edited {
                    successor: Some(_), ..
                } => continue,
            };
            *slot += 1;
        }
        counts
    }

    /// The user's own verdicts: confirmed, accepted as general knowledge,
    /// removed or edited away.
    pub fn resolved_by_you(&self) -> usize {
        self.confirmed + self.general_knowledge + self.removed_by_you + self.edited_by_you
    }
}
