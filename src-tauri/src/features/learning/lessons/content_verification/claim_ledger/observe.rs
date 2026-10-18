//! Observing a pass: which claim each finding and failed example is, what is
//! new about it, and what research found for it (§3, §4 `ledger.observe`).

use super::super::support_gate::{Sufficiency, Support};
use super::super::*;
use super::{is_gone, ClaimLedger, RepairRecord};
use crate::features::learning::lessons::claim_reviews::repository::ShownPassage;
use lattice_core::domain::learning::claim_review::{
    identities, new_information, reopen_cause, text_sha256, Check, CheckResult, Checker,
    ClaimEvent, ClaimId, ClaimLedgerEntry, ClaimLocation, ClaimState, EvidenceFingerprint,
    EvidenceSeen, ExtractedClaim, NewInformation, Observation, RepairOutcome, SourceChange,
    UnitKind, VerdictKind,
};

/// The statement a code example's claim makes (§2.1: an example that cannot
/// run is a claim like any other).
fn example_statement(observation_id: &str) -> String {
    format!("The code example {observation_id} runs to completion with the output it shows.")
}

/// The text of a lesson field: `body`, `rubric/2/description`, `options/1`.
pub(in crate::features::learning) fn field_text(
    candidate: &Value,
    unit: usize,
    field: &str,
) -> Option<String> {
    let blocks = candidate.get("blocks")?.as_array()?.len();
    let pointer = if unit < blocks {
        format!("/blocks/{unit}/{field}")
    } else {
        format!("/questions/{}/{field}", unit - blocks)
    };
    candidate.pointer(&pointer)?.as_str().map(str::to_owned)
}

/// The fields of a unit a claim can come from, in the order searched.
fn unit_fields(item: &Value, block: bool) -> Vec<String> {
    let mut fields: Vec<String> = Vec::new();
    let names: &[&str] = if block {
        &["body", "title", "rubric"]
    } else {
        &["prompt", "options", "explanation"]
    };
    for name in names {
        match item.get(*name) {
            Some(Value::String(_)) => fields.push((*name).to_owned()),
            Some(Value::Array(items)) => {
                for (index, entry) in items.iter().enumerate() {
                    match entry {
                        Value::String(_) => fields.push(format!("{name}/{index}")),
                        Value::Object(object) => fields.extend(
                            object
                                .iter()
                                .filter(|(_, value)| value.is_string())
                                .map(|(key, _)| format!("{name}/{index}/{key}")),
                        ),
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }
    fields
}

/// Each unit of a lesson in the report's order (sections, then questions)
/// as `(title, text)`: a section's title or a question's prompt, and every
/// other field a claim can come from, joined. What the Verifier reads.
pub(in crate::features::learning) fn unit_texts(candidate: &Value) -> Vec<(String, String)> {
    let mut units = Vec::new();
    for (list, title_field, block) in [("blocks", "title", true), ("questions", "prompt", false)] {
        let items = candidate.get(list).and_then(Value::as_array);
        for item in items.into_iter().flatten() {
            let title = item
                .get(title_field)
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned();
            let text = unit_fields(item, block)
                .iter()
                .filter(|field| field.as_str() != title_field)
                .filter_map(|field| item.pointer(&format!("/{field}")).and_then(Value::as_str))
                .collect::<Vec<_>>()
                .join("\n\n");
            units.push((title, text));
        }
    }
    units
}

/// Where `quote` is in unit `unit`: the first field holding it.
pub(in crate::features::learning) fn locate(
    candidate: &Value,
    unit: usize,
    quote: &str,
) -> ClaimLocation {
    let blocks = candidate
        .get("blocks")
        .and_then(Value::as_array)
        .map_or(0, Vec::len);
    let block = unit < blocks;
    let item = if block {
        candidate.pointer(&format!("/blocks/{unit}"))
    } else {
        candidate.pointer(&format!("/questions/{}", unit - blocks))
    };
    let fields = item
        .map(|item| unit_fields(item, block))
        .unwrap_or_default();
    let found = fields.iter().find_map(|field| {
        field_text(candidate, unit, field)
            .filter(|text| text.contains(quote))
            .map(|text| (field.clone(), text))
    });
    let (field, text) = found.unwrap_or_else(|| {
        let field = if block { "body" } else { "prompt" };
        (
            field.to_owned(),
            field_text(candidate, unit, field).unwrap_or_default(),
        )
    });
    let kind = match (block, field.as_str()) {
        (true, _) => UnitKind::Block,
        (false, "explanation") => UnitKind::AnswerKey,
        (false, _) => UnitKind::Question,
    };
    ClaimLocation::locate(unit, kind, field, quote, &text)
}

fn seen(passage: &EvidencePassage) -> EvidenceSeen {
    EvidenceSeen {
        source_version_id: passage.source_id.clone(),
        start_byte: passage.start_byte,
        end_byte: passage.end_byte,
        text_sha256: text_sha256(&passage.text),
    }
}

fn shown(passage: &EvidencePassage) -> ShownPassage {
    ShownPassage {
        source_version_id: passage.source_id.clone(),
        start_byte: passage.start_byte,
        end_byte: passage.end_byte,
        text: passage.text.clone(),
        retrieval_kind: passage.retrieval_kind.clone(),
    }
}

impl ClaimLedger {
    /// The check a finding records, through the support gate (P4's seam).
    /// `propose_general_knowledge`: research has had its chance, so an
    /// unsupported claim without invented specifics is proposed as general
    /// knowledge (D1) rather than researched or repaired.
    fn check_of(&self, finding: &Finding, propose_general_knowledge: bool) -> Check {
        let evidence: Vec<EvidenceSeen> = finding.evidence.iter().map(seen).collect();
        let (result, reason) = match finding.verdict {
            ClaimVerdict::Supported => {
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
                    trust: finding.trust.as_ref(),
                };
                match self.gate.judge(&support) {
                    Sufficiency::Sufficient => (
                        CheckResult::Supported {
                            source_version_id: source,
                            supporting_quote: quote,
                        },
                        finding.reason.clone(),
                    ),
                    Sufficiency::Insufficient { reason } => (
                        CheckResult::Failed {
                            kind: VerdictKind::Unsupported,
                        },
                        reason,
                    ),
                    Sufficiency::Contradicted { reason } => (
                        CheckResult::Failed {
                            kind: VerdictKind::Contradicted,
                        },
                        reason,
                    ),
                }
            }
            verdict => {
                let kind = match verdict {
                    ClaimVerdict::Contradicted => VerdictKind::Contradicted,
                    ClaimVerdict::Unverified => VerdictKind::CouldNotJudge,
                    _ if propose_general_knowledge && finding.proposes_general_knowledge() => {
                        VerdictKind::GeneralKnowledgeProposed
                    }
                    _ if finding.evidence.is_empty() => VerdictKind::NoSource,
                    _ => VerdictKind::Unsupported,
                };
                (CheckResult::Failed { kind }, finding.reason.clone())
            }
        };
        self.check(result, reason, evidence)
    }

    /// The check an example's run records: it completed, or it did not.
    fn example_check(&self, observation: &execution::Observation) -> Check {
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
        self.check(result, evidence, seen)
    }

    fn check(&self, result: CheckResult, reason: String, evidence: Vec<EvidenceSeen>) -> Check {
        Check {
            checker: Checker::Local {
                model: self.checker_model.clone(),
            },
            result,
            reason,
            fingerprint: EvidenceFingerprint::of(POLICY, &self.checker_model, &evidence),
            evidence,
            window_tokens: self.window_tokens,
        }
    }

    /// Source versions a claim relied on that are no longer active.
    fn source_changes(entry: &ClaimLedgerEntry, active: &HashSet<&str>) -> Vec<SourceChange> {
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

    /// Records one pass: which claim each finding and failed example is, and
    /// what is new about it (§4 `ledger.observe`).
    pub(in crate::features::learning::lessons::content_verification) async fn observe(
        &mut self,
        candidate: &Value,
        report: &LessonVerificationReport,
    ) -> Result<()> {
        // What this pass found, each with its check and the passages shown.
        let mut found: Vec<(ExtractedClaim, Check, Vec<ShownPassage>)> = Vec::new();
        for finding in &report.findings {
            found.push((
                ExtractedClaim {
                    location: locate(candidate, finding.unit, &finding.quote),
                    statement: finding.statement.clone(),
                },
                self.check_of(finding, false),
                finding.evidence.iter().map(shown).collect(),
            ));
        }
        let mut example_ids = Vec::new();
        for observation in &report.executions {
            let statement = example_statement(&observation.id);
            let tracked = self
                .entries
                .iter()
                .any(|entry| entry.statement() == statement && !is_gone(entry.state()));
            if observation.passed() && !tracked {
                continue;
            }
            let check = self.example_check(observation);
            found.push((
                ExtractedClaim {
                    location: locate(candidate, observation.unit.unwrap_or(0), observation.code()),
                    statement,
                },
                check.clone(),
                vec![ShownPassage {
                    source_version_id: format!("execution:{}", observation.id),
                    start_byte: 0,
                    end_byte: check.reason.len(),
                    text: check.reason.clone(),
                    retrieval_kind: "execution".into(),
                }],
            ));
            example_ids.push(observation.id.clone());
        }

        let reference = self.pending.previous.clone();
        let reference = reference.as_ref().unwrap_or(candidate);
        let prior_ids: Vec<ClaimId> = self
            .entries
            .iter()
            .filter(|entry| !is_gone(entry.state()) || self.edited_now.contains(entry.claim_id()))
            .map(|entry| entry.claim_id().clone())
            .collect();
        let texts: HashMap<(usize, String), String> = prior_ids
            .iter()
            .filter_map(|id| self.entry(id))
            .filter_map(|entry| {
                let location = entry.location();
                field_text(reference, location.unit_index, &location.field)
                    .map(|text| ((location.unit_index, location.field.clone()), text))
            })
            .collect();
        let extracted: Vec<ExtractedClaim> =
            found.iter().map(|(claim, _, _)| claim.clone()).collect();
        let mut assigned = {
            let prior: Vec<&ClaimLedgerEntry> =
                prior_ids.iter().filter_map(|id| self.entry(id)).collect();
            identities(&prior, &extracted, |unit, field| {
                texts.get(&(unit, field.to_owned())).map(String::as_str)
            })
        };
        let repairs = std::mem::take(&mut self.pending.repairs);
        // A repaired sentence the matching rules lost (a rewrite that changed
        // its statement where no single span can be named) continues as the
        // new claim its repair left in the same field: the rewrite keeps the
        // claim's attempts, so a claim the machine keeps rewriting still
        // reaches the user (§4 "Bounded").
        for record in &repairs {
            let Some(position) = assigned
                .orphaned
                .iter()
                .position(|id| *id == record.claim_id)
            else {
                continue;
            };
            let field = self
                .entry(&record.claim_id)
                .map(|entry| entry.location().field.clone());
            let successor = extracted
                .iter()
                .zip(&assigned.assigned)
                .position(|(claim, owner)| {
                    owner.is_none()
                        && claim.location.unit_index == record.unit
                        && Some(&claim.location.field) == field.as_ref()
                });
            if let Some(slot) = successor.and_then(|index| assigned.assigned.get_mut(index)) {
                *slot = Some(assigned.orphaned.remove(position));
            }
        }
        let records: HashMap<ClaimId, RepairRecord> = repairs
            .into_iter()
            .map(|record| (record.claim_id.clone(), record))
            .collect();
        let active: HashSet<&str> = report.sources.iter().map(|s| s.id.as_str()).collect();
        let mut claim_ids = Vec::with_capacity(found.len());
        for ((claim, check, passages), prior) in found.into_iter().zip(assigned.assigned) {
            let edited = prior.as_ref().is_some_and(|id| self.edited_now.remove(id));
            let id = match prior {
                Some(id) if edited => {
                    let successor = ClaimId::generate();
                    self.append(
                        &id,
                        ClaimEvent::Superseded {
                            successor: Some(successor.clone()),
                        },
                    )
                    .await?;
                    self.start(&successor, claim, check).await?;
                    successor
                }
                Some(id) => {
                    self.carry_on(&id, candidate, claim, check, records.get(&id), &active)
                        .await?;
                    id
                }
                None => {
                    let id = ClaimId::generate();
                    self.start(&id, claim, check).await?;
                    id
                }
            };
            self.show(&id, &passages).await?;
            claim_ids.push(id);
        }
        for id in assigned.orphaned {
            if self.edited_now.contains(&id) {
                continue;
            }
            let event = match records.get(&id) {
                // The repair took the sentence out (§4: undo restores it).
                Some(record) => ClaimEvent::RepairAttempted {
                    attempt: record.attempt,
                    escalated: record.escalated,
                    before: record.before.clone(),
                    after: None,
                    outcome: RepairOutcome::Removed,
                },
                None => ClaimEvent::Superseded { successor: None },
            };
            self.append(&id, event).await?;
        }
        // An edited claim no re-extracted claim continues: removed by the edit.
        for id in std::mem::take(&mut self.edited_now) {
            self.append(&id, ClaimEvent::Superseded { successor: None })
                .await?;
        }
        self.pending.previous = None;
        if self.sink.is_some() {
            self.save_pending().await?;
        }
        let examples = claim_ids.split_off(report.findings.len());
        self.examples = example_ids.into_iter().zip(examples).collect();
        self.findings = claim_ids;
        crate::features::learning::lesson_progress::claims(self.tally());
        Ok(())
    }

    /// A claim seen for the first time: extracted, then checked.
    async fn start(&mut self, id: &ClaimId, claim: ExtractedClaim, check: Check) -> Result<()> {
        self.append(
            id,
            ClaimEvent::Extracted {
                location: claim.location,
                statement: claim.statement,
            },
        )
        .await?;
        self.append(id, ClaimEvent::Checked(check)).await?;
        Ok(())
    }

    /// A claim seen before: record what changed, then a check only when
    /// something about it is new or a repair awaits its verdict.
    async fn carry_on(
        &mut self,
        id: &ClaimId,
        candidate: &Value,
        claim: ExtractedClaim,
        check: Check,
        repair: Option<&RepairRecord>,
        active: &HashSet<&str>,
    ) -> Result<()> {
        let Some(entry) = self.entry(id).cloned() else {
            return self.start(id, claim, check).await;
        };
        let observation = Observation {
            quote: claim.location.quote.clone(),
            fingerprint: check.fingerprint.clone(),
            checker_model: self.checker_model.clone(),
            window_tokens: self.window_tokens,
            source_changes: Self::source_changes(&entry, active),
            hint: None,
            attached_source_version_id: None,
        };
        let mut information = new_information(&entry, &observation);
        // A confirmation is bound to its sentence, not to the quote an
        // extraction picked: while the bound sentence is still in its field,
        // nothing about it changed (§3 `relitigate`).
        if matches!(information, Some(NewInformation::SentenceChanged { .. }))
            && field_text(
                candidate,
                entry.location().unit_index,
                &entry.location().field,
            )
            .is_some_and(|text| text.contains(&entry.location().quote))
            && matches!(entry.state(), ClaimState::Confirmed { .. })
        {
            information = None;
        }
        let quote = claim.location.quote.clone();
        if claim.location != *entry.location() || claim.statement != entry.statement() {
            self.append(
                id,
                ClaimEvent::Extracted {
                    location: claim.location,
                    statement: claim.statement,
                },
            )
            .await?;
        }
        let mut decides_repair = false;
        if let Some(record) = repair {
            decides_repair = self
                .append(
                    id,
                    ClaimEvent::RepairAttempted {
                        attempt: record.attempt,
                        escalated: record.escalated,
                        before: record.before.clone(),
                        after: Some(quote),
                        outcome: RepairOutcome::Rewritten,
                    },
                )
                .await?;
        }
        if let Some(change) = &information {
            let cause = self
                .entry(id)
                .and_then(|entry| reopen_cause(entry.state(), change));
            if let Some(cause) = cause {
                self.append(id, ClaimEvent::Reopened { cause }).await?;
            }
        }
        let unchecked = matches!(entry.state(), ClaimState::Unchecked);
        if unchecked || decides_repair || information.is_some() {
            self.append(id, ClaimEvent::Checked(check)).await?;
        }
        Ok(())
    }

    /// The claims research may look for evidence for, with the hints the user
    /// gave: once per claim and evidence (§4), and a hint once (§5.1).
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
            if entry.may_research() || hint.is_some() {
                claims.push(id.clone());
                findings.push(finding.clone());
                hints.extend(hint);
            }
        }
        (claims, findings, hints)
    }

    /// Records a research round for each claim it looked for (§3
    /// `Researched`): "found nothing new" is an attempt with no passages.
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
                },
            )
            .await?;
        }
        Ok(())
    }

    /// Once research has had its chance, an unsupported claim without
    /// invented specifics is proposed as general knowledge and waits for the
    /// user; it is never repaired (D1, §13).
    pub(in crate::features::learning::lessons::content_verification) async fn propose_general_knowledge(
        &mut self,
        report: &LessonVerificationReport,
    ) -> Result<()> {
        let proposals: Vec<(ClaimId, Check)> = report
            .findings
            .iter()
            .zip(&self.findings)
            .filter(|(finding, _)| finding.proposes_general_knowledge())
            .filter_map(|(finding, id)| {
                let entry = self.entry(id)?;
                let researched = !entry.may_research()
                    && matches!(
                        entry.state().verdict(),
                        Some(VerdictKind::Unsupported | VerdictKind::NoSource)
                    );
                researched.then(|| (id.clone(), self.check_of(finding, true)))
            })
            .collect();
        for (id, check) in proposals {
            self.append(&id, ClaimEvent::Checked(check)).await?;
        }
        Ok(())
    }
}
