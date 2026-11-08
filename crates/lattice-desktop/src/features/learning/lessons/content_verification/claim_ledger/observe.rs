//! What a reading or a research round leaves in the ledger: the check a
//! reading observed, what research found for a claim (§3, §4).

use super::super::provenance_gate::gate_reason;
use super::super::support_gate::{Sufficiency, Support};
use super::super::*;
use super::ClaimLedger;
use crate::features::learning::lessons::claim_reviews::repository::ShownPassage;
use lattice_core::application::services::claim_verification::{
    CONSISTENCY_POLICY, KNOWLEDGE_POLICY,
};
use lattice_core::domain::learning::acceptance::{AcceptancePolicy, Outcome, Reading, Step};
use lattice_core::domain::learning::claim_review::{
    text_sha256, Check, CheckResult, Checker, ClaimEvent, ClaimId, ClaimLedgerEntry, ClaimState,
    EvidenceFingerprint, EvidenceSeen, SourceChange, TrustAssessment, VerdictKind,
};

/// The ledger's record of the gate's weighing, after the check it decided.
pub(super) fn assessed(trust: TrustAssessment) -> ClaimEvent {
    ClaimEvent::StandingAssessed {
        reason: gate_reason(&trust),
        trust: Box::new(trust),
        cause: None,
    }
}

fn seen(passage: &EvidencePassage) -> EvidenceSeen {
    EvidenceSeen {
        source_version_id: passage.source_id.clone(),
        start_byte: passage.start_byte,
        end_byte: passage.end_byte,
        text_sha256: text_sha256(&passage.text),
    }
}

pub(super) fn shown(passage: &EvidencePassage) -> ShownPassage {
    ShownPassage {
        source_version_id: passage.source_id.clone(),
        start_byte: passage.start_byte,
        end_byte: passage.end_byte,
        text: passage.text.clone(),
        retrieval_kind: passage.retrieval_kind.clone(),
    }
}

/// The claims a research round looks for evidence for.
pub(in crate::features::learning::lessons::content_verification) struct Targets {
    /// The claims, in the pass's order.
    pub(in crate::features::learning::lessons::content_verification) claims: Vec<ClaimId>,
    /// Each claim's latest finding.
    pub(in crate::features::learning::lessons::content_verification) findings: Vec<Finding>,
    /// The hints the user gave, searched verbatim, each with the place in
    /// `findings` of the claim it names.
    pub(in crate::features::learning::lessons::content_verification) hints: Vec<(usize, String)>,
    /// The claims a hint names. They are the user's: they take no step, and
    /// the round's page reading answers the hint.
    pub(in crate::features::learning::lessons::content_verification) hinted: Vec<ClaimId>,
}

/// The failure a finding that is not supported records: what its verdict
/// says, or that no passage was compared, or that none supports it.
fn failed_check(finding: &Finding, verdict: ClaimVerdict) -> (CheckResult, String) {
    let kind = match verdict {
        ClaimVerdict::Contradicted => VerdictKind::Contradicted,
        ClaimVerdict::Unverified => VerdictKind::CouldNotJudge,
        _ if finding.evidence.is_empty() => VerdictKind::NoSource,
        _ => VerdictKind::Unsupported,
    };
    (CheckResult::Failed { kind }, finding.reason.clone())
}

impl ClaimLedger {
    /// The check a reading records, named by the reading and not by the
    /// class that was read: a finding that stands passes as its reading
    /// passes a statement (a veto that stands is `NotFalse`, never the
    /// unanimity `KnownCommon` records), any other fails with the kind its
    /// verdict names, and its fingerprint follows the reading's policy. With
    /// the gate's assessment when the gate decided the check.
    pub(super) fn check_of_reading(
        &self,
        reading: Reading,
        finding: &Finding,
    ) -> (Check, Option<TrustAssessment>) {
        let evidence: Vec<EvidenceSeen> = finding.evidence.iter().map(seen).collect();
        let ((result, reason), trust) = match (finding.verdict, reading) {
            (ClaimVerdict::Supported, Reading::Knowledge) => {
                ((CheckResult::KnownCommon, finding.reason.clone()), None)
            }
            (ClaimVerdict::Supported, Reading::Veto) => {
                ((CheckResult::NotFalse, finding.reason.clone()), None)
            }
            (ClaimVerdict::Supported, Reading::Consistency) => {
                ((CheckResult::Consistent, finding.reason.clone()), None)
            }
            (ClaimVerdict::Supported, Reading::Parts | Reading::Ground | Reading::Page) => {
                self.supported_check(finding)
            }
            (verdict, _) => (failed_check(finding, verdict), None),
        };
        let policy = match reading {
            Reading::Consistency => CONSISTENCY_POLICY,
            Reading::Knowledge | Reading::Veto => KNOWLEDGE_POLICY,
            Reading::Parts | Reading::Ground | Reading::Page => POLICY,
        };
        (self.check(policy, result, reason, evidence), trust)
    }

    /// The result of a supported finding through the support gate, and the
    /// gate's assessment when it weighed the support: the source is the one
    /// whose passage holds the quotation, else the first passage.
    fn supported_check(
        &self,
        finding: &Finding,
    ) -> ((CheckResult, String), Option<TrustAssessment>) {
        let quote = finding.supporting_quote.clone().unwrap_or_default();
        let source = finding
            .evidence
            .iter()
            .find(|passage| !quote.is_empty() && passage.text.contains(&quote))
            .or_else(|| finding.evidence.first())
            .map(|passage| passage.source_id.clone())
            .unwrap_or_default();
        let support = Support {
            claim: &finding.statement,
            source_version_id: &source,
            supporting_quote: &quote,
            passages: &finding.evidence,
        };
        let trust = finding
            .trust
            .as_ref()
            .map(|trust| trust.assessment(&source, &quote));
        // Support found on a page of the course when the section's
        // citations gave none: the claim stands on the found passage, and
        // the ledger says its citation was elsewhere.
        let result = match &finding.two_pass.supported_by {
            Some(found) => CheckResult::SupportedElsewhere {
                cited: found.cited.clone(),
                source_version_id: source.clone(),
                supporting_quote: quote.clone(),
            },
            None => CheckResult::Supported {
                source_version_id: source.clone(),
                supporting_quote: quote.clone(),
            },
        };
        match self.gate.judge(&support) {
            Sufficiency::Sufficient => ((result, finding.reason.clone()), trust),
            // A quotation that is not in its passage is a plain
            // unsupported claim.
            Sufficiency::Insufficient { reason } => (
                (
                    CheckResult::Failed {
                        kind: VerdictKind::Unsupported,
                    },
                    reason,
                ),
                None,
            ),
        }
    }

    /// The check an example's run records: it completed, or it did not.
    pub(super) fn example_check(&self, observation: &execution::Observation) -> Check {
        let source = format!("execution:{}", observation.id);
        let evidence = observation.evidence();
        let seen = vec![EvidenceSeen {
            source_version_id: source.clone(),
            start_byte: 0,
            end_byte: evidence.len(),
            text_sha256: text_sha256(&evidence),
        }];
        let result = if observation.passed() {
            CheckResult::Supported {
                source_version_id: source,
                supporting_quote: observation.code().to_owned(),
            }
        } else {
            CheckResult::Failed {
                kind: VerdictKind::Unsupported,
            }
        };
        self.check(POLICY, result, evidence, seen)
    }

    fn check(
        &self,
        policy: &str,
        result: CheckResult,
        reason: String,
        evidence: Vec<EvidenceSeen>,
    ) -> Check {
        Check {
            checker: Checker::Local {
                model: self.checker_model.clone(),
            },
            result,
            reason,
            fingerprint: EvidenceFingerprint::of(policy, &self.checker_model, &evidence),
            evidence,
            window_tokens: self.window_tokens,
        }
    }

    /// Source versions a claim relied on that are no longer active.
    pub(super) fn source_changes(
        entry: &ClaimLedgerEntry,
        active: &HashSet<&str>,
    ) -> Vec<SourceChange> {
        let mut cited: Vec<String> = entry
            .basis()
            .map(|basis| basis.source_version_ids.iter().cloned().collect())
            .unwrap_or_default();
        let mut state = entry.state();
        if let ClaimState::Reopened { previous, .. } = state {
            state = previous;
        }
        match state {
            ClaimState::ResolvedByEvidence {
                source_version_id, ..
            } => cited.push(source_version_id.clone()),
            ClaimState::Confirmed { bound, .. } => {
                cited.extend(bound.source_version_ids.iter().cloned())
            }
            _ => {}
        }
        cited.sort();
        cited.dedup();
        cited
            .into_iter()
            .filter(|id| !id.starts_with("execution:") && !active.contains(id.as_str()))
            .map(SourceChange::Deleted)
            .collect()
    }

    /// The claims research looks for evidence for, with the hints the user
    /// gave: the statements at a `Research` node, which the policy sends once
    /// in a statement's life, and a hint once (§5.1). Each with its latest
    /// finding in `report`.
    pub(in crate::features::learning::lessons::content_verification) fn research_targets(
        &self,
        policy: &dyn AcceptancePolicy,
        report: &LessonVerificationReport,
    ) -> Targets {
        let at_research = self.at(policy, super::position::StepKind::Research);
        let mut targets = Targets {
            claims: Vec::new(),
            findings: Vec::new(),
            hints: Vec::new(),
            hinted: Vec::new(),
        };
        for (finding, id) in report.findings.iter().zip(&self.findings) {
            let Some(entry) = self.entry(id) else {
                continue;
            };
            let hint = super::super::review_pass::pending_hint(entry);
            if at_research.contains(id) || hint.is_some() {
                if let Some(hint) = hint {
                    targets.hints.push((targets.findings.len(), hint));
                    targets.hinted.push(id.clone());
                }
                targets.claims.push(id.clone());
                targets.findings.push(finding.clone());
            }
        }
        targets
    }

    /// The claims whose user's hint no research round has run with yet, as
    /// statements waiting for a source.
    pub(in crate::features::learning::lessons::content_verification) fn hinted_waiting(
        &self,
    ) -> Vec<super::Waiting> {
        self.entries
            .iter()
            .filter(|entry| super::super::review_pass::pending_hint(entry).is_some())
            .map(|entry| super::Waiting {
                claim: entry.claim_id().clone(),
                node: "hint",
                step: Step::Research,
            })
            .collect()
    }

    /// Records a research round for each claim it looked for (§3
    /// `Researched`): "found nothing new" is an attempt with no passages. Each
    /// claim's event carries what the round cost per claim (D5) and the
    /// passages `found` for it, from the sources the round saved. Then, for a
    /// claim at a `Research` node, the policy's step: `Saved` when the round
    /// saved a source, `FoundNothing` otherwise. A claim a hint named has no
    /// node to step.
    pub(in crate::features::learning::lessons::content_verification) async fn record_research(
        &mut self,
        policy: &dyn AcceptancePolicy,
        claims: &[(ClaimId, usize)],
        searched: &research::Searched,
        saved: bool,
    ) -> Result<()> {
        let outcome = if saved {
            Outcome::Saved
        } else {
            Outcome::FoundNothing
        };
        for (id, passages_found) in claims {
            self.append(
                id,
                ClaimEvent::Researched {
                    queries: searched.queries.clone(),
                    sources_added: searched.sources_added.clone(),
                    passages_found: *passages_found,
                    round_ms_per_claim: searched
                        .per_claim
                        .map(|cost| u64::try_from(cost.as_millis()).unwrap_or(u64::MAX)),
                },
            )
            .await?;
            self.record_step(policy, id, Step::Research, outcome)
                .await?;
        }
        Ok(())
    }
}
