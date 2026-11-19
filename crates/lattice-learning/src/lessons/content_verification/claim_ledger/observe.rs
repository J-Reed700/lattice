//! What a reading or a research round leaves in the ledger: the check a
//! reading observed, what research found for a claim (§3, §4).

use super::super::provenance_gate::{gate_reason, FindingTrust};
use super::super::readings::Read;
use super::super::*;
use super::ClaimLedger;
use crate::lessons::claim_reviews::repository::ShownPassage;
use lattice_core::domain::learning::acceptance::{AcceptancePolicy, Outcome, Reading, Step};
use lattice_core::domain::learning::claim_review::{
    cited_source_versions, Check, CheckResult, Checker, ClaimEvent, ClaimId, ClaimLedgerEntry,
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
    EvidenceSeen::of(
        &passage.source_id,
        passage.start_byte,
        passage.end_byte,
        &passage.text,
    )
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
pub(in crate::lessons::content_verification) struct Targets {
    /// The claims, in the pass's order.
    pub(in crate::lessons::content_verification) claims: Vec<ClaimId>,
    /// Each claim's latest finding.
    pub(in crate::lessons::content_verification) findings: Vec<Finding>,
    /// The hints the user gave, searched verbatim, each with the place in
    /// `findings` of the claim it names.
    pub(in crate::lessons::content_verification) hints: Vec<(usize, String)>,
    /// The claims a hint names. They are the user's: they take no step, and
    /// the round's page reading answers the hint.
    pub(in crate::lessons::content_verification) hinted: Vec<ClaimId>,
}

impl ClaimLedger {
    /// The check a reading records: its finding's verdict, which its outcome
    /// decided, passes or fails; a pass is named by the reading (`KnownCommon`,
    /// `NotFalse`, `Consistent`) or rests on the passage holding its quotation
    /// (`Supported`, `SupportedElsewhere` when the outcome is `StandsElsewhere`).
    pub(super) fn check_of_reading(
        &self,
        reading: Reading,
        read: &Read,
    ) -> Result<(Check, Option<TrustAssessment>)> {
        let finding = read.finding();
        let evidence: Vec<EvidenceSeen> = finding.evidence.iter().map(seen).collect();
        let ((result, reason), trust) = match finding.verdict.failure(!evidence.is_empty()) {
            Some(kind) => ((CheckResult::Failed { kind }, finding.reason.clone()), None),
            None => self.passing(reading, read)?,
        };
        let policy = Judged::of(reading).policy();
        Ok((self.check(policy, result, reason, evidence), trust))
    }

    /// The result of a finding that passed, named by the reading.
    fn passing(
        &self,
        reading: Reading,
        read: &Read,
    ) -> Result<((CheckResult, String), Option<TrustAssessment>)> {
        let reason = read.finding().reason.clone();
        Ok(match reading {
            Reading::Knowledge => ((CheckResult::KnownCommon, reason), None),
            Reading::Veto => ((CheckResult::NotFalse, reason), None),
            Reading::Consistency => ((CheckResult::Consistent, reason), None),
            Reading::Parts | Reading::Ground | Reading::Page => self.supported_check(read)?,
        })
    }

    /// The result of a supported finding and the gate's assessment when it
    /// weighed the support: the source is the passage holding the quotation,
    /// and the quotation recorded is its words of it.
    fn supported_check(
        &self,
        read: &Read,
    ) -> Result<((CheckResult, String), Option<TrustAssessment>)> {
        let finding = read.finding();
        let quote = finding.supporting_quote.as_deref().unwrap_or_default();
        let strongest = finding.trust.as_deref().and_then(FindingTrust::strongest);
        let (passage, words) =
            supporting_passage(quote, &finding.evidence, strongest).ok_or_else(|| {
                AppError::InternalError(format!(
                    "Claim \"{}\": its supported finding names no passage that holds its quotation",
                    finding.statement
                ))
            })?;
        let source = passage.source_id.clone();
        let trust = finding
            .trust
            .as_ref()
            .map(|trust| trust.assessment(&source, words));
        let result = if read.outcome() == Outcome::StandsElsewhere {
            CheckResult::SupportedElsewhere {
                cited: finding
                    .two_pass
                    .supported_by
                    .as_ref()
                    .map(|found| found.cited.clone())
                    .unwrap_or_default(),
                source_version_id: source,
                supporting_quote: words.to_owned(),
            }
        } else {
            CheckResult::Supported {
                source_version_id: source,
                supporting_quote: words.to_owned(),
            }
        };
        Ok(((result, finding.reason.clone()), trust))
    }

    /// The check an example's run records: it completed, or it did not.
    pub(super) fn example_check(&self, observation: &execution::Observation) -> Check {
        let passage = observation.passage();
        let result = if observation.passed() {
            CheckResult::Supported {
                source_version_id: passage.source_id.clone(),
                supporting_quote: observation.code().to_owned(),
            }
        } else {
            CheckResult::Failed {
                kind: VerdictKind::Unsupported,
            }
        };
        let policy = Judged::Passages.policy();
        self.check(policy, result, passage.text.clone(), vec![seen(&passage)])
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
        cited_source_versions(entry)
            .into_iter()
            .filter(|id| !execution::is_run(id) && !active.contains(id))
            .map(|id| SourceChange::Deleted(id.to_owned()))
            .collect()
    }

    /// The claims research looks for evidence for, with the hints the user
    /// gave: the statements at a `Research` node, which the policy sends once
    /// in a statement's life, and a hint once (§5.1). Each with its latest
    /// finding in `report`.
    pub(in crate::lessons::content_verification) fn research_targets(
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
    pub(in crate::lessons::content_verification) fn hinted_waiting(&self) -> Vec<super::Waiting> {
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
    pub(in crate::lessons::content_verification) async fn record_research(
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
