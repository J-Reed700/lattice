//! One claim's append-only event log and the state replayed from it.
//!
//! Every event is validated against the state it lands on, so a log that
//! loads is a log the rules allowed. State is never written, only replayed:
//! undo restores the state as it stood before the undone event (§5.1).

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use super::replay::{apply_check, charge, hand_to_user, queued, redecided, reopened, Cursor};
use super::{
    Actor, AttachOutcome, ClaimEvent, ClaimId, ClaimLocation, ClaimState, EvidenceFingerprint,
    ReopenCause, RepairOutcome, TrustAssessment, UserAction, VerdictKind, MACHINE_ATTEMPTS,
};
use crate::shared::error::AppError;

/// Who appended an event, when, and under which job, app version and
/// checker model (§5.1 provenance).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventProvenance {
    pub actor: Actor,
    /// Milliseconds since the Unix epoch, as the learning tables store time.
    pub at: i64,
    pub job_id: Option<String>,
    pub app_version: String,
    pub checker_model: Option<String>,
}

/// An event as stored: its ordinal in the claim's log (0, 1, 2, ...).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecordedEvent {
    pub ordinal: u32,
    pub provenance: EventProvenance,
    pub event: ClaimEvent,
}

/// What the claim's latest local check depended on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckBasis {
    pub fingerprint: EvidenceFingerprint,
    pub checker_model: String,
    pub window_tokens: usize,
    pub source_version_ids: BTreeSet<String>,
}

/// The per-claim actions of the review card (§5.1), in the card's order.
/// "Accept all general-knowledge proposals" is a lesson action made of one
/// `GeneralKnowledge` per claim.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClaimActionKind {
    Confirm,
    Edit,
    Remove,
    GeneralKnowledge,
    AttachEvidence,
    ResearchWithHint,
    Unconfirm,
    Undo,
}

/// An event the claim's state does not allow; nothing was appended.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum LedgerError {
    #[error("claim {claim}: event {ordinal} is out of order (expected {expected})")]
    OutOfOrder {
        claim: String,
        ordinal: u32,
        expected: u32,
    },
    #[error("claim {claim}: a claim's log starts with its extraction")]
    NotExtractedFirst { claim: String },
    #[error("claim {claim}: a {kind} event cannot be recorded as the {actor} actor")]
    WrongActor {
        claim: String,
        kind: &'static str,
        actor: &'static str,
    },
    #[error("claim {claim}: {kind} is not allowed while the claim is {state}")]
    NotAllowed {
        claim: String,
        kind: &'static str,
        state: &'static str,
    },
    #[error("claim {claim}: confirming a contradicted claim needs a note")]
    NoteRequired { claim: String },
    #[error("claim {claim}: event {ordinal} is not the event undo applies to")]
    NotUndoable { claim: String, ordinal: u32 },
    #[error("claim {claim}: repair attempt {attempt} (escalated: {escalated}) does not match the attempts left")]
    RepairMismatch {
        claim: String,
        attempt: u8,
        escalated: bool,
    },
}

impl From<LedgerError> for AppError {
    fn from(error: LedgerError) -> Self {
        AppError::InvalidState(error.to_string())
    }
}

/// One claim: its identity, where it is, and its log.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClaimLedgerEntry {
    claim_id: ClaimId,
    location: ClaimLocation,
    statement: String,
    events: Vec<RecordedEvent>,
    /// `before[i]` is the position `events[i]` landed on; undo returns to it.
    before: Vec<Cursor>,
    current: Cursor,
}

impl ClaimLedgerEntry {
    /// A newly extracted claim: its log is the extraction.
    pub fn extracted(
        claim_id: ClaimId,
        provenance: EventProvenance,
        location: ClaimLocation,
        statement: String,
    ) -> Result<Self, LedgerError> {
        let event = RecordedEvent {
            ordinal: 0,
            provenance,
            event: ClaimEvent::Extracted {
                location,
                statement,
            },
        };
        Self::from_events(claim_id, vec![event])
    }

    /// A stored log, replayed and validated event by event.
    pub fn from_events(claim_id: ClaimId, events: Vec<RecordedEvent>) -> Result<Self, LedgerError> {
        let mut events = events.into_iter();
        let first = events
            .next()
            .ok_or_else(|| LedgerError::NotExtractedFirst {
                claim: claim_id.0.clone(),
            })?;
        let (location, statement) = match &first.event {
            ClaimEvent::Extracted {
                location,
                statement,
            } if first.ordinal == 0 && first.provenance.actor == Actor::Machine => {
                (location.clone(), statement.clone())
            }
            _ => {
                return Err(LedgerError::NotExtractedFirst {
                    claim: claim_id.0.clone(),
                })
            }
        };
        let unchecked = Cursor {
            state: ClaimState::Unchecked,
            basis: None,
            repair_pending: false,
            researched: false,
            latest_failure: None,
            trust: None,
        };
        let mut entry = Self {
            claim_id,
            location,
            statement,
            events: vec![first],
            before: vec![unchecked.clone()],
            current: unchecked,
        };
        for event in events {
            entry.push(event)?;
        }
        Ok(entry)
    }

    /// Appends `event` and returns its ordinal, refusing it when the claim's
    /// state does not allow it.
    pub fn append(
        &mut self,
        provenance: EventProvenance,
        event: ClaimEvent,
    ) -> Result<u32, LedgerError> {
        let ordinal = self.next_ordinal();
        self.push(RecordedEvent {
            ordinal,
            provenance,
            event,
        })?;
        Ok(ordinal)
    }

    pub fn claim_id(&self) -> &ClaimId {
        &self.claim_id
    }

    /// Where the claim is, as of its latest extraction.
    pub fn location(&self) -> &ClaimLocation {
        &self.location
    }

    pub fn statement(&self) -> &str {
        &self.statement
    }

    pub fn events(&self) -> &[RecordedEvent] {
        &self.events
    }

    pub fn state(&self) -> &ClaimState {
        &self.cursor().state
    }

    /// What the latest local check depended on; `None` before the first.
    pub fn basis(&self) -> Option<&CheckBasis> {
        self.cursor().basis.as_ref()
    }

    /// The latest failing verdict from either checker, kept after the claim
    /// closes so a card can say what the user confirmed over.
    pub fn latest_failure(&self) -> Option<&VerdictKind> {
        self.cursor().latest_failure.as_ref()
    }

    /// What the provenance gate weighed for the claim's latest local check,
    /// when the gate decided it: the claim's state is then the gate's.
    pub fn trust(&self) -> Option<&TrustAssessment> {
        self.cursor().trust.as_ref()
    }

    /// Whether a trust change may re-decide the claim (source provenance
    /// §18.7): its state is the gate's (a failure the gate's assessment
    /// gave, a verification it allowed, or a reopen an earlier trust change
    /// made) and no rewrite awaits its check. A human verdict never is.
    pub fn redecidable(&self) -> bool {
        let cursor = self.cursor();
        cursor.trust.is_some()
            && !cursor.repair_pending
            && matches!(
                &cursor.state,
                ClaimState::Open { .. }
                    | ClaimState::Queued { .. }
                    | ClaimState::ResolvedByEvidence { .. }
                    | ClaimState::Reopened {
                        cause: ReopenCause::TrustChanged(_),
                        ..
                    }
            )
    }

    /// §3 `may_repair`: Open, attempts left, a kind a rewrite can address,
    /// and no rewrite already waiting for its check.
    pub fn may_repair(&self) -> bool {
        let cursor = self.cursor();
        matches!(
            &cursor.state,
            ClaimState::Open {
                kind,
                machine_attempts_left,
                ..
            } if kind.is_repairable() && *machine_attempts_left > 0
        ) && !cursor.repair_pending
    }

    /// The next repair is the escalated one (`repeatedDefects`): it is the
    /// claim's last attempt on this evidence.
    pub fn next_repair_escalates(&self) -> bool {
        matches!(
            self.state(),
            ClaimState::Open {
                machine_attempts_left: 1,
                ..
            }
        )
    }

    /// Research runs once per claim and fingerprint, for the kinds new
    /// passages can answer (§4).
    pub fn may_research(&self) -> bool {
        let cursor = self.cursor();
        matches!(
            &cursor.state,
            ClaimState::Open { kind, .. }
                if kind.is_repairable() || *kind == VerdictKind::NoSource
        ) && !cursor.researched
            && !cursor.repair_pending
    }

    /// Confirming needs a note when a passage contradicts the claim (D3).
    pub fn confirm_requires_note(&self) -> bool {
        self.latest_failure()
            .is_some_and(VerdictKind::is_contradiction)
    }

    /// The ordinal undo applies to: the claim's last user event, or the
    /// machine's removal of the sentence when that came later and still
    /// stands (§4: the user may object to a machine removal).
    pub fn undo_target(&self) -> Option<u32> {
        let last = |wanted: &dyn Fn(&RecordedEvent) -> bool| {
            self.events
                .iter()
                .rev()
                .find(|event| wanted(event))
                .map(|event| event.ordinal)
        };
        let last_user = last(&|event| event.provenance.actor == Actor::User);
        let machine_removal = match self.state() {
            ClaimState::Removed {
                by: Actor::Machine, ..
            } => last(&is_machine_removal),
            _ => None,
        };
        last_user.max(machine_removal)
    }

    /// The per-claim actions the current state allows, in the card's order.
    pub fn available_actions(&self) -> Vec<ClaimActionKind> {
        [
            ClaimActionKind::Confirm,
            ClaimActionKind::Edit,
            ClaimActionKind::Remove,
            ClaimActionKind::GeneralKnowledge,
            ClaimActionKind::AttachEvidence,
            ClaimActionKind::ResearchWithHint,
            ClaimActionKind::Unconfirm,
            ClaimActionKind::Undo,
        ]
        .into_iter()
        .filter(|action| self.allows(*action))
        .collect()
    }

    /// §5.1's preconditions, by state.
    pub fn allows(&self, action: ClaimActionKind) -> bool {
        let state = self.state();
        match action {
            ClaimActionKind::Confirm | ClaimActionKind::ResearchWithHint => is_open(state),
            ClaimActionKind::Unconfirm => matches!(state, ClaimState::Confirmed { .. }),
            ClaimActionKind::Edit | ClaimActionKind::Remove => !matches!(
                state,
                ClaimState::Removed { .. } | ClaimState::Edited { .. }
            ),
            ClaimActionKind::GeneralKnowledge => matches!(
                state,
                ClaimState::Open {
                    kind: VerdictKind::Unsupported | VerdictKind::GeneralKnowledgeProposed,
                    ..
                } | ClaimState::Queued {
                    kind: VerdictKind::Unsupported | VerdictKind::GeneralKnowledgeProposed,
                    ..
                }
            ),
            ClaimActionKind::AttachEvidence => {
                is_open(state) || matches!(state, ClaimState::Confirmed { .. })
            }
            ClaimActionKind::Undo => self.undo_target().is_some(),
        }
    }

    fn cursor(&self) -> &Cursor {
        &self.current
    }

    fn next_ordinal(&self) -> u32 {
        u32::try_from(self.events.len()).unwrap_or(u32::MAX)
    }

    fn push(&mut self, record: RecordedEvent) -> Result<(), LedgerError> {
        let expected = self.next_ordinal();
        if record.ordinal != expected {
            return Err(LedgerError::OutOfOrder {
                claim: self.claim_id.0.clone(),
                ordinal: record.ordinal,
                expected,
            });
        }
        self.check_actor(&record)?;
        let next = self.apply(&record)?;
        if let ClaimEvent::Extracted {
            location,
            statement,
        } = &record.event
        {
            self.location = location.clone();
            self.statement = statement.clone();
        }
        self.events.push(record);
        self.before.push(std::mem::replace(&mut self.current, next));
        Ok(())
    }

    fn check_actor(&self, record: &RecordedEvent) -> Result<(), LedgerError> {
        let required = match &record.event {
            ClaimEvent::UserAction { .. }
            | ClaimEvent::EvidenceAttached { .. }
            | ClaimEvent::Undone { .. } => Some(Actor::User),
            ClaimEvent::Researched { .. } => None,
            _ => Some(Actor::Machine),
        };
        match required {
            Some(actor) if actor != record.provenance.actor => Err(LedgerError::WrongActor {
                claim: self.claim_id.0.clone(),
                kind: record.event.kind_name(),
                actor: record.provenance.actor.as_str(),
            }),
            _ => Ok(()),
        }
    }

    fn refuse(&self, event: &ClaimEvent) -> LedgerError {
        LedgerError::NotAllowed {
            claim: self.claim_id.0.clone(),
            kind: event.kind_name(),
            state: state_name(self.state()),
        }
    }

    /// The position after `record`, or why the state does not allow it.
    fn apply(&self, record: &RecordedEvent) -> Result<Cursor, LedgerError> {
        let mut next = self.cursor().clone();
        let at = record.provenance.at;
        match &record.event {
            ClaimEvent::Extracted { .. } => {
                if is_gone(&next.state) {
                    return Err(self.refuse(&record.event));
                }
            }
            ClaimEvent::Checked(check) => apply_check(&mut next, check, false),
            ClaimEvent::Researched { passages_found, .. } => {
                next.researched = true;
                if let ClaimState::Open {
                    kind: VerdictKind::NoSource,
                    reason,
                    ..
                } = &next.state
                {
                    if *passages_found == 0 {
                        next.state = queued(VerdictKind::NoSource, reason.clone());
                    }
                }
            }
            ClaimEvent::RepairAttempted {
                attempt,
                escalated,
                outcome,
                ..
            } => self.apply_repair(&mut next, *attempt, *escalated, outcome, at, &record.event)?,
            ClaimEvent::Executed { .. } | ClaimEvent::Stopped { .. } => {}
            ClaimEvent::StandingAssessed {
                trust,
                reason,
                cause,
            } => match cause {
                _ if is_gone(&next.state) => return Err(self.refuse(&record.event)),
                None => next.trust = Some(trust.as_ref().clone()),
                Some(_) if !self.redecidable() => return Err(self.refuse(&record.event)),
                Some(cause) => redecided(&mut next, trust, reason, cause),
            },
            ClaimEvent::UserAction { action, note } => {
                self.apply_user_action(&mut next, action, note, at, &record.event)?
            }
            ClaimEvent::EvidenceAttached { result, .. } => {
                if !self.allows(ClaimActionKind::AttachEvidence) {
                    return Err(self.refuse(&record.event));
                }
                match result {
                    AttachOutcome::Checked(check) => apply_check(&mut next, check, true),
                    AttachOutcome::NoPassage => hand_to_user(&mut next.state),
                }
            }
            ClaimEvent::Superseded { successor } => {
                next.state = match &next.state {
                    ClaimState::Removed { .. } => return Err(self.refuse(&record.event)),
                    ClaimState::Edited { at: edited, .. } => ClaimState::Edited {
                        at: *edited,
                        successor: successor.clone(),
                    },
                    _ => ClaimState::Edited {
                        at,
                        successor: successor.clone(),
                    },
                };
            }
            ClaimEvent::Reopened { cause } => {
                if !matches!(
                    next.state,
                    ClaimState::ResolvedByEvidence { .. }
                        | ClaimState::Confirmed { .. }
                        | ClaimState::GeneralKnowledge { .. }
                ) {
                    return Err(self.refuse(&record.event));
                }
                next.state = reopened(cause.clone(), next.state);
            }
            ClaimEvent::Undone { event_ordinal } => {
                let target = (self.undo_target() == Some(*event_ordinal))
                    .then(|| usize::try_from(*event_ordinal).ok())
                    .flatten()
                    .and_then(|index| Some((self.events.get(index)?, self.before.get(index)?)));
                let Some((undone, before)) = target else {
                    return Err(LedgerError::NotUndoable {
                        claim: self.claim_id.0.clone(),
                        ordinal: *event_ordinal,
                    });
                };
                next = before.clone();
                if is_machine_removal(undone) {
                    hand_to_user(&mut next.state);
                }
            }
        }
        Ok(next)
    }

    fn apply_repair(
        &self,
        next: &mut Cursor,
        attempt: u8,
        escalated: bool,
        outcome: &RepairOutcome,
        at: i64,
        event: &ClaimEvent,
    ) -> Result<(), LedgerError> {
        if *outcome == RepairOutcome::SupersededByUser {
            return Ok(());
        }
        let ClaimState::Open {
            kind,
            reason,
            machine_attempts_left,
        } = &next.state
        else {
            return Err(self.refuse(event));
        };
        if !self.may_repair() {
            return Err(self.refuse(event));
        }
        if attempt != (MACHINE_ATTEMPTS + 1).saturating_sub(*machine_attempts_left)
            || escalated != (*machine_attempts_left == 1)
        {
            return Err(LedgerError::RepairMismatch {
                claim: self.claim_id.0.clone(),
                attempt,
                escalated,
            });
        }
        match outcome {
            RepairOutcome::Rewritten => next.repair_pending = true,
            RepairOutcome::Removed => {
                next.state = ClaimState::Removed {
                    at,
                    by: Actor::Machine,
                }
            }
            RepairOutcome::Rejected { .. } => {
                next.state = charge(kind.clone(), reason.clone(), *machine_attempts_left)
            }
            RepairOutcome::SupersededByUser => {}
        }
        Ok(())
    }

    fn apply_user_action(
        &self,
        next: &mut Cursor,
        action: &UserAction,
        note: &Option<String>,
        at: i64,
        event: &ClaimEvent,
    ) -> Result<(), LedgerError> {
        let kind = match action {
            UserAction::Confirm { .. } => ClaimActionKind::Confirm,
            UserAction::Unconfirm => ClaimActionKind::Unconfirm,
            UserAction::Edit { .. } => ClaimActionKind::Edit,
            UserAction::Remove => ClaimActionKind::Remove,
            UserAction::GeneralKnowledge => ClaimActionKind::GeneralKnowledge,
            UserAction::ResearchWithHint { .. } => ClaimActionKind::ResearchWithHint,
        };
        if !self.allows(kind) {
            return Err(self.refuse(event));
        }
        let note = note.as_ref().filter(|text| !text.trim().is_empty());
        if matches!(action, UserAction::Confirm { .. })
            && note.is_none()
            && self.confirm_requires_note()
        {
            return Err(LedgerError::NoteRequired {
                claim: self.claim_id.0.clone(),
            });
        }
        // The user's action supersedes any rewrite waiting for its check.
        next.repair_pending = false;
        match action {
            UserAction::Confirm { bound } => {
                next.state = ClaimState::Confirmed {
                    at,
                    note: note.cloned(),
                    bound: bound.clone(),
                }
            }
            UserAction::Unconfirm => {
                next.state = reopened(ReopenCause::Unconfirmed, next.state.clone())
            }
            UserAction::Edit { .. } => {
                next.state = ClaimState::Edited {
                    at,
                    successor: None,
                }
            }
            UserAction::Remove => {
                next.state = ClaimState::Removed {
                    at,
                    by: Actor::User,
                }
            }
            UserAction::GeneralKnowledge => {
                next.state = ClaimState::GeneralKnowledge {
                    at,
                    by: Actor::User,
                }
            }
            UserAction::ResearchWithHint { .. } => hand_to_user(&mut next.state),
        }
        Ok(())
    }
}

/// Open, Queued or Reopened: a claim the user may resolve.
fn is_open(state: &ClaimState) -> bool {
    matches!(state, ClaimState::Open { .. }) || state.awaits_user()
}

/// The sentence is gone: nothing to extract, check or supersede.
fn is_gone(state: &ClaimState) -> bool {
    matches!(
        state,
        ClaimState::Removed { .. } | ClaimState::Edited { .. }
    )
}

fn is_machine_removal(record: &RecordedEvent) -> bool {
    matches!(
        record.event,
        ClaimEvent::RepairAttempted {
            outcome: RepairOutcome::Removed,
            ..
        }
    )
}

fn state_name(state: &ClaimState) -> &'static str {
    match state {
        ClaimState::Unchecked => "unchecked",
        ClaimState::Open { .. } => "open",
        ClaimState::Queued { .. } => "queued",
        ClaimState::ResolvedByEvidence { .. } => "resolved by evidence",
        ClaimState::Confirmed { .. } => "confirmed",
        ClaimState::Removed { .. } => "removed",
        ClaimState::Edited { .. } => "edited",
        ClaimState::GeneralKnowledge { .. } => "general knowledge",
        ClaimState::Reopened { .. } => "reopened",
    }
}
