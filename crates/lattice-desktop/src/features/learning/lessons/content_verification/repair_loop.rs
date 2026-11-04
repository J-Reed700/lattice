//! The verify-and-repair loop and its resumable pending-repair checkpoint.
use super::claim_ledger::{self, ClaimLedger, StepKind};
use super::passes;
use super::readings::Readers;
use super::repair::{goal_words, Cited};
use super::*;
use lattice_core::domain::learning::acceptance::{
    AcceptancePolicy, Outcome, Position, StandardPolicy, Step, Taken,
};
use lattice_core::domain::learning::claim_review::{ClaimEvent, ClaimId, ClaimState, StopReason};

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

/// Verifies, researches and repairs until nothing is left for the machine
/// (§4), then publishes if every claim is resolved and otherwise saves the
/// draft for the user's review (§6.1). Each pass reads the statements that
/// wait for a reading, and the acceptance policy decides the step of each
/// (acceptance design §2.10): research, a citation moved or dropped, a rewrite
/// or a cut. Sync points before each pass and repair group pick up what the
/// user did (§9). Operational checker errors propagate rather than asking the
/// author to rewrite valid content.
///
/// `numbered` is the lesson's numbered sources, what its `sourceIndex`es
/// name; the lesson is parsed against it and `writer_schema`'s indices are
/// widened to it.
pub(in crate::features::learning) async fn verify_and_repair_with_references(
    llm: &dyn LLMPort,
    runtimes: &BuiltinRuntimes,
    prompt: &str,
    writer_schema: &Value,
    mut raw: String,
    references: &ReferenceCollection<'_>,
    (output_tokens, numbered): (usize, &mut Vec<LearningSourceDto>),
) -> Result<(String, LessonVerificationReport)> {
    let policy = StandardPolicy::new();
    let readers = Readers::standard();
    let writers = numbered.len();
    renumber(numbered, references).await?;
    let mut numbered_schema = writer_schema.clone();
    repair::number_sources(&mut numbered_schema, numbered.len());
    let mut ledger = ClaimLedger::open(llm, &raw).await?;
    let mut expanded = None;
    let mut resume_repair = true;
    let mut checks = ClaimChecks::default();
    let mut verified: Option<String> = None;
    // The claims the call plan was made for, in its order: the first pass's.
    let mut planned: Vec<ClaimId> = Vec::new();
    // Citations this job repaired to where a claim's support was found.
    let mut cited_where_supported = HashSet::new();
    loop {
        let schema = &numbered_schema;
        raw = normalize_example_fences(llm, raw).await?;
        let mut candidate: Value = crate::features::learning::generation::parse_json(&raw)?;
        ledger.sync().await?;
        let active = expanded.as_ref().unwrap_or(references);
        checks.settled = ledger.user_evidence(active);
        checks.rewrites = ledger.rewrites(&policy);
        checks.removed = ledger.cuts.units();
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
        // repeating the diagnosis; it never skips reading the rewritten text.
        if std::mem::take(&mut resume_repair) {
            let saved =
                crate::features::learning::lesson_drafts::pending_repair(&candidate.to_string())
                    .await?
                    .and_then(|raw| serde_json::from_str::<PendingRepair>(&raw).ok())
                    .filter(|saved| saved.matches(llm, prompt, schema, &candidate, active));
            let left = saved.map(|saved| still_repairable(&ledger, &policy, saved.context));
            if matches!(left, Some(None)) {
                // Everything the saved repair was for was resolved meanwhile.
                crate::features::learning::lesson_drafts::save_repair(&raw, None).await?;
            }
            if let Some(context) = left.flatten() {
                crate::features::learning::lesson_progress::phase(
                    crate::features::learning::lesson_progress::Phase::Repair,
                    "Resuming the saved evidence-based repair",
                );
                raw = repair::candidate(
                    llm,
                    &policy,
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
        // What this iteration starts from, to know whether it moved anything.
        let (stepped, draft) = (ledger.stepped(), raw.clone());
        let extraction = passes::extract(llm, runtimes, &candidate, &checks).await?;
        let pass = passes::identify(
            llm,
            &policy,
            &mut ledger,
            (&candidate, active),
            extraction,
            &checks,
        )
        .await?;
        if let Some(reason) = ledger.fault(&policy) {
            return Err(stop(&mut ledger, &raw, reason).await?);
        }
        let due = ledger.due(&policy);
        let readings = passes::read_steps(
            llm,
            &policy,
            &readers,
            &ledger,
            &checks,
            (&candidate, active),
            (&pass, &due),
        )
        .await?;
        let mut stopped =
            passes::record_readings(&policy, &mut ledger, &mut checks, readings).await?;
        let findings = passes::latest_findings(&ledger, &mut checks, &pass).await?;
        let mut report = passes::report(llm, active, &candidate, &pass, findings, &checks);
        plan_moved(&policy, &ledger, &mut planned);
        // A flow repair that wrote a statement is taken back (claim classes
        // design §3.10).
        if let Some(restored) = sections::unmend(&mut ledger, &candidate).await? {
            raw = restored;
            crate::features::learning::lesson_drafts::save(&raw).await?;
            continue;
        }
        // Research for the statements at a research node and the claims a hint
        // names, once per claim: what it saves is read at once, and what it
        // does not find is recorded as tried (§4). A claim a hint names is the
        // user's: it takes no step, and the page that reads what the round
        // saved answers the hint. A round that reaches nothing records
        // nothing, and the claims stay where they are.
        let mut unreached = false;
        while stopped.is_empty() {
            let targets = ledger.research_targets(&policy, &report);
            if targets.claims.is_empty() {
                break;
            }
            let active = expanded.as_ref().unwrap_or(references);
            let expansion =
                research::expand(llm, active, &targets.findings, &targets.hints, &checks)
                    .boxed()
                    .await?;
            let (searched, saved) = match expansion {
                research::Expansion::Saved(additional, searched) => {
                    expanded = Some(*additional);
                    (searched, true)
                }
                research::Expansion::Unchanged(searched) if searched.unreached => {
                    tracing::info!(
                        "claim.state: research reached nothing; the claims stay at their node"
                    );
                    unreached = true;
                    break;
                }
                research::Expansion::Unchanged(searched) => (searched, false),
                research::Expansion::Exhausted(exhausted, searched) => {
                    tracing::info!(%exhausted, "claim.state: research found nothing new for these claims");
                    (searched, false)
                }
            };
            // What the round found is read at once, from the node each
            // statement's research step leads to: before the step is recorded,
            // so that the round records what it found for each statement.
            let outcome = if saved {
                Outcome::Saved
            } else {
                Outcome::FoundNothing
            };
            let active = expanded.as_ref().unwrap_or(references);
            let due: Vec<(ClaimId, Position)> = ledger
                .at(&policy, StepKind::Research)
                .into_iter()
                .filter_map(|id| {
                    let step = Step::Research;
                    let next = ledger.after(&policy, &id, Taken { step, outcome })?;
                    let node = policy.graph(next.class).node(next.node)?;
                    matches!(node.step, Step::Read(_)).then_some((id, next))
                })
                .collect();
            if saved {
                crate::features::learning::lesson_progress::new_references(Some(
                    searched.sources_added.len(),
                ));
            }
            let read = async {
                let readings = passes::read_steps(
                    llm,
                    &policy,
                    &readers,
                    &ledger,
                    &checks,
                    (&candidate, active),
                    (&pass, &due),
                )
                .await?;
                let hinted = if saved {
                    passes::read_hinted(
                        llm,
                        &readers,
                        &ledger,
                        &checks,
                        (&candidate, active),
                        (&pass, &targets.hinted),
                    )
                    .await?
                } else {
                    Vec::new()
                };
                Ok::<_, AppError>((readings, hinted))
            }
            .await;
            crate::features::learning::lesson_progress::new_references(None);
            let (readings, hinted) = read?;
            let found = |id: &ClaimId| {
                let finding = readings
                    .iter()
                    .find(|(read, _)| read == id)
                    .and_then(|(_, walk)| walk.steps.first())
                    .map(|step| &step.read.finding)
                    .or_else(|| {
                        hinted
                            .iter()
                            .find(|(read, _)| read == id)
                            .map(|(_, read)| &read.finding)
                    });
                finding.map_or(0, |finding| {
                    finding
                        .evidence
                        .iter()
                        .filter(|passage| searched.sources_added.contains(&passage.source_id))
                        .count()
                })
            };
            let round: Vec<(ClaimId, usize)> = targets
                .claims
                .iter()
                .map(|id| (id.clone(), found(id)))
                .collect();
            ledger
                .record_research(&policy, &round, &searched, saved)
                .await?;
            stopped.extend(
                passes::record_readings(&policy, &mut ledger, &mut checks, readings).await?,
            );
            passes::record_hinted(&mut ledger, &mut checks, hinted).await?;
            let findings = passes::latest_findings(&ledger, &mut checks, &pass).await?;
            report = passes::report(llm, active, &candidate, &pass, findings, &checks);
            plan_moved(&policy, &ledger, &mut planned);
            if !saved {
                break;
            }
        }
        if stopped.is_empty() {
            let active = expanded.as_ref().unwrap_or(references);
            let recited = recite(
                &mut ledger,
                &policy,
                (&mut candidate, &report),
                (&mut *numbered, &mut cited_where_supported),
                active,
            )
            .await?;
            if recited {
                // The added sources are saved before the draft that cites them.
                let added: Vec<&str> = numbered
                    .iter()
                    .skip(writers)
                    .map(|source| source.id.as_str())
                    .collect();
                crate::features::learning::lesson_drafts::record_checkpoint(
                    NUMBERED_KEY,
                    json!(added),
                )
                .await?;
                repair::number_sources(&mut numbered_schema, numbered.len());
                raw = candidate.to_string();
                crate::features::learning::lesson_drafts::save(&raw).await?;
                continue;
            }
            if uncite(&mut ledger, &policy, &mut candidate, active).await? {
                raw = candidate.to_string();
                crate::features::learning::lesson_drafts::save(&raw).await?;
                continue;
            }
        }
        let active = expanded.as_ref().unwrap_or(references);
        match ledger.decide(&policy, &stopped) {
            claim_ledger::Decision::Continue(waiting) => {
                // A pass that moved no statement and changed nothing would go
                // round again as it was.
                if ledger.stepped() == stepped && raw == draft {
                    let error = ledger.stop_no_progress(&raw, &waiting, unreached).await?;
                    crate::features::learning::lesson_progress::phase(
                        crate::features::learning::lesson_progress::Phase::Evidence,
                        error.to_string(),
                    );
                    return Err(error);
                }
                crate::features::learning::lesson_progress::phase(
                    crate::features::learning::lesson_progress::Phase::Evidence,
                    if waiting.iter().any(|waits| waits.step == Step::Research) {
                        "Searching again for the claims the last round of research could not reach"
                    } else {
                        "Reading the statements the last pass left waiting"
                    },
                );
                continue;
            }
            claim_ledger::Decision::Stop(reason) => {
                return Err(stop(&mut ledger, &raw, *reason).await?);
            }
            claim_ledger::Decision::Publish => {
                // The last sync point: an edit made during this pass is
                // applied and verified before anything publishes.
                ledger.sync().await?;
                if review_pass::has_user_changes(&ledger, &candidate)
                    || !matches!(ledger.decide(&policy, &[]), claim_ledger::Decision::Publish)
                {
                    continue;
                }
                // Sections after cuts (claim classes design §3.10): the flow
                // repairs and the sections that go are settled and checked
                // before the lesson is final, and the sections that go are then
                // taken out.
                match sections::settle(
                    llm,
                    (prompt, schema, output_tokens),
                    active,
                    &raw,
                    &mut ledger,
                )
                .await?
                {
                    sections::Settled::Again(next) => {
                        raw = next;
                        continue;
                    }
                    sections::Settled::Stopped(error) => return Err(error),
                    sections::Settled::Done => {}
                }
                // The last guard (acceptance design §2.9): the lesson holds no
                // sentence that a claim the machine ended stood on.
                if let Some((reason, claims)) = ledger.ended_but_standing(&candidate) {
                    return Err(hold(&mut ledger, &raw, claims, Some(reason)).await?);
                }
                (raw, candidate, report) =
                    sections::excise(llm, &mut ledger, raw, candidate, report).await?;
                // Then the Verifier audit, when the course opted in (§14.3):
                // review pass → audit → publish. Its checks, and anything the
                // user did while it ran, are read back before publishing.
                match ledger.audit(&raw, &candidate).await? {
                    claim_ledger::Audited::Off => {}
                    claim_ledger::Audited::Agreed => {
                        ledger.sync().await?;
                        if review_pass::has_user_changes(&ledger, &candidate)
                            || !matches!(
                                ledger.decide(&policy, &[]),
                                claim_ledger::Decision::Publish
                            )
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
                let escalated = targets.iter().any(|id| ledger.cuts_next(&policy, id));
                crate::features::learning::lesson_progress::phase(
                    crate::features::learning::lesson_progress::Phase::Repair,
                    if escalated {
                        "Removing a repeatedly unsupported claim"
                    } else {
                        "Correcting defects found by evidence checks"
                    },
                );
                let mut context = repair_context(
                    prompt,
                    &candidate,
                    &report,
                    (&ledger, &policy),
                    &targets,
                    active,
                )?;
                repair::number_evidence(&mut context, numbered);
                ledger.begin_repairs(&policy, &candidate, &targets).await?;
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
                    &policy,
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

/// The lesson stops for `reason`: the draft is saved, the review row says so,
/// and the returned error unwinds the preparation.
async fn stop(ledger: &mut ClaimLedger, raw: &str, reason: StopReason) -> Result<AppError> {
    crate::features::learning::lesson_drafts::save(raw).await?;
    let error = ledger.stop(raw, reason).await?;
    crate::features::learning::lesson_progress::phase(
        crate::features::learning::lesson_progress::Phase::Evidence,
        error.to_string(),
    );
    Ok(error)
}

/// A claim supported elsewhere in the course keeps its text; the citation of
/// its sentence moves to the passage found, or is added where the sentence has
/// none of its own or shares it with a live statement, and the next pass reads
/// it against that citation (§5.1). The statements at a recite node are the
/// repair's, one claim of a sentence a call: a second waits for the next round
/// with nothing recorded, and a claim that cannot be cited is done, the reading
/// after it telling. A claim the user resolved, on a page their section does
/// not cite, is cited there too, as the call goes: it takes no step and is not
/// read. Whether any statement took its step or was cited.
pub(super) async fn recite(
    ledger: &mut ClaimLedger,
    policy: &dyn AcceptancePolicy,
    (candidate, report): (&mut Value, &LessonVerificationReport),
    (numbered, tried): (&mut Vec<LearningSourceDto>, &mut HashSet<(usize, String)>),
    references: &ReferenceCollection<'_>,
) -> Result<bool> {
    let waiting = ledger.at(policy, StepKind::Recite);
    let resolved = |id: &ClaimId, finding: &Finding| {
        finding.two_pass.supported_by.is_some()
            && matches!(
                ledger.entry(id).map(|entry| entry.state()),
                Some(ClaimState::ResolvedByEvidence { .. })
            )
    };
    let findings: Vec<(ClaimId, Finding)> = ledger
        .finding_claims()
        .iter()
        .zip(&report.findings)
        .filter(|(id, finding)| waiting.contains(id) || resolved(id, finding))
        .map(|(id, finding)| (id.clone(), finding.clone()))
        .collect();
    if findings.is_empty() {
        return Ok(false);
    }
    // A sentence is cited as its field writes it: the claim's located quote,
    // not the extractor's passage.
    let sent: Vec<Finding> = findings
        .iter()
        .map(|(id, finding)| Finding {
            quote: ledger.entry(id).map_or_else(
                || finding.quote.clone(),
                |entry| entry.location().quote.clone(),
            ),
            ..finding.clone()
        })
        .collect();
    let reciting: Vec<ClaimId> = findings.iter().map(|(id, _)| id.clone()).collect();
    let kept = ledger.live_statements_but(&reciting);
    let cited = repair::cite_where_supported(candidate, &sent, &kept, numbered, references, tried)?;
    let mut moved = false;
    for ((id, _), cited) in findings.iter().zip(cited) {
        match cited {
            Cited::Recited(event) => {
                ledger.append(id, *event).await?;
                moved |= !waiting.contains(id);
            }
            Cited::Cannot => {}
            Cited::Deferred => continue,
        }
        if waiting.contains(id) {
            moved |= ledger
                .record_step(policy, id, Step::Recite, Outcome::Done)
                .await?;
        }
    }
    if moved {
        crate::features::learning::lesson_progress::phase(
            crate::features::learning::lesson_progress::Phase::Repair,
            "Citing the passages that support claims their sections cited elsewhere",
        );
    }
    Ok(moved)
}

/// The statements at an uncite node lose the citation of their sentence: every
/// part of them is common knowledge, so the citation was the fault, not the
/// statement. Whether any statement took its step.
async fn uncite(
    ledger: &mut ClaimLedger,
    policy: &dyn AcceptancePolicy,
    candidate: &mut Value,
    references: &ReferenceCollection<'_>,
) -> Result<bool> {
    let waiting = ledger.at(policy, StepKind::Uncite);
    if waiting.is_empty() {
        return Ok(false);
    }
    let statements: Vec<(usize, String)> = waiting
        .iter()
        .filter_map(|id| ledger.entry(id))
        .map(|entry| (entry.location().unit_index, entry.statement().to_owned()))
        .collect();
    let kept = ledger.live_statements_but(&waiting);
    crate::features::learning::lesson_progress::phase(
        crate::features::learning::lesson_progress::Phase::Repair,
        "Dropping the citation of statements whose every part is common knowledge",
    );
    let dropped = repair::uncite(candidate, &statements, &kept, references)?;
    let mut moved = false;
    for (id, cited) in waiting.iter().zip(dropped) {
        ledger
            .append(id, ClaimEvent::CitationDropped { cited })
            .await?;
        moved |= ledger
            .record_step(policy, id, Step::Uncite, Outcome::Done)
            .await?;
    }
    Ok(moved)
}

/// Tells the call plan where the ledger has its statements now. They are the
/// claims of the first pass the ledger observed, in its order, each followed
/// to the claim that carries its sentence on.
pub(super) fn plan_moved(
    policy: &dyn AcceptancePolicy,
    ledger: &ClaimLedger,
    planned: &mut Vec<ClaimId>,
) {
    if planned.is_empty() {
        planned.extend_from_slice(ledger.finding_claims());
    }
    crate::features::learning::lesson_progress::statements_moved(
        policy,
        &ledger.planned(policy, planned),
    );
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
fn still_repairable(
    ledger: &ClaimLedger,
    policy: &dyn AcceptancePolicy,
    mut context: Value,
) -> Option<Value> {
    let keep = |item: &Value| {
        item.get("claimId")
            .and_then(Value::as_str)
            .is_some_and(|id| ledger.may_repair(policy, &ClaimId(id.to_owned())))
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

/// The repair context, with each exact passage sent once rather than per
/// finding; nothing is discarded. Passages carry their source titles so a
/// source index is not read as evidence N. Only `targets` are sent, each with
/// its claim id and, at a cut, `lastAttempt`: that attempt takes the sentence
/// out in code, so the model never reads the flag. A claim whose sentence says
/// more carries `alsoStates`, the other statements on it. A claim at a rewrite
/// carries its node's `goal`, and at the retry of a rewrite `notApplied`, why
/// its first correction could not be applied. `repairHistory` is what the
/// ledger holds of the sentences earlier corrections replaced or removed.
pub(super) fn repair_context(
    prompt: &str,
    candidate: &Value,
    report: &LessonVerificationReport,
    (ledger, policy): (&ClaimLedger, &dyn AcceptancePolicy),
    targets: &[ClaimId],
    references: &ReferenceCollection<'_>,
) -> Result<Value> {
    let escalates = |id: &ClaimId| ledger.cuts_next(policy, id);
    let mates: HashMap<ClaimId, Vec<ClaimId>> = targets
        .iter()
        .map(|id| (id.clone(), ledger.cotenants(id)))
        .collect();
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
            let mut claim = json!({"unit":finding.unit,"claimId":id.0,"statement":citations.label(&finding.statement, references),"sentence":sentence,"verdict":finding.verdict,"reason":finding.reason,"evidence":evidence});
            if let Some(fields) = claim.as_object_mut() {
                if escalates(id) {
                    fields.insert("lastAttempt".into(), json!(true));
                }
                // What the corrected sentence must do, which the reading
                // after it tests, and why the first could not be applied.
                if let Some((goal, why)) = ledger.rewrite_ask(policy, id) {
                    fields.insert("goal".into(), json!(goal_words(goal)));
                    if let Some(reason) = why {
                        fields.insert("notApplied".into(), json!(reason));
                    }
                }
                // What the sentence says besides the failed claim, which the
                // rewrite keeps (claim classes design §3.2).
                let also: Vec<&str> = mates
                    .get(id)
                    .into_iter()
                    .flatten()
                    .filter(|mate| !targets.contains(mate))
                    .filter_map(|mate| ledger.entry(mate).map(|entry| entry.statement()))
                    .collect();
                if !also.is_empty() {
                    fields.insert("alsoStates".into(), json!(also));
                }
            }
            claim
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
        "exampleClaims":examples.iter().map(|(observation, id)| {
            let mut example = json!({"unit":observation.unit,"claimId":id.0});
            if escalates(id) {
                if let Some(fields) = example.as_object_mut() {
                    fields.insert("lastAttempt".into(), json!(true));
                }
            }
            example
        }).collect::<Vec<_>>(),
        "repairHistory":ledger.repair_history(),
    }))
}
