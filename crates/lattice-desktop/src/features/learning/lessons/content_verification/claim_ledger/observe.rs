//! Observing a pass: the checks a finding or a failed example records, and
//! what research found for a claim (§3, §4 `ledger.observe`). `observe` is
//! `identify` and then `record_checks`.

use super::super::provenance_gate::gate_reason;
use super::super::support_gate::{Sufficiency, Support};
use super::super::*;
use super::ClaimLedger;
use crate::features::learning::lessons::claim_reviews::repository::ShownPassage;
use lattice_core::application::services::claim_verification::{
    CONSISTENCY_POLICY, KNOWLEDGE_POLICY,
};
use lattice_core::domain::learning::acceptance::Reading;
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
    /// The check a finding records, through the support gate (P4's seam),
    /// with the gate's assessment when the gate decided the check (a
    /// supported claim whose support it weighed).
    pub(super) fn check_of(&self, finding: &Finding) -> (Check, Option<TrustAssessment>) {
        let evidence: Vec<EvidenceSeen> = finding.evidence.iter().map(seen).collect();
        let class = finding.class.map(|classed| classed.class);
        let ((result, reason), trust) = match finding.verdict {
            // A statement the lighter checks passed rests on no quotation.
            ClaimVerdict::Supported if class == Some(Class::Common) => {
                ((CheckResult::KnownCommon, finding.reason.clone()), None)
            }
            ClaimVerdict::Supported
                if class == Some(Class::Reasoning) && finding.supporting_quote.is_none() =>
            {
                ((CheckResult::Consistent, finding.reason.clone()), None)
            }
            ClaimVerdict::Supported => self.supported_check(finding),
            verdict => (failed_check(finding, verdict), None),
        };
        let policy = match class {
            Some(Class::Reasoning) => CONSISTENCY_POLICY,
            Some(Class::Common) => KNOWLEDGE_POLICY,
            _ => POLICY,
        };
        (self.check(policy, result, reason, evidence), trust)
    }

    /// The check a reading records, named by the reading and not by the
    /// class that was read: a finding that stands passes as its reading
    /// passes a statement (a veto that stands is `NotFalse`, never the
    /// unanimity `KnownCommon` records), any other fails as `check_of` fails
    /// it, and its fingerprint follows the reading's policy. With the gate's
    /// assessment when the gate decided the check.
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
        // Support found in pass B when the section's citations gave
        // none: the claim stands on the found passage, and the
        // ledger says its citation was elsewhere.
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

    /// Records one pass: which claim each finding and failed example is, what
    /// is new about it, and what the pass checked (§4 `ledger.observe`).
    pub(in crate::features::learning::lessons::content_verification) async fn observe(
        &mut self,
        candidate: &Value,
        report: &LessonVerificationReport,
    ) -> Result<()> {
        let extracted = self.extracted(candidate, report);
        let matched = self.match_pass(candidate, &extracted);
        let ids = self.identify(candidate, extracted, matched).await?;
        self.record_checks(candidate, report, &ids).await
    }

    /// Records the checks `report` made of `claims`, which the ledger holds
    /// verified and that pass could not support: the report a lesson
    /// publishes and its ledger say the same of every claim (§4 `decide`).
    pub(in crate::features::learning::lessons::content_verification) async fn reconcile(
        &mut self,
        report: &LessonVerificationReport,
        claims: &[ClaimId],
    ) -> Result<()> {
        let checks: Vec<(ClaimId, Check, Option<TrustAssessment>)> = report
            .findings
            .iter()
            .zip(&self.findings)
            .filter(|(_, id)| claims.contains(id))
            .map(|(finding, id)| {
                let (check, trust) = self.check_of(finding);
                (id.clone(), check, trust)
            })
            .collect();
        for (id, check, trust) in checks {
            tracing::warn!(claim = %id.0, reason = %check.reason, "claim.state: the last pass could not support a verified claim; its check applies before the lesson publishes");
            self.append(&id, ClaimEvent::Checked(check)).await?;
            if let Some(trust) = trust {
                self.append(&id, assessed(trust)).await?;
            }
        }
        Ok(())
    }

    /// The claims research may look for evidence for, with the hints the user
    /// gave: once per claim for its life (§4), and a hint once (§5.1).
    pub(in crate::features::learning::lessons::content_verification) fn research_targets(
        &self,
        report: &LessonVerificationReport,
    ) -> (Vec<ClaimId>, Vec<Finding>, Vec<String>) {
        let mut claims = Vec::new();
        let mut findings = Vec::new();
        let mut hints = Vec::new();
        for (finding, id) in report.findings.iter().zip(&self.findings) {
            let Some(entry) = self.entry(id) else {
                continue;
            };
            let hint = super::super::review_pass::pending_hint(entry);
            // Only a statement that needs a source is looked up on the web.
            let sourced = entry.class().is_none_or(|class| class == Class::Sourced);
            if (sourced && entry.may_research()) || hint.is_some() {
                claims.push(id.clone());
                findings.push(finding.clone());
                hints.extend(hint);
            }
        }
        (claims, findings, hints)
    }

    /// Records a research round for each claim it looked for (§3
    /// `Researched`): "found nothing new" is an attempt with no passages.
    /// Each claim's event carries what the round cost per claim (D5).
    pub(in crate::features::learning::lessons::content_verification) async fn record_research(
        &mut self,
        claims: &[ClaimId],
        searched: &research::Searched,
        report: &LessonVerificationReport,
    ) -> Result<()> {
        for id in claims {
            let passages_found = report
                .findings
                .iter()
                .zip(&self.findings)
                .find(|(_, claim)| *claim == id)
                .map_or(0, |(finding, _)| {
                    finding
                        .evidence
                        .iter()
                        .filter(|passage| searched.sources_added.contains(&passage.source_id))
                        .count()
                });
            self.append(
                id,
                ClaimEvent::Researched {
                    queries: searched.queries.clone(),
                    sources_added: searched.sources_added.clone(),
                    passages_found,
                    round_ms_per_claim: searched
                        .per_claim
                        .map(|cost| u64::try_from(cost.as_millis()).unwrap_or(u64::MAX)),
                },
            )
            .await?;
        }
        Ok(())
    }
}
