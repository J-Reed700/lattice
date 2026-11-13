//! Where a claim stands: the state replayed from its events.
use super::*;

/// Where a claim stands. Replayed from its events ([`ClaimLedgerEntry`]),
/// never written directly.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum ClaimState {
    /// Extracted and not yet checked; the machine owns it.
    Unchecked,
    /// The machine may still act.
    Open {
        kind: VerdictKind,
        reason: String,
        machine_attempts_left: u8,
    },
    /// Waiting for the user; the machine acts again only on new information.
    Queued { kind: VerdictKind, reason: String },
    /// Machine-verified against a saved source (or an execution).
    ResolvedByEvidence {
        source_version_id: String,
        supporting_quote: String,
    },
    /// A Reasoning statement its section's passages do not contradict and
    /// that asserts no specific they lack (claim classes design §3.11), or a
    /// statement the model sorted Internal that the falsity veto passed. Like
    /// a verification it is the machine's, and the lesson may publish with it.
    Consistent { at: i64 },
    /// The user's verdict.
    Confirmed {
        at: i64,
        note: Option<String>,
        bound: HumanBinding,
    },
    /// The sentence or claim was deleted.
    Removed { at: i64, by: Actor },
    /// The text changed; the successor claim owns the state. `None` is the
    /// user's edit that left no successor, "removed by the edit": the
    /// machine's change with none is [`ClaimState::Removed`]. `by`: who
    /// changed the text, the user's edit or the machine's repair (claim
    /// sources design §11).
    Edited {
        at: i64,
        successor: Option<ClaimId>,
        #[serde(default)]
        by: Actor,
    },
    /// A Common statement the knowledge judge passed: standard knowledge,
    /// checked against no source (claim classes design §3.11).
    GeneralKnowledge { at: i64 },
    /// A closed claim reopened; it waits for the user like `Queued`.
    Reopened {
        cause: ReopenCause,
        previous: Box<ClaimState>,
    },
    /// The machine did all it will do and could not settle the statement: the
    /// lesson publishes with it, marked not verified. It is the machine's
    /// word, never a verification and never the user's decision.
    Unverified { at: i64, reason: UnverifiedReason },
}

/// Why a statement was published without being verified.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UnverifiedReason {
    /// The check could not be completed.
    Unchecked,
    /// The lesson's sources do not state it.
    Unsupported,
    /// A source appears to disagree with it.
    Contradicted,
}

impl UnverifiedReason {
    /// Why a statement whose last failure is `kind` is published not verified.
    pub fn of(kind: &VerdictKind) -> Self {
        match kind {
            kind if kind.is_contradiction() => Self::Contradicted,
            VerdictKind::CouldNotJudge => Self::Unchecked,
            _ => Self::Unsupported,
        }
    }
}

/// A state the Verifier audits (§14.5): a pass by evidence, the user's
/// confirmation, or general knowledge.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuditedState {
    Verified,
    Confirmed,
    GeneralKnowledge,
}

impl AuditedState {
    /// Whether a Verifier failure of `kind` disputes a claim in this state.
    pub fn disputed_by(self, kind: &VerdictKind) -> bool {
        match self {
            Self::Verified => matches!(
                kind,
                VerdictKind::Unsupported
                    | VerdictKind::Contradicted
                    | VerdictKind::NoSource
                    | VerdictKind::CouldNotJudge
            ),
            Self::Confirmed => matches!(
                kind,
                VerdictKind::Unsupported | VerdictKind::Contradicted | VerdictKind::NoSource
            ),
            Self::GeneralKnowledge => *kind == VerdictKind::Contradicted,
        }
    }
}

impl ClaimState {
    /// Unchecked or Open: the machine has work left on this claim.
    pub fn is_machine_owned(&self) -> bool {
        matches!(self, Self::Unchecked | Self::Open { .. })
    }

    /// Queued or Reopened: only the user (or new information) moves it.
    pub fn awaits_user(&self) -> bool {
        matches!(self, Self::Queued { .. } | Self::Reopened { .. })
    }

    /// Closed in one of the ways a lesson may publish with (§2.1): verified,
    /// found consistent, passed as common knowledge, confirmed, removed,
    /// edited (the successor answers for it) or published not verified.
    pub fn is_resolved(&self) -> bool {
        !self.is_machine_owned() && !self.awaits_user()
    }

    /// A verdict no machine check changes: the user's, or a sentence that is
    /// gone. Only [`ClaimEvent::Reopened`], a Verifier dispute or the user
    /// moves it.
    pub fn is_settled(&self) -> bool {
        matches!(self, Self::Confirmed { .. }) || self.is_gone()
    }

    /// The claim's sentence is gone from the lesson: removed, or edited away.
    pub fn is_gone(&self) -> bool {
        matches!(self, Self::Removed { .. } | Self::Edited { .. })
    }

    /// Passed by the machine: verified by evidence, found consistent, or
    /// passed as common knowledge.
    pub fn is_verified(&self) -> bool {
        matches!(
            self,
            Self::ResolvedByEvidence { .. }
                | Self::Consistent { .. }
                | Self::GeneralKnowledge { .. }
        )
    }

    /// The Verifier disputed the claim and the user has not resolved it since.
    pub fn verifier_disputed(&self) -> bool {
        matches!(
            self,
            Self::Open {
                kind: VerdictKind::Disputed { .. },
                ..
            } | Self::Queued {
                kind: VerdictKind::Disputed { .. },
                ..
            } | Self::Reopened {
                cause: ReopenCause::VerifierDispute,
                ..
            }
        )
    }

    /// What the Verifier audits of this state; `None` for every state it does
    /// not.
    pub fn audited_state(&self) -> Option<AuditedState> {
        match self {
            Self::ResolvedByEvidence { .. } => Some(AuditedState::Verified),
            Self::Confirmed { .. } => Some(AuditedState::Confirmed),
            Self::GeneralKnowledge { .. } => Some(AuditedState::GeneralKnowledge),
            _ => None,
        }
    }

    /// Holds a decision of the user's: settled, or reopened from one. A queued
    /// claim, or a machine's check reopened, only waits for them.
    pub fn is_the_users_decision(&self) -> bool {
        match self {
            Self::Reopened { previous, .. } => previous.is_the_users_decision(),
            state => state.is_settled(),
        }
    }

    /// The failure an Open or Queued claim carries.
    pub fn verdict(&self) -> Option<&VerdictKind> {
        match self {
            Self::Open { kind, .. } | Self::Queued { kind, .. } => Some(kind),
            _ => None,
        }
    }
}
