//! What supports a claim the user brought evidence for (§5.4). The attach
//! runs outside a pass, often while the job is finished, so its check cannot
//! be saved as a job checkpoint, and a pass would select other passages and
//! miss it anyway. The ledger keeps the check instead, and it is the claim's
//! receipt: while the passages it compared are unchanged in the collection,
//! and the checker and its window are the pass's own (what a receipt key
//! names), every later pass takes that check's finding without asking the
//! model again. Otherwise the pass checks the claim as usual.
//! The lesson then cites the source on the claim's unit, with no rewrite.

use super::super::*;
use super::ClaimLedger;
use lattice_core::domain::learning::claim_review::{
    text_sha256, AttachOutcome, Check, CheckResult, ClaimEvent, ClaimLedgerEntry, ClaimState,
    EvidenceFingerprint, EvidenceSeen,
};

/// How a passage of evidence the user brought is labelled in the report:
/// the learner view shows it, and the claim's unit cites its source.
const USER_EVIDENCE: &str = "user_attached";

impl ClaimLedger {
    /// The findings of claims resolved by a check against a source the user
    /// attached to this lesson (the attached claim, and the other claims the
    /// attach checked against it), when that check still holds as a receipt.
    pub(in super::super) fn user_evidence(
        &self,
        references: &ReferenceCollection<'_>,
    ) -> HashMap<(usize, String), Finding> {
        let attached: HashSet<&str> = self
            .entries
            .iter()
            .flat_map(ClaimLedgerEntry::events)
            .filter_map(|record| match &record.event {
                ClaimEvent::EvidenceAttached {
                    source_version_id, ..
                } => Some(source_version_id.as_str()),
                _ => None,
            })
            .collect();
        if attached.is_empty() {
            return HashMap::new();
        }
        self.entries
            .iter()
            .filter_map(|entry| {
                let ClaimState::ResolvedByEvidence {
                    source_version_id,
                    supporting_quote,
                } = entry.state()
                else {
                    return None;
                };
                let check = deciding_check(entry, source_version_id)?;
                let receipt = check.window_tokens == self.window_tokens
                    && check.fingerprint
                        == EvidenceFingerprint::of(POLICY, &self.checker_model, &check.evidence)
                    && check
                        .evidence
                        .iter()
                        .all(|seen| attached.contains(seen.source_version_id.as_str()));
                if !receipt {
                    return None;
                }
                let evidence = check
                    .evidence
                    .iter()
                    .map(|seen| passage(references, seen))
                    .collect::<Option<Vec<_>>>()?;
                let location = entry.location();
                let statement = entry.statement().to_owned();
                Some((
                    (location.unit_index, statement.clone()),
                    Finding {
                        unit: location.unit_index,
                        quote: location.quote.clone(),
                        statement,
                        verdict: ClaimVerdict::Supported,
                        reason: check.reason.clone(),
                        evidence,
                        supporting_quote: Some(supporting_quote.clone()),
                        resolution: None,
                        trust: None,
                    },
                ))
            })
            .collect()
    }
}

/// The latest supported check naming `source_version_id`: the one that
/// resolved the claim.
fn deciding_check<'e>(entry: &'e ClaimLedgerEntry, source_version_id: &str) -> Option<&'e Check> {
    entry.events().iter().rev().find_map(|record| {
        let check = match &record.event {
            ClaimEvent::Checked(check)
            | ClaimEvent::EvidenceAttached {
                result: AttachOutcome::Checked(check),
                ..
            } => check,
            _ => return None,
        };
        matches!(
            &check.result,
            CheckResult::Supported { source_version_id: supported, .. } if supported == source_version_id
        )
        .then_some(check)
    })
}

/// The compared passage, read again from the collection: `None` when its
/// source is gone or its text changed.
fn passage(references: &ReferenceCollection<'_>, seen: &EvidenceSeen) -> Option<EvidencePassage> {
    let source = references
        .sources
        .iter()
        .find(|source| source.id == seen.source_version_id)?;
    let text = source.excerpt.get(seen.start_byte..seen.end_byte)?;
    (text_sha256(text) == seen.text_sha256).then(|| EvidencePassage {
        source_id: seen.source_version_id.clone(),
        text: text.to_owned(),
        start_byte: seen.start_byte,
        end_byte: seen.end_byte,
        retrieval_kind: USER_EVIDENCE.into(),
        score: 0.0,
        branch: None,
        branch_score: 0.0,
    })
}

impl ClaimChecks {
    /// The ledger's finding for a claim settled by the user's evidence.
    pub(in super::super) fn settled_finding(&self, unit: usize, claim: &Claim) -> Option<Finding> {
        self.settled
            .get(&(unit, claim.statement.clone()))
            .cloned()
            .map(|mut finding| {
                finding.quote = claim.quote.clone();
                finding
            })
    }
}

impl LessonVerificationReport {
    /// Cites, on each unit, the sources the user brought that support one of
    /// its claims (§5.4 "the unit's `source_ids` gains the source id"): a
    /// supported sentence is entailed as written, so only the citation
    /// changes. Called on the assembled lesson, before the report is bound
    /// to it.
    pub(in crate::features::learning) fn cite_user_evidence(
        &self,
        lesson: &mut PreparedLearningLesson,
    ) {
        let blocks = lesson.blocks.len();
        for finding in self
            .findings
            .iter()
            .filter(|finding| finding.verdict == ClaimVerdict::Supported)
        {
            let cited = if finding.unit < blocks {
                lesson
                    .blocks
                    .get_mut(finding.unit)
                    .map(|block| &mut block.source_ids)
            } else {
                lesson
                    .questions
                    .get_mut(finding.unit - blocks)
                    .map(|question| &mut question.source_ids)
            };
            let Some(cited) = cited else {
                continue;
            };
            for passage in finding
                .evidence
                .iter()
                .filter(|passage| passage.retrieval_kind == USER_EVIDENCE)
            {
                if !cited.contains(&passage.source_id) {
                    cited.push(passage.source_id.clone());
                }
            }
        }
    }
}
