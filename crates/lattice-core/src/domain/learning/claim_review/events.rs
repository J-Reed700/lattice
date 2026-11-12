//! A claim's append-only audit trail: the events its state is replayed from.
use super::*;

/// One entry of a claim's append-only audit trail.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ClaimEvent {
    Extracted {
        location: ClaimLocation,
        statement: String,
    },
    Checked(Check),
    Researched {
        queries: Vec<String>,
        /// Source version ids the research saved.
        sources_added: Vec<String>,
        passages_found: usize,
        /// What the round cost per claim it researched, in milliseconds:
        /// its time from start to result over its claims: the cost of
        /// researching one claim on the web (course evidence index D5).
        /// `None` when no round ran, and on events written before rounds were
        /// timed.
        #[serde(default)]
        round_ms_per_claim: Option<u64>,
    },
    RepairAttempted {
        attempt: u8,
        escalated: bool,
        before: String,
        after: Option<String>,
        outcome: RepairOutcome,
        /// What the repair was asked to achieve (acceptance design §2.9);
        /// `None` on repairs recorded before the policy named a goal.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        goal: Option<Goal>,
    },
    Executed {
        observation_id: String,
        passed: bool,
        evidence: String,
    },
    /// The machine moved the claim's section citation to the passage that
    /// supports the claim, found elsewhere in the course (course evidence
    /// index §5.1). The claim's words are unchanged, so no repair attempt is
    /// charged; the next check reads the new citation as the section's own.
    CitationRepaired {
        /// The source versions the section cited, none of which supported it.
        cited: Vec<String>,
        /// The source version it cites now, and the quotation it carries.
        source_version_id: String,
        quote: String,
        /// The lesson did not number the source before: it was added to the
        /// lesson's sources to be cited.
        added_source: bool,
    },
    /// The machine dropped the sentence's citation: the claim's parts are all
    /// common and the passages it cited state none (acceptance design §2.1).
    /// The claim's words are unchanged, so no repair attempt is charged; the
    /// next check reads it without the citation.
    CitationDropped {
        /// The source versions the sentence cited.
        cited: Vec<String>,
    },
    UserAction {
        action: UserAction,
        note: Option<String>,
    },
    EvidenceAttached {
        source_version_id: String,
        provenance: Provenance,
        result: AttachOutcome,
    },
    /// Re-extraction found the claim's successor (`None`: no successor, the
    /// sentence went with the change). `by`: whose change, the user's edit
    /// or the machine's repair; the machine's with no successor removes the
    /// claim, as it does a claim on a sentence cut for another's failure.
    Superseded {
        successor: Option<ClaimId>,
        #[serde(default)]
        by: Actor,
    },
    /// The claim was born of a repair of `parent` (claim sources design
    /// §9.1, acceptance design §2.6): a statement the rewrite or narrowing
    /// wrote beside the parent's successor. It begins at the post-rewrite node
    /// of its class with the parent's successor's rewrites (`attempts_left`),
    /// so a split sentence adds no budget.
    BornOfRepair {
        parent: ClaimId,
        attempts_left: u8,
    },
    Reopened {
        cause: ReopenCause,
    },
    /// The statement's class and who decided it (claim classes design §3.1).
    Classified {
        class: Class,
        by: ClassifiedBy,
    },
    Undone {
        event_ordinal: u32,
    },
    Stopped {
        reason: StopReason,
    },
    /// What the provenance gate weighed for the claim. With no `cause` it
    /// follows the local check the gate decided; with one it is the gate's
    /// re-decision after a trust change, made without a model call.
    /// `reason` is the gate's explanation, the claim's reason when it fails.
    StandingAssessed {
        trust: Box<TrustAssessment>,
        reason: String,
        cause: Option<TrustCause>,
    },
    /// What the acceptance policy made of one machine step, recorded beside
    /// the step's own event (acceptance design §2.9). A machine-owned claim's
    /// state is read from its latest decision.
    Stepped {
        /// The policy's version, [`acceptance::STANDARD_POLICY`] for the
        /// standard one. A claim is never continued under another.
        policy: String,
        step: Step,
        outcome: acceptance::Outcome,
        /// Where the policy sent the statement.
        then: Then,
        /// The step's own reason, which the claim shows while it is open.
        reason: String,
        /// A rewrite is reachable from `then`: the Open state's attempt left.
        rewrite_ahead: bool,
    },
    /// The machine has done all it will do for the claim and the lesson
    /// publishes with it, not verified.
    PublishedUnverified {
        reason: UnverifiedReason,
    },
    /// What the machine's cut took out of the lesson for this claim, and the
    /// sentences it stood between, so the user can put it back where it was.
    SentenceCut {
        cut: CutPlace,
    },
    /// What the user's removal took out of a published section: the exact
    /// text (the sentence with the whitespace beside it) and the shortest
    /// texts that occur once in what was left and end (`follows`) or start
    /// (`precedes`) where it was taken from, `None` at the start or the end
    /// of the section, so an undo can write it back only where they still
    /// meet.
    TextRemoved {
        text: String,
        follows: Option<String>,
        precedes: Option<String>,
    },
    /// The user's edit of the claim's whole section ended it with the user
    /// event that follows: editing the section again takes that back, an
    /// undo of the claim alone does not.
    SectionEdited {},
    /// The claim stands for the run of the code example `observation`.
    CodeExample {
        observation: String,
    },
    /// A fault in the machine's handling of the statement; it holds the
    /// statement back and moves no state.
    Faulted {
        stage: FaultStage,
        message: String,
    },
}

/// Where in a statement's handling a fault happened.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FaultStage {
    Read,
    Take,
    Ask,
    Record,
}

impl ClaimEvent {
    /// The events table's `kind` column: the serde tag.
    pub fn kind_name(&self) -> &'static str {
        match self {
            Self::Extracted { .. } => "extracted",
            Self::Checked(_) => "checked",
            Self::Researched { .. } => "researched",
            Self::RepairAttempted { .. } => "repair_attempted",
            Self::Executed { .. } => "executed",
            Self::CitationRepaired { .. } => "citation_repaired",
            Self::CitationDropped { .. } => "citation_dropped",
            Self::UserAction { .. } => "user_action",
            Self::EvidenceAttached { .. } => "evidence_attached",
            Self::Superseded { .. } => "superseded",
            Self::BornOfRepair { .. } => "born_of_repair",
            Self::Reopened { .. } => "reopened",
            Self::Classified { .. } => "classified",
            Self::Undone { .. } => "undone",
            Self::Stopped { .. } => "stopped",
            Self::StandingAssessed { .. } => "standing_assessed",
            Self::Stepped { .. } => "stepped",
            Self::PublishedUnverified { .. } => "published_unverified",
            Self::SentenceCut { .. } => "sentence_cut",
            Self::TextRemoved { .. } => "text_removed",
            Self::SectionEdited { .. } => "section_edited",
            Self::CodeExample { .. } => "code_example",
            Self::Faulted { .. } => "statement_fault",
        }
    }
}
