//! The verify-and-repair loop and its resumable pending-repair checkpoint.
use super::claim_ledger::{self, ClaimLedger};
use super::*;
use lattice_core::domain::learning::claim_review::ClaimId;

/// [`verify_and_repair_with_references`] over lexical references, for tests.
#[cfg(test)]
pub(in crate::features::learning) async fn verify_and_repair(
    llm: &dyn LLMPort,
    prompt: &str,
    schema: &Value,
    raw: String,
    sources: &[LearningSourceDto],
    output_tokens: usize,
) -> Result<(String, LessonVerificationReport)> {
    let references = ReferenceCollection::lexical(sources)?;
    verify_and_repair_with_references(
        llm,
        &BuiltinRuntimes::for_tests(),
        prompt,
        schema,
        raw,
        &references,
        (output_tokens, &mut sources.to_vec()),
    )
    .await
}

/// The checkpoint holding the sources this lesson's verification numbered to
/// cite them, by version id, in the order it numbered them.
const NUMBERED_KEY: &str = "lesson-numbered-sources-v1";

/// `numbered` with the sources an earlier run of this preparation numbered
/// to cite them appended, in their order, so the draft's source indices name
/// the same sources after a restart. A source no longer in `references` is
/// left out.
async fn renumber(
    numbered: &mut Vec<LearningSourceDto>,
    references: &ReferenceCollection<'_>,
) -> Result<()> {
    let Some(saved) = crate::features::learning::lesson_drafts::checkpoint(NUMBERED_KEY)
        .await?
        .and_then(|value| serde_json::from_value::<Vec<String>>(value).ok())
    else {
        return Ok(());
    };
    for id in saved {
        if numbered.iter().any(|known| known.id == id) {
            continue;
        }
        if let Some(source) = references.sources.iter().find(|source| source.id == id) {
            numbered.push(source.clone());
        }
    }
    Ok(())
}

/// Verify, research and repair until nothing is left for the machine (§4).
/// Each pass is observed by the claim ledger, which records a check only when
/// something about the claim is new, researches a claim once per evidence,
/// and lets a repair target only claims with attempts left: one correction
/// and one escalated rewrite (`repeatedDefect`, remove the assertion whole)
/// per piece of evidence. When no claim can be repaired, the lesson publishes
/// if every claim is resolved, and otherwise waits for the user's review with
/// every open claim kept: the draft is saved, the review row says so, and
/// [`claim_ledger::needs_review`] unwinds the preparation (§6.1). Sync points
/// before each pass and each repair group pick up what the user did (§9).
/// Operational extraction/audit errors propagate rather than asking the author
/// to rewrite valid content because a checker is unavailable.
///
/// `numbered` is the lesson's numbered sources, what its `sourceIndex`es
/// name: the writer's, and any this verification adds to cite a claim's
/// support (`repair::cite_where_supported`). The lesson is parsed against
/// it, and `writer_schema`'s source indices are widened to it.
pub(in crate::features::learning) async fn verify_and_repair_with_references(
    llm: &dyn LLMPort,
    runtimes: &BuiltinRuntimes,
    prompt: &str,
    writer_schema: &Value,
    mut raw: String,
    references: &ReferenceCollection<'_>,
    (output_tokens, numbered): (usize, &mut Vec<LearningSourceDto>),
) -> Result<(String, LessonVerificationReport)> {
    let writers = numbered.len();
    renumber(numbered, references).await?;
    let mut numbered_schema = writer_schema.clone();
    repair::number_sources(&mut numbered_schema, numbered.len());
    let mut ledger = ClaimLedger::open(llm, &raw).await?;
    let mut expanded = None;
    let mut repair_history = Vec::new();
    let mut resume_repair = true;
    let mut checks = ClaimChecks::default();
    let mut verified: Option<String> = None;
    // Citations this job repaired to where a claim's support was found.
    let mut cited_where_supported = HashSet::new();
    loop {
        let schema = &numbered_schema;
        raw = normalize_example_fences(llm, raw).await?;
        let mut candidate: Value = crate::features::learning::generation::parse_json(&raw)?;
        ledger.sync().await?;
        let active = expanded.as_ref().unwrap_or(references);
        checks.settled = ledger.user_evidence(active);
        if review_pass::apply_user_changes(&mut ledger, &mut candidate).await? {
            raw = candidate.to_string();
            crate::features::learning::lesson_drafts::save(&raw).await?;
            // The user's text gets the teaching review a rewrite gets (§6.2).
            raw = crate::features::learning::teaching::review_lesson(
                llm,
                crate::features::learning::teaching::LESSON_REPAIR_SYSTEM,
                prompt,
                schema,
                raw,
                output_tokens,
                active,
                &repair::Protected::of_ledger(&ledger, &[]),
            )
            .await?;
            candidate = crate::features::learning::generation::parse_json(&raw)?;
        }
        // A failed rewrite can resume from its saved defects. This only skips
        // repeating the diagnosis; it never skips checking the rewritten text.
        if std::mem::take(&mut resume_repair) {
            let saved =
                crate::features::learning::lesson_drafts::pending_repair(&candidate.to_string())
                    .await?
                    .and_then(|raw| serde_json::from_str::<PendingRepair>(&raw).ok())
                    .filter(|saved| saved.matches(llm, prompt, schema, &candidate, active));
            let left = saved.map(|saved| still_repairable(&ledger, saved.context));
            if matches!(left, Some(None)) {
                // Everything the saved repair was for was resolved meanwhile.
                crate::features::learning::lesson_drafts::save_repair(&raw, None).await?;
            }
            if let Some(context) = left.flatten() {
                crate::features::learning::lesson_progress::phase(
                    crate::features::learning::lesson_progress::Phase::Repair,
                    "Resuming the saved evidence-based repair",
                );
                repair_history = context
                    .get("repairHistory")
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default();
                raw = repair::candidate(
                    llm,
                    prompt,
                    schema,
                    context,
                    output_tokens,
                    active,
                    &mut ledger,
                )
                .await?;
                continue;
            }
        }
        if verified.as_deref() != Some(raw.as_str()) {
            crate::features::learning::lesson_progress::new_pass();
            verified = Some(raw.clone());
        }
        let mut report =
            verify_with_references(llm, runtimes, &candidate, active, &mut checks).await?;
        ledger.observe(&candidate, &report).await?;
        // Research once per claim and evidence: what it saves is checked at
        // once; what it does not find is recorded as tried (§4).
        loop {
            let (claims, findings, hints) = ledger.research_targets(&report);
            if claims.is_empty() {
                break;
            }
            let active = expanded.as_ref().unwrap_or(references);
            let expansion = research::expand(llm, active, &findings, &hints, &checks)
                .boxed()
                .await?;
            // What the round cost per claim is what this job's later walks
            // weigh against reading on (D5).
            if let Some(per_claim) = expansion.per_claim() {
                checks.research_cost = Some(per_claim);
            }
            match expansion {
                research::Expansion::Saved(additional, searched) => {
                    expanded = Some(*additional);
                    let active = expanded.as_ref().ok_or_else(|| {
                        AppError::InternalError("Missing expanded references".into())
                    })?;
                    crate::features::learning::lesson_progress::new_references(Some(
                        searched.sources_added.len(),
                    ));
                    let rechecked = check_evidence(
                        llm,
                        &candidate,
                        active,
                        Inventory {
                            units: report.coverage,
                        },
                        Coverage {
                            units: report.coverage_audit,
                        },
                        report.executions,
                        &mut checks,
                    )
                    .await;
                    crate::features::learning::lesson_progress::new_references(None);
                    report = rechecked?;
                    // The round is recorded first: a claim whose evidence it
                    // changed may be researched again on that evidence.
                    ledger.record_research(&claims, &searched, &report).await?;
                    ledger.observe(&candidate, &report).await?;
                }
                research::Expansion::Unchanged(searched) => {
                    // A round that reached no search engine and no page is
                    // not the claims' research: they keep their round.
                    if !searched.unreached {
                        ledger.record_research(&claims, &searched, &report).await?;
                    } else {
                        tracing::info!(
                            "claim.state: research reached nothing; the claims keep their round"
                        );
                    }
                    break;
                }
                research::Expansion::Exhausted(exhausted, searched) => {
                    tracing::info!(%exhausted, "claim.state: research found nothing new for these claims");
                    ledger.record_research(&claims, &searched, &report).await?;
                    break;
                }
            }
        }
        ledger.propose_general_knowledge(&report).await?;
        // A claim supported elsewhere in the course keeps its text; its
        // section's citation moves to the passage found, and the next pass
        // checks it as the section's own citation (§5.1).
        let active = expanded.as_ref().unwrap_or(references);
        let repaired = repair::cite_where_supported(
            &mut candidate,
            &report.findings,
            numbered,
            active,
            &mut cited_where_supported,
        )?;
        if !repaired.is_empty() {
            crate::features::learning::lesson_progress::phase(
                crate::features::learning::lesson_progress::Phase::Repair,
                "Citing the passages that support claims their sections cited elsewhere",
            );
            // The added sources are saved before the draft that cites them.
            let added: Vec<&str> = numbered
                .iter()
                .skip(writers)
                .map(|source| source.id.as_str())
                .collect();
            crate::features::learning::lesson_drafts::record_checkpoint(NUMBERED_KEY, json!(added))
                .await?;
            repair::number_sources(&mut numbered_schema, numbered.len());
            for (finding, event) in repaired {
                if let Some(claim) = ledger.finding_claims().get(finding).cloned() {
                    ledger.append(&claim, event).await?;
                }
            }
            raw = candidate.to_string();
            crate::features::learning::lesson_drafts::save(&raw).await?;
            continue;
        }
        // A failed claim whose every sentence another live claim stands on
        // waits for the user without a repair call (claim sources design
        // §18.5); with none left to repair the decision is the review.
        let decision = loop {
            match ledger.decide() {
                claim_ledger::Decision::Repair(targets) => {
                    let editable = ledger.withhold_shared(&targets).await?;
                    if editable.is_empty() {
                        continue;
                    }
                    break claim_ledger::Decision::Repair(editable);
                }
                decision => break decision,
            }
        };
        match decision {
            claim_ledger::Decision::Publish => {
                // The last sync point: an edit made during this pass is
                // applied and verified before anything publishes.
                ledger.sync().await?;
                if review_pass::has_user_changes(&ledger, &candidate)
                    || !matches!(ledger.decide(), claim_ledger::Decision::Publish)
                {
                    continue;
                }
                // Then the Verifier audit, when the course opted in (§14.3):
                // review pass → audit → publish. Its checks, and anything the
                // user did while it ran, are read back before publishing.
                match ledger.audit(&raw, &candidate).await? {
                    claim_ledger::Audited::Off => {}
                    claim_ledger::Audited::Agreed => {
                        ledger.sync().await?;
                        if review_pass::has_user_changes(&ledger, &candidate)
                            || !matches!(ledger.decide(), claim_ledger::Decision::Publish)
                        {
                            continue;
                        }
                    }
                    claim_ledger::Audited::Hold(stop_reason) => {
                        ledger.sync().await?;
                        let claims = ledger.awaiting_user();
                        return Err(
                            hold(&mut ledger, &raw, claims, stop_reason.map(|stop| *stop)).await?,
                        );
                    }
                }
                return Ok((raw, ledger.bind(report)));
            }
            claim_ledger::Decision::NeedsReview(claims) => {
                return Err(hold(&mut ledger, &raw, claims, None).await?);
            }
            claim_ledger::Decision::Repair(targets) => {
                remember_failed_claims(&mut repair_history, &report);
                let escalated = targets.iter().any(|id| {
                    ledger
                        .entry(id)
                        .is_some_and(|entry| entry.next_repair_escalates())
                });
                crate::features::learning::lesson_progress::phase(
                    crate::features::learning::lesson_progress::Phase::Repair,
                    if escalated {
                        "Removing a repeatedly unsupported claim"
                    } else {
                        "Correcting defects found by evidence checks"
                    },
                );
                let active = expanded.as_ref().unwrap_or(references);
                let context = repair_context(
                    prompt,
                    &candidate,
                    &report,
                    &repair_history,
                    &ledger,
                    &targets,
                    active,
                )?;
                ledger.begin_repairs(&candidate, &targets).await?;
                let pending = PendingRepair {
                    fingerprint: repair_fingerprint(llm, prompt, schema, &candidate, active),
                    defects: targets.len(),
                    context: context.clone(),
                };
                crate::features::learning::lesson_drafts::record_pending_repair(
                    &candidate.to_string(),
                    serde_json::to_string(&pending)?,
                )
                .await?;
                raw = repair::candidate(
                    llm,
                    prompt,
                    schema,
                    context,
                    output_tokens,
                    active,
                    &mut ledger,
                )
                .await?;
            }
        }
    }
}

/// The lesson waits for the user's review (§6.1): the draft is saved, the
/// review row says so (with the audit's stop when the Verifier audit could
/// not finish, §14.6), and the returned error unwinds the preparation.
async fn hold(
    ledger: &mut ClaimLedger,
    raw: &str,
    claims: usize,
    stop_reason: Option<lattice_core::domain::learning::claim_review::StopReason>,
) -> Result<AppError> {
    crate::features::learning::lesson_drafts::save(raw).await?;
    let message = match &stop_reason {
        Some(stop) => stop.message.clone(),
        None => format!(
            "Needs your review: {claims} claim{} could not be verified",
            if claims == 1 { "" } else { "s" }
        ),
    };
    ledger.stop_for_review(raw, stop_reason).await?;
    crate::features::learning::lesson_progress::phase(
        crate::features::learning::lesson_progress::Phase::Repair,
        message,
    );
    Ok(claim_ledger::needs_review(claims))
}

/// A saved repair's work, less the claims the user resolved while the job
/// was away (§9 Restart); `None` when nothing of it is left to send.
fn still_repairable(ledger: &ClaimLedger, mut context: Value) -> Option<Value> {
    let keep = |item: &Value| {
        item.get("claimId")
            .and_then(Value::as_str)
            .is_some_and(|id| ledger.may_repair(&ClaimId(id.to_owned())))
    };
    let examples = context
        .get("exampleClaims")
        .and_then(Value::as_array)
        .is_some_and(|items| items.iter().any(keep));
    let claims = context.get_mut("failedClaims")?.as_array_mut()?;
    claims.retain(keep);
    (!claims.is_empty() || examples).then_some(context)
}

#[derive(Serialize, Deserialize)]
pub(super) struct PendingRepair {
    pub(super) fingerprint: String,
    pub(super) defects: usize,
    pub(super) context: Value,
}

fn remember_failed_claims(history: &mut Vec<Value>, report: &LessonVerificationReport) {
    for finding in report
        .findings
        .iter()
        .filter(|finding| !finding.publishable())
    {
        let rejected = json!({
            "unit": finding.unit,
            "statement": finding.statement,
            "verdict": finding.verdict,
            "reason": finding.reason,
        });
        if !history.contains(&rejected) {
            history.push(rejected);
        }
    }
}

pub(in crate::features::learning) async fn has_pending_repair(
    llm: &dyn LLMPort,
    prompt: &str,
    schema: &Value,
    raw: &str,
    references: &ReferenceCollection<'_>,
) -> Result<bool> {
    let candidate: Value = crate::features::learning::generation::parse_json(raw)?;
    Ok(
        crate::features::learning::lesson_drafts::pending_repair(raw)
            .await?
            .and_then(|raw| serde_json::from_str::<PendingRepair>(&raw).ok())
            .is_some_and(|saved| saved.matches(llm, prompt, schema, &candidate, references)),
    )
}

pub(super) fn repair_fingerprint(
    llm: &dyn LLMPort,
    prompt: &str,
    schema: &Value,
    candidate: &Value,
    references: &ReferenceCollection<'_>,
) -> String {
    let mut sources: Vec<_> = references
        .sources
        .iter()
        .map(|source| (&source.id, digest(&source.excerpt)))
        .collect();
    sources.sort();
    digest(&json!({"policy":POLICY,"repairPolicy":"pending-repair-v3","model":llm.model_name(),"prompt":prompt,"schema":schema,"candidate":candidate,"sources":sources}).to_string())
}

impl PendingRepair {
    pub(super) fn matches(
        &self,
        llm: &dyn LLMPort,
        prompt: &str,
        schema: &Value,
        candidate: &Value,
        references: &ReferenceCollection<'_>,
    ) -> bool {
        self.defects > 0
            && self.context.get("candidate") == Some(candidate)
            && self.fingerprint == repair_fingerprint(llm, prompt, schema, candidate, references)
    }
}

/// References overlap heavily across related claims. Send each exact passage
/// once instead of multiplying an entire retrieval result by every finding.
/// This changes representation only: no conflicting or unsupported evidence is
/// discarded, and the full candidate still undergoes independent verification.
/// Each passage carries its source's title, and a failed claim names a
/// "Source N" by title too, so a source index is not read as evidence N.
/// Only `targets` are sent, each with its claim id and, on its last attempt,
/// `repeatedDefect` (remove the assertion whole); failed examples go with
/// their section when their claim is a target.
pub(super) fn repair_context(
    prompt: &str,
    candidate: &Value,
    report: &LessonVerificationReport,
    repair_history: &[Value],
    ledger: &ClaimLedger,
    targets: &[ClaimId],
    references: &ReferenceCollection<'_>,
) -> Result<Value> {
    let escalates = |id: &ClaimId| {
        ledger
            .entry(id)
            .is_some_and(|entry| entry.next_repair_escalates())
    };
    let citations = section_citations(candidate, references)?;
    let title = |id: &str| {
        references
            .sources
            .iter()
            .find(|source| source.id == id)
            .map(|source| source.title.clone())
    };
    let mut locations = HashMap::new();
    let mut passages = Vec::new();
    let failed: Vec<_> = report.findings.iter().zip(ledger.finding_claims())
        .filter(|(_, id)| targets.contains(id))
        .map(|(finding, id)| {
            let evidence: Vec<_> = finding.evidence.iter().map(|passage| {
                let key = (&passage.source_id, passage.start_byte, passage.end_byte, &passage.text);
                let index = *locations.entry(key).or_insert_with(|| {
                    let index = passages.len();
                    passages.push(json!({"id":format!("evidence-{index}"),"sourceId":passage.source_id,"sourceTitle":title(&passage.source_id),"startByte":passage.start_byte,"endByte":passage.end_byte,"text":passage.text}));
                    index
                });
                format!("evidence-{index}")
            }).collect();
            // The exact lesson text the repair may change (claim sources
            // design §18.5).
            let sentence = ledger.entry(id).map(|entry| entry.location().quote.clone());
            json!({"unit":finding.unit,"claimId":id.0,"statement":citations.label(&finding.statement, references),"sentence":sentence,"verdict":finding.verdict,"reason":finding.reason,"evidence":evidence,"repeatedDefect":escalates(id)})
        }).collect();
    let examples: Vec<(&execution::Observation, &ClaimId)> = report
        .executions
        .iter()
        .filter_map(|observation| {
            let id = ledger.example_claim(&observation.id)?;
            targets.contains(id).then_some((observation, id))
        })
        .collect();
    Ok(json!({
        "requirements":crate::features::learning::generation::parse_json::<Value>(prompt)?,
        "candidate":candidate,"failedClaims":failed,"evidence":passages,
        "executions":examples.iter().map(|(observation, _)| observation).collect::<Vec<_>>(),
        "exampleClaims":examples.iter().map(|(observation, id)| json!({"unit":observation.unit,"claimId":id.0,"repeatedDefect":escalates(id)})).collect::<Vec<_>>(),
        "repairHistory":repair_history,
    }))
}
