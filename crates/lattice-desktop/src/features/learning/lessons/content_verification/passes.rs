//! One verification pass over a candidate lesson (acceptance design §2.10):
//! what it extracts (the claim inventory and coverage audit, reused per
//! unchanged section, and the runs of the executed examples), which claim of
//! the ledger each statement is, the reading of each statement that waits for
//! one along its graph, and the report of what the latest readings found.
use super::claim_ledger::ClaimLedger;
use super::readings::driver::{read_page, read_until_remedy, ReadSteps};
use super::readings::{Read, ReadContext, ReaderSet, Statement};
use super::*;
use crate::features::learning::lessons::claim_reviews::repository::ShownPassage;
use lattice_core::domain::learning::acceptance::{AcceptancePolicy, Position, Walked};
use lattice_core::domain::learning::claim_review::{
    CheckResult, Checker, ClaimEvent, ClaimId, ClaimLedgerEntry, ClaimState, VerdictKind,
};

/// What a pass extracts from a candidate lesson before anything is read.
pub(super) struct Extraction {
    pub(super) inventory: Inventory,
    pub(super) coverage: Coverage,
    pub(super) executions: Vec<execution::Observation>,
}

/// A statement of the lesson that is a claim of the ledger.
pub(super) struct Identified {
    pub(super) id: ClaimId,
    /// The section it stands in.
    pub(super) unit: usize,
    pub(super) claim: Claim,
}

/// A pass identified with the ledger: what it extracted and which of its
/// statements are claims.
pub(super) struct Pass {
    pub(super) extraction: Extraction,
    /// The statements of the sections that stay, in the inventory's order,
    /// and whether the rule layer makes each Internal: such a statement is
    /// left out of its claims.
    left: Vec<bool>,
    /// The claims of the lesson's statements, in the inventory's order.
    pub(super) statements: Vec<Identified>,
}

/// What the readings of a round made of the statements: the steps of each.
pub(super) type Readings = Vec<(ClaimId, ReadSteps)>;

/// The checkpoint holding a statement's latest reading's finding, by claim.
fn finding_key(id: &ClaimId) -> String {
    format!("claim-finding-v1:{}", id.0)
}

/// The claim inventory and coverage audit of `candidate` and the runs of its
/// examples: extraction alone, before any statement is read.
pub(super) async fn extract(
    llm: &dyn LLMPort,
    runtimes: &BuiltinRuntimes,
    candidate: &Value,
    checks: &ClaimChecks,
) -> Result<Extraction> {
    let candidate_text = candidate.to_string();
    let content = units(candidate)?;
    let checkpoint = crate::features::learning::lesson_drafts::claim_inventory(&candidate_text)
        .await?
        .and_then(|raw| serde_json::from_str::<InventoryCheckpoint>(&raw).ok())
        .filter(|saved| {
            saved.policy == INVENTORY_POLICY
                && saved.model == llm.model_name()
                && saved.model_context == llm.max_context_tokens()
                && saved.content_sha256 == digest(&candidate_text)
                && validate_inventory(&saved.inventory, &content).is_ok()
        });
    let (inventory, coverage) = match checkpoint {
        Some(mut saved) => {
            if saved.coverage_policy.as_deref() != Some(coverage::POLICY)
                || !valid_saved_coverage(&saved.coverage, &content)
            {
                saved.coverage.units.clear();
            }
            crate::features::learning::lesson_progress::phase(
                crate::features::learning::lesson_progress::Phase::Coverage,
                "Resuming saved claim coverage with the current policy",
            );
            complete_inventory(
                llm,
                &content,
                saved.inventory,
                (&candidate_text, candidate),
                Some(saved.coverage),
                &checks.rewrites,
            )
            .await?
        }
        None => {
            extract_audited_inventory(
                llm,
                &content,
                (&candidate_text, candidate),
                &checks.rewrites,
            )
            .await?
        }
    };
    let checkpoint = InventoryCheckpoint {
        policy: INVENTORY_POLICY.into(),
        model: llm.model_name().into(),
        model_context: llm.max_context_tokens(),
        content_sha256: digest(&candidate_text),
        inventory,
        coverage,
        coverage_policy: Some(coverage::POLICY.into()),
    };
    crate::features::learning::lesson_drafts::record_claim_inventory(
        &candidate_text,
        serde_json::to_string(&checkpoint)?,
    )
    .await?;
    crate::features::learning::lesson_progress::phase(
        crate::features::learning::lesson_progress::Phase::Examples,
        "Checking executable examples",
    );
    let removed = checks.removed.clone();
    let executions: Vec<_> = execution::observe(runtimes, candidate)
        .await?
        .into_iter()
        .filter(|run| run.unit.is_none_or(|unit| !removed.contains(unit)))
        .collect();
    if executions
        .iter()
        .any(|observation| observation.unavailable())
    {
        return Err(AppError::ServiceNotAvailable("An example runtime is unavailable or cancelled. No lesson was published; retry when the runtime is available.".into()));
    }
    Ok(Extraction {
        inventory: checkpoint.inventory,
        coverage: checkpoint.coverage,
        executions,
    })
}

/// Matches `extraction`'s statements to the claims the ledger holds and
/// identifies them, before anything is read: each statement that continues a
/// claim takes that claim's class, a repair's heir takes its parent's, and
/// only a statement new to the ledger is sorted. A statement the rule layer
/// makes Internal (`Classed::checked`) is left out of the claims; one the model
/// sorts Internal is a claim, put to the falsity veto. The reopenings that
/// need no check are made here, and a failed or tracked example is checked.
pub(super) async fn identify(
    llm: &dyn LLMPort,
    policy: &dyn AcceptancePolicy,
    ledger: &mut ClaimLedger,
    (candidate, references): (&Value, &ReferenceCollection<'_>),
    extraction: Extraction,
    checks: &ClaimChecks,
) -> Result<Pass> {
    let removed = &checks.removed;
    let claims: Vec<(usize, Claim)> = extraction
        .inventory
        .units
        .iter()
        .filter(|unit| !removed.contains(unit.index))
        .flat_map(|unit| {
            unit.claims
                .iter()
                .cloned()
                .map(move |claim| (unit.index, claim))
        })
        .collect();
    let citations = section_citations(candidate, references)?;
    let mut extracted = ledger.extracted(candidate, &claims, &extraction.executions);
    let matched = ledger.match_pass(candidate, &extracted);
    let given = ledger.classes_given(&matched, claims.len());
    let classes = class_checks::sort(
        llm,
        policy,
        candidate,
        &claims,
        &given,
        &checks.rewrites,
        &citations,
    )
    .await?;
    let left: Vec<bool> = classes.iter().map(|classed| !classed.checked()).collect();
    for (found, classed) in extracted.iter_mut().zip(&classes) {
        found.class = Some(*classed);
    }
    let kept: Vec<bool> = left.iter().map(|left| !left).collect();
    let matched = matched.keep(&kept);
    let extracted: Vec<_> = extracted
        .into_iter()
        .enumerate()
        .filter(|(index, _)| kept.get(*index).copied().unwrap_or(true))
        .map(|(_, found)| found)
        .collect();
    ledger
        .identify(policy, candidate, extracted, matched)
        .await?;
    let active: HashSet<&str> = references
        .sources
        .iter()
        .map(|source| source.id.as_str())
        .collect();
    let identified = ledger.finding_claims().to_vec();
    ledger
        .reopen_changed(candidate, &active, &identified)
        .await?;
    ledger
        .record_example_checks(candidate, &extraction.executions, &active)
        .await?;
    let mut ids = identified.iter();
    let mut statements = Vec::with_capacity(identified.len());
    for ((unit, claim), left) in claims.iter().zip(&left) {
        if *left {
            continue;
        }
        let id = ids.next().ok_or_else(|| {
            AppError::InternalError("A statement was left without a claim".into())
        })?;
        statements.push(Identified {
            id: id.clone(),
            unit: *unit,
            claim: claim.clone(),
        });
    }
    crate::features::learning::lesson_progress::claims(ledger.tally());
    Ok(Pass {
        extraction,
        left,
        statements,
    })
}

/// Reads each of the statements `due` from the position it waits at, along its
/// graph until its node is a remedy or an end. Nothing is recorded: a reader's
/// outage is the `Err`, and the readings are recorded by [`record_readings`].
/// The progress counts a statement as the reading that ends its walk lands.
pub(super) async fn read_steps(
    llm: &dyn LLMPort,
    policy: &dyn AcceptancePolicy,
    readers: &dyn ReaderSet,
    ledger: &ClaimLedger,
    checks: &ClaimChecks,
    (candidate, references): (&Value, &ReferenceCollection<'_>),
    (pass, due): (&Pass, &[(ClaimId, Position)]),
) -> Result<Readings> {
    let mut statements: Vec<(Statement<'_>, Position)> = Vec::with_capacity(due.len());
    let mut ids = Vec::with_capacity(due.len());
    for (id, position) in due {
        let Some(known) = pass.statements.iter().find(|known| known.id == *id) else {
            continue;
        };
        let held = ledger.entry(id).and_then(ClaimLedgerEntry::classified);
        let by = held
            .filter(|(class, _)| *class == position.class)
            .map_or(ClassifiedBy::Evidence, |(_, by)| by);
        statements.push((
            Statement {
                unit: known.unit,
                claim: &known.claim,
                classed: Classed {
                    class: position.class,
                    by,
                },
            },
            *position,
        ));
        ids.push(id.clone());
    }
    if statements.is_empty() {
        return Ok(Vec::new());
    }
    crate::features::learning::lesson_progress::phase(
        crate::features::learning::lesson_progress::Phase::Evidence,
        format!(
            "Checking {} statement{}",
            statements.len(),
            if statements.len() == 1 { "" } else { "s" },
        ),
    );
    crate::features::learning::lesson_progress::begin_checks(statements.len());
    let citations = section_citations(candidate, references)?;
    let context = ReadContext::new(
        llm,
        candidate,
        references,
        &citations,
        &pass.extraction.executions,
        checks,
    )
    .await?;
    let steps = read_until_remedy(policy, readers, &context, &statements).await?;
    Ok(ids.into_iter().zip(steps).collect())
}

/// Records `readings`: each statement's latest reading's finding is kept (in
/// memory and in a checkpoint, before the ledger records the decisions that
/// lead to it), then each reading is recorded with the policy's decision on
/// it. Gives the statements this run stopped on: a second unreadable answer.
pub(super) async fn record_readings(
    policy: &dyn AcceptancePolicy,
    ledger: &mut ClaimLedger,
    checks: &mut ClaimChecks,
    readings: Readings,
) -> Result<Vec<ClaimId>> {
    for (id, read) in &readings {
        if let Some(last) = read.steps.last() {
            let finding = last.read.finding.clone();
            crate::features::learning::lesson_drafts::record_checkpoint(
                &finding_key(id),
                serde_json::to_value(&finding)?,
            )
            .await?;
            checks.latest.insert(id.clone(), finding);
        }
    }
    let mut stopped = Vec::new();
    for (id, read) in &readings {
        for step in &read.steps {
            ledger.record_reading(policy, id, step).await?;
        }
        // The card keeps the passages the last reading that compared some
        // showed.
        let compared = read
            .steps
            .iter()
            .rev()
            .find(|step| !step.read.finding.evidence.is_empty());
        if let Some(step) = compared {
            ledger
                .show_evidence(id, &step.read.finding.evidence)
                .await?;
        }
        if matches!(read.walked, Walked::Stopped) {
            stopped.push(id.clone());
        }
    }
    crate::features::learning::lesson_progress::claims(ledger.tally());
    Ok(stopped)
}

/// The finding a statement's latest reading left, restored from the checkpoint
/// the job saved it in, when it saved one.
async fn saved_finding(id: &ClaimId) -> Result<Option<Finding>> {
    Ok(
        crate::features::learning::lesson_drafts::checkpoint(&finding_key(id))
            .await?
            .and_then(|value| serde_json::from_value::<Finding>(value).ok()),
    )
}

/// What a check's result says of a statement.
fn verdict_of(result: &CheckResult) -> ClaimVerdict {
    match result {
        CheckResult::Supported { .. }
        | CheckResult::SupportedElsewhere { .. }
        | CheckResult::Consistent
        | CheckResult::KnownCommon
        | CheckResult::NotFalse => ClaimVerdict::Supported,
        CheckResult::Failed {
            kind: VerdictKind::Contradicted,
        } => ClaimVerdict::Contradicted,
        CheckResult::Failed {
            kind: VerdictKind::CouldNotJudge,
        } => ClaimVerdict::Unverified,
        CheckResult::Failed { .. } => ClaimVerdict::Unsupported,
    }
}

/// The finding of a statement no reading of this job has made, rebuilt from the
/// ledger: the verdict, reason and support of its latest local check, where a
/// support found elsewhere is (which moving the citation needs), the passages
/// its card shows, and its class. A fresh job over a lesson the ledger already
/// holds has none saved, and a missing finding is never a fault.
fn from_ledger(
    ledger: &ClaimLedger,
    known: &Identified,
    shown: &[ShownPassage],
) -> Result<Finding> {
    let entry = ledger.entry(&known.id).ok_or_else(|| {
        AppError::InternalError(format!(
            "The statement \"{}\" is not in the claim ledger",
            known.claim.statement
        ))
    })?;
    let latest = entry
        .events()
        .iter()
        .rev()
        .find_map(|record| match &record.event {
            ClaimEvent::Checked(check) if matches!(check.checker, Checker::Local { .. }) => {
                Some(check)
            }
            _ => None,
        });
    let passed = matches!(
        entry.state(),
        ClaimState::ResolvedByEvidence { .. }
            | ClaimState::Consistent { .. }
            | ClaimState::GeneralKnowledge { .. }
    );
    let verdict = if passed {
        ClaimVerdict::Supported
    } else {
        latest.map_or(ClaimVerdict::Unsupported, |check| verdict_of(&check.result))
    };
    let supporting_quote = match entry.state() {
        ClaimState::ResolvedByEvidence {
            supporting_quote, ..
        } => Some(supporting_quote.clone()),
        _ => latest
            .and_then(|check| check.result.support())
            .map(|(_, quote)| quote.to_owned()),
    };
    let supported_by = match latest.map(|check| (&check.result, &check.evidence)) {
        Some((
            CheckResult::SupportedElsewhere {
                cited,
                source_version_id,
                supporting_quote,
            },
            seen,
        )) => {
            let of_source =
                |passage: &&ShownPassage| passage.source_version_id == *source_version_id;
            let bytes = shown
                .iter()
                .find(|passage| {
                    of_source(passage) && passage.text.contains(supporting_quote.as_str())
                })
                .or_else(|| shown.iter().find(of_source))
                .map(|passage| (passage.start_byte, passage.end_byte))
                .or_else(|| {
                    seen.iter()
                        .find(|passage| passage.source_version_id == *source_version_id)
                        .map(|passage| (passage.start_byte, passage.end_byte))
                });
            bytes.map(|(start_byte, end_byte)| SupportedElsewhere {
                source_id: source_version_id.clone(),
                start_byte,
                end_byte,
                cited: cited.clone(),
            })
        }
        _ => None,
    };
    let evidence = shown
        .iter()
        .map(|passage| EvidencePassage {
            source_id: passage.source_version_id.clone(),
            text: passage.text.clone(),
            start_byte: passage.start_byte,
            end_byte: passage.end_byte,
            retrieval_kind: passage.retrieval_kind.clone(),
            score: 0.0,
        })
        .collect();
    Ok(Finding {
        unit: known.unit,
        quote: known.claim.quote.clone(),
        statement: known.claim.statement.clone(),
        verdict,
        reason: latest.map(|check| check.reason.clone()).unwrap_or_default(),
        evidence,
        supporting_quote,
        resolution: None,
        trust: None,
        label: None,
        class: entry.classified().map(|(class, by)| Classed { class, by }),
        two_pass: TwoPass {
            supported_by,
            ..Default::default()
        },
    })
}

/// The findings of the statements that are claims: each one's latest
/// reading's, in the inventory's order. A claim the user resolved with evidence
/// of their own is reported with that check, whatever a reading left. Any other
/// finding is kept in this job's memory; otherwise it is restored from its
/// checkpoint, otherwise rebuilt from the ledger. It decides nothing: the policy
/// has decided from the reading.
pub(super) async fn latest_findings(
    ledger: &ClaimLedger,
    checks: &mut ClaimChecks,
    pass: &Pass,
) -> Result<Vec<Finding>> {
    let mut lost = Vec::new();
    for known in &pass.statements {
        if checks.latest.contains_key(&known.id)
            || checks.settled_finding(known.unit, &known.claim).is_some()
        {
            continue;
        }
        match saved_finding(&known.id).await? {
            Some(finding) => {
                checks.latest.insert(known.id.clone(), finding);
            }
            None => lost.push(known),
        }
    }
    if !lost.is_empty() {
        let shown = ledger.shown_passages().await?;
        for known in lost {
            let passages = shown.get(&known.id).map_or(&[][..], Vec::as_slice);
            let finding = from_ledger(ledger, known, passages)?;
            checks.latest.insert(known.id.clone(), finding);
        }
    }
    let mut findings = Vec::with_capacity(pass.statements.len());
    for known in &pass.statements {
        let mut finding = match checks.settled_finding(known.unit, &known.claim) {
            Some(mut settled) => {
                settled.class = ledger
                    .entry(&known.id)
                    .and_then(ClaimLedgerEntry::classified)
                    .map(|(class, by)| Classed { class, by });
                settled
            }
            None => checks.latest.get(&known.id).cloned().ok_or_else(|| {
                AppError::InternalError(format!(
                    "The statement \"{}\" was left without a finding",
                    known.claim.statement
                ))
            })?,
        };
        finding.unit = known.unit;
        finding.quote = known.claim.quote.clone();
        finding.statement = known.claim.statement.clone();
        findings.push(finding);
    }
    Ok(findings)
}

/// What reading the statements a hint named came to, by claim.
pub(super) type Answers = Vec<(ClaimId, Read)>;

/// Reads each of the `hinted` statements once by the page, over `references`
/// the round expanded, whatever its class: the user asked for a source. Nothing
/// is recorded, and no policy walks them.
pub(super) async fn read_hinted(
    llm: &dyn LLMPort,
    readers: &dyn ReaderSet,
    ledger: &ClaimLedger,
    checks: &ClaimChecks,
    (candidate, references): (&Value, &ReferenceCollection<'_>),
    (pass, hinted): (&Pass, &[ClaimId]),
) -> Result<Answers> {
    let mut statements: Vec<Statement<'_>> = Vec::with_capacity(hinted.len());
    let mut ids = Vec::with_capacity(hinted.len());
    for id in hinted {
        let Some(known) = pass.statements.iter().find(|known| known.id == *id) else {
            continue;
        };
        let Some((class, by)) = ledger.entry(id).and_then(ClaimLedgerEntry::classified) else {
            continue;
        };
        statements.push(Statement {
            unit: known.unit,
            claim: &known.claim,
            classed: Classed { class, by },
        });
        ids.push(id.clone());
    }
    if statements.is_empty() {
        return Ok(Vec::new());
    }
    let citations = section_citations(candidate, references)?;
    let context = ReadContext::new(
        llm,
        candidate,
        references,
        &citations,
        &pass.extraction.executions,
        checks,
    )
    .await?;
    let reads = read_page(readers, &context, &statements).await?;
    Ok(ids.into_iter().zip(reads).collect())
}

/// Records the reading that answered each user's hint: its finding becomes the
/// claim's latest (kept in memory and in a checkpoint, before the ledger
/// records it), its passages are shown on the card, and the ledger records the
/// check and nothing else.
pub(super) async fn record_hinted(
    ledger: &mut ClaimLedger,
    checks: &mut ClaimChecks,
    answers: Answers,
) -> Result<()> {
    for (id, read) in answers {
        let finding = read.finding;
        crate::features::learning::lesson_drafts::record_checkpoint(
            &finding_key(&id),
            serde_json::to_value(&finding)?,
        )
        .await?;
        checks.latest.insert(id.clone(), finding.clone());
        ledger.record_hint_reading(&id, &finding).await?;
        if !finding.evidence.is_empty() {
            ledger.show_evidence(&id, &finding.evidence).await?;
        }
    }
    Ok(())
}

/// The report of a pass: one finding for each statement that is a claim, in
/// the inventory's order, with the coverage accounting, the examples' runs and
/// the issues the findings leave open.
pub(super) fn report(
    llm: &dyn LLMPort,
    references: &ReferenceCollection<'_>,
    candidate: &Value,
    pass: &Pass,
    findings: Vec<Finding>,
    checks: &ClaimChecks,
) -> LessonVerificationReport {
    let removed = &checks.removed;
    let mut issues = Vec::new();
    for observation in &pass.extraction.executions {
        if !observation.passed() {
            issues.push(format!(
                "Example {} did not complete: {}",
                observation.id,
                observation.evidence()
            ));
        }
    }
    // The statements left out of the lesson's claims are not in its coverage
    // either: the report accounts for each claim by one finding.
    let mut left = pass.left.iter().copied();
    let mut accounted: Vec<UnitClaims> = Vec::new();
    for unit in &pass.extraction.inventory.units {
        if removed.contains(unit.index) {
            accounted.push(UnitClaims {
                index: unit.index,
                claims: Vec::new(),
                non_factual_reason: "Taken out of the lesson: its sources do not support it."
                    .to_owned(),
            });
            continue;
        }
        let claims: Vec<Claim> = unit
            .claims
            .iter()
            .filter(|_| !left.next().unwrap_or(false))
            .cloned()
            .collect();
        let non_factual_reason = if claims.is_empty() && unit.non_factual_reason.trim().is_empty() {
            "Its statements are about the lesson itself: a rubric, an option or a remark about its sources.".to_owned()
        } else {
            unit.non_factual_reason.clone()
        };
        accounted.push(UnitClaims {
            index: unit.index,
            claims,
            non_factual_reason,
        });
    }
    // Every claim a passage does not support is an issue until the claim
    // ledger closes it.
    for finding in &findings {
        if !finding.publishable() {
            issues.push(format!(
                "Section {}: {} — {}",
                finding.unit + 1,
                finding.statement,
                finding.reason
            ));
        }
    }
    LessonVerificationReport {
        policy: POLICY.into(),
        lesson_id: String::new(),
        content_sha256: String::new(),
        checker_model: llm.model_name().into(),
        checked_at: chrono::Utc::now().timestamp_millis(),
        sources: references
            .sources
            .iter()
            .map(|s| SourceBinding {
                id: s.id.clone(),
                sha256: digest(&s.excerpt),
            })
            .collect(),
        coverage: accounted,
        coverage_audit: pass.extraction.coverage.units.clone(),
        findings,
        executions: pass.extraction.executions.clone(),
        confirmed_examples: None,
        issues,
        retrieval_mode: references.mode().label().into(),
        embedding_model: references.embedding_model(),
        page_budget: Some(evidence_checks::page_budget_note(llm, checks)),
        unexecuted_languages: execution::unexecuted_languages(candidate),
    }
}
