//! The verify-and-repair loop: the preparation job's review of a lesson, whose
//! steps `round::run` takes in order.
use super::claim_ledger::{self, ClaimLedger, StepKind};
use super::passes;
use super::readings::Readers;
use super::repair::Cited;
use super::repair_request::{repair_context, repair_fingerprint, PendingRepair, Restart};
use super::round::{self, Audit, Researched, Review, Settled, Unsettled, Verdict};
use super::*;
use crate::lessons::shape::{LessonSchema, LessonShape, Phase, ShapePolicy};
use crate::lessons::trace::{self, Cause, Stage};
use async_trait::async_trait;
use lattice_core::domain::learning::acceptance::{
    AcceptancePolicy, Outcome, Position, StandardPolicy, Step, Taken,
};
use lattice_core::domain::learning::claim_review::{ClaimEvent, ClaimId, ClaimState, StopReason};

/// [`verify_and_repair_with_references`] over lexical references, for tests.
#[cfg(test)]
pub(crate) async fn verify_and_repair(
    llm: &dyn LLMPort,
    prompt: &str,
    schema: &Value,
    raw: String,
    sources: &[LearningSourceDto],
    output_tokens: usize,
) -> Result<(String, LessonVerificationReport)> {
    let references = ReferenceCollection::lexical(sources)?;
    let shape = crate::lessons::shape::FixedSchema(schema.clone());
    let mut numbered = sources.to_vec();
    let started = restart(llm, prompt, &shape, &raw, &references, &mut numbered).await?;
    verify_and_repair_with_references(
        llm,
        &BuiltinRuntimes::for_tests(),
        prompt,
        &shape,
        raw,
        &references,
        (output_tokens, &mut numbered),
        started,
    )
    .await
}

/// The checkpoint holding the sources this lesson's verification numbered to
/// cite them, by version id, in the order it numbered them.
const NUMBERED_KEY: Protocol = lattice_core::protocol!("lesson-numbered-sources", 1);

/// `numbered` with the sources an earlier run of this preparation numbered
/// to cite them appended, in their order, so the draft's source indices name
/// the same sources after a restart. A source no longer in `references` is
/// left out.
pub(super) async fn renumber(
    numbered: &mut Vec<LearningSourceDto>,
    references: &ReferenceCollection<'_>,
) -> Result<()> {
    let Some(saved) = crate::lesson_drafts::checkpoint(NUMBERED_KEY.as_str())
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
/// name; the lesson is parsed against it and the kept schema's indices are
/// widened to it. Verification holds the lesson to what it is kept with
/// (`Phase::Kept`), so it builds that schema from `shape` itself.
///
/// `restart` is what [`restart`] made of an earlier attempt: the writer's
/// source count and the repair it saved.
pub(crate) async fn verify_and_repair_with_references(
    llm: &dyn LLMPort,
    runtimes: &BuiltinRuntimes,
    prompt: &str,
    shape: &dyn ShapePolicy,
    raw: String,
    references: &ReferenceCollection<'_>,
    (output_tokens, numbered): (usize, &mut Vec<LearningSourceDto>),
    restart: Restart,
) -> Result<(String, LessonVerificationReport)> {
    let policy = StandardPolicy::new();
    let mut review = open(
        llm,
        runtimes,
        prompt,
        shape,
        raw,
        references,
        (output_tokens, numbered),
        restart,
    )
    .await?;
    round::run(&mut review, &policy).await
}

/// The review of `raw`, held to the kept lesson's schema: the one choice of
/// the promise verification holds a lesson to.
async fn open<'a, 'r>(
    llm: &'a dyn LLMPort,
    runtimes: &'a BuiltinRuntimes,
    prompt: &'a str,
    shape: &'a dyn ShapePolicy,
    raw: String,
    references: &'a ReferenceCollection<'r>,
    (output_tokens, numbered): (usize, &'a mut Vec<LearningSourceDto>),
    restart: Restart,
) -> Result<PreparationReview<'a, 'r>> {
    let readers = Readers::standard();
    let schema = shape.schema(Phase::Kept, numbered.len());
    let ledger = ClaimLedger::open(llm, &raw).await?;
    Ok(PreparationReview {
        llm,
        runtimes,
        prompt,
        output_tokens,
        references,
        numbered,
        writers: restart.writers,
        saved: restart.saved,
        shape,
        schema,
        readers,
        ledger,
        raw,
        expanded: None,
        checks: ClaimChecks::default(),
        verified: None,
        cited_where_supported: HashSet::new(),
        candidate: Value::Null,
        extraction: None,
        pass: None,
        report: None,
    })
}

/// The step that settles the sections after cuts, on a review opened as
/// `verify_and_repair_with_references` opens it, with each of `going` sent
/// out first: whether the lesson was settled.
#[cfg(test)]
pub(crate) async fn settle_after_cuts(
    llm: &dyn LLMPort,
    prompt: &str,
    shape: &dyn ShapePolicy,
    raw: String,
    references: &ReferenceCollection<'_>,
    going: &[(usize, &str)],
) -> Result<bool> {
    let runtimes = BuiltinRuntimes::for_tests();
    let mut numbered = Vec::new();
    let mut review = open(
        llm,
        &runtimes,
        prompt,
        shape,
        raw,
        references,
        (4_000, &mut numbered),
        Restart {
            writers: 0,
            saved: None,
        },
    )
    .await?;
    for (unit, label) in going {
        review.ledger.cuts.go(*unit, (*label).to_owned());
    }
    Ok(matches!(review.settle().await?, Settled::Done))
}

/// The preparation job's review of a lesson (acceptance design §2.7): the
/// steps `round::run` takes, on the job's draft, its claim ledger and the
/// course it cites.
struct PreparationReview<'a, 'r> {
    llm: &'a dyn LLMPort,
    runtimes: &'a BuiltinRuntimes,
    prompt: &'a str,
    output_tokens: usize,
    references: &'a ReferenceCollection<'r>,
    numbered: &'a mut Vec<LearningSourceDto>,
    /// How many of `numbered` the writer was given.
    writers: usize,
    /// The repair an earlier attempt saved for this draft, until it resumes.
    saved: Option<PendingRepair>,
    shape: &'a dyn ShapePolicy,
    /// The kept schema, its source indices widened to `numbered`.
    schema: LessonSchema,
    readers: Readers,
    ledger: ClaimLedger,
    raw: String,
    /// The course's references with what research saved, once it saved any.
    expanded: Option<ReferenceCollection<'r>>,
    checks: ClaimChecks,
    verified: Option<String>,
    /// Citations this job repaired to where a claim's support was found.
    cited_where_supported: HashSet<(usize, String, String)>,
    /// The draft as this round parsed it, and what its extraction,
    /// identification and readings left.
    candidate: Value,
    extraction: Option<passes::Extraction>,
    pass: Option<passes::Pass>,
    report: Option<LessonVerificationReport>,
}

/// The error of a step of the round taken before the step that leaves what it
/// reads.
fn out_of_order(what: &str) -> AppError {
    AppError::InternalError(format!(
        "A step of the round ran before its {what} was made"
    ))
}

#[async_trait]
impl Review for PreparationReview<'_, '_> {
    type Finished = (String, LessonVerificationReport);

    async fn sync(&mut self, policy: &dyn AcceptancePolicy) -> Result<()> {
        self.raw = normalize_example_fences(self.llm, std::mem::take(&mut self.raw)).await?;
        self.candidate = crate::generation::parse_json(&self.raw)?;
        self.ledger.sync().await?;
        let active = self.expanded.as_ref().unwrap_or(self.references);
        self.checks.settled = self.ledger.user_evidence(active);
        self.checks.rewrites = self.ledger.rewrites();
        self.checks.removed = self.ledger.cuts.units();
        if review_pass::apply_user_changes(&mut self.ledger, &mut self.candidate).await? {
            let edited = self.candidate.to_string();
            trace::changed(Stage::UserEdit, &Cause::none(), &self.raw, &edited);
            self.raw = edited;
            self.saved = None;
            crate::lesson_drafts::save(&self.raw).await?;
            // The user's text gets the teaching review a rewrite gets (§6.2).
            let protected =
                repair::teaching_protected(&mut self.ledger, policy, &self.candidate, &[]).await?;
            self.raw = crate::teaching::review_lesson(
                self.llm,
                crate::teaching::LESSON_REPAIR_SYSTEM,
                self.prompt,
                LessonShape::new(&self.schema, self.shape),
                std::mem::take(&mut self.raw),
                self.output_tokens,
                (active, &*self.numbered),
                &protected,
            )
            .await?;
            self.candidate = crate::generation::parse_json(&self.raw)?;
        }
        Ok(())
    }

    async fn resume(&mut self, policy: &dyn AcceptancePolicy) -> Result<bool> {
        let active = self.expanded.as_ref().unwrap_or(self.references);
        let Some(saved) = self.saved.take() else {
            return Ok(false);
        };
        let Some(context) = still_repairable(&self.ledger, policy, saved.context) else {
            // Everything the saved repair was for was resolved meanwhile.
            crate::lesson_drafts::save_repair(&self.raw, None).await?;
            return Ok(false);
        };
        crate::lesson_progress::phase(
            crate::lesson_progress::Phase::Repair,
            "Resuming the saved evidence-based repair",
        );
        trace::decided(
            Stage::ClaimRepair,
            "lesson",
            "resuming a saved repair",
            "the repair an earlier attempt saved for this draft still has claims to repair",
        );
        self.raw = crate::lesson_progress::checks(repair::candidate(
            self.llm,
            policy,
            self.prompt,
            LessonShape::new(&self.schema, self.shape),
            context,
            self.output_tokens,
            (active, &*self.numbered),
            &mut self.ledger,
        ))
        .await?;
        Ok(true)
    }

    fn stepped(&self) -> usize {
        self.ledger.stepped()
    }

    fn draft(&self) -> &str {
        &self.raw
    }

    async fn extract(&mut self) -> Result<()> {
        if self.verified.as_deref() != Some(self.raw.as_str()) {
            crate::lesson_progress::new_pass();
            self.verified = Some(self.raw.clone());
        }
        let extraction =
            passes::extract(self.llm, self.runtimes, &self.candidate, &self.checks).await?;
        self.extraction = Some(extraction);
        Ok(())
    }

    async fn identify(&mut self, policy: &dyn AcceptancePolicy) -> Result<()> {
        let extraction = self
            .extraction
            .take()
            .ok_or_else(|| out_of_order("extraction"))?;
        let active = self.expanded.as_ref().unwrap_or(self.references);
        let pass = crate::lesson_progress::checks(passes::identify(
            self.llm,
            policy,
            &mut self.ledger,
            (&self.candidate, active),
            extraction,
            &self.checks,
        ))
        .await?;
        self.pass = Some(pass);
        // The plan sees each successor where it starts and takes in each
        // statement new to the ledger, before either is read.
        plan_moved(policy, &self.ledger, &self.candidate);
        Ok(())
    }

    fn fault(&self, policy: &dyn AcceptancePolicy) -> Option<StopReason> {
        self.ledger.fault(policy)
    }

    async fn read(&mut self, policy: &dyn AcceptancePolicy) -> Result<Vec<ClaimId>> {
        let active = self.expanded.as_ref().unwrap_or(self.references);
        let pass = self.pass.as_ref().ok_or_else(|| out_of_order("pass"))?;
        let due = self.ledger.due(policy);
        let readings = crate::lesson_progress::checks(passes::read_steps(
            self.llm,
            policy,
            &self.readers,
            &self.ledger,
            &self.checks,
            (&self.candidate, active),
            (pass, &due),
        ))
        .await?;
        let stopped =
            passes::record_readings(policy, &mut self.ledger, &mut self.checks, readings).await?;
        let findings = passes::latest_findings(&self.ledger, &mut self.checks, pass).await?;
        let report = passes::report(
            self.llm,
            active,
            &self.candidate,
            pass,
            findings,
            &self.checks,
        );
        self.report = Some(report);
        plan_moved(policy, &self.ledger, &self.candidate);
        Ok(stopped)
    }

    async fn unmend(&mut self) -> Result<Option<String>> {
        sections::unmend(&mut self.ledger, &self.candidate).await
    }

    async fn restore(&mut self, draft: String) -> Result<()> {
        trace::changed(Stage::FlowRepair, &Cause::none(), &self.raw, &draft);
        self.raw = draft;
        crate::lesson_drafts::save(&self.raw).await
    }

    async fn research(&mut self, policy: &dyn AcceptancePolicy) -> Result<Researched> {
        let report = self.report.as_ref().ok_or_else(|| out_of_order("report"))?;
        let pass = self.pass.as_ref().ok_or_else(|| out_of_order("pass"))?;
        let targets = self.ledger.research_targets(policy, report);
        if targets.claims.is_empty() {
            return Ok(Researched::Nothing);
        }
        let active = self.expanded.as_ref().unwrap_or(self.references);
        let expansion = research::expand(
            self.llm,
            active,
            &targets.findings,
            &targets.hints,
            &self.checks,
        )
        .boxed()
        .await?;
        let (searched, saved) = match expansion {
            research::Expansion::Saved(additional, searched) => {
                self.expanded = Some(*additional);
                (searched, true)
            }
            research::Expansion::Unchanged(research::Searched {
                unresearched: Some(cause),
                ..
            }) => {
                tracing::info!(
                    ?cause,
                    "claim.state: research could not run; the claims stay at their node"
                );
                trace::decided(
                    Stage::Research,
                    "research round",
                    "could not run",
                    &format!("{cause:?}; the claims stay at their node"),
                );
                return Ok(Researched::Unresearched(cause));
            }
            research::Expansion::Unchanged(searched) => (searched, false),
            research::Expansion::Exhausted(exhausted, searched) => {
                tracing::info!(%exhausted, "claim.state: research found nothing new for these claims");
                trace::decided(
                    Stage::Research,
                    "research round",
                    "found nothing new",
                    &exhausted.to_string(),
                );
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
        let active = self.expanded.as_ref().unwrap_or(self.references);
        let due: Vec<(ClaimId, Position)> = self
            .ledger
            .at(policy, StepKind::Research)
            .into_iter()
            .filter_map(|id| {
                let step = Step::Research;
                let next = self.ledger.after(policy, &id, Taken { step, outcome })?;
                let node = policy.graph(next.class).node(next.node)?;
                matches!(node.step, Step::Read(_)).then_some((id, next))
            })
            .collect();
        if saved {
            crate::lesson_progress::new_references(Some(searched.sources_added.len()));
        }
        let read = async {
            let readings = crate::lesson_progress::checks(passes::read_steps(
                self.llm,
                policy,
                &self.readers,
                &self.ledger,
                &self.checks,
                (&self.candidate, active),
                (pass, &due),
            ))
            .await?;
            // A hint's reading answers the user, whose claim no graph walks:
            // it is no check of the plan's statements.
            let hinted = if saved {
                passes::read_hinted(
                    self.llm,
                    &self.readers,
                    &self.ledger,
                    &self.checks,
                    (&self.candidate, active),
                    (pass, &targets.hinted),
                )
                .await?
            } else {
                Vec::new()
            };
            Ok::<_, AppError>((readings, hinted))
        }
        .await;
        crate::lesson_progress::new_references(None);
        let (readings, hinted) = read?;
        let found = |id: &ClaimId| {
            let finding = readings
                .iter()
                .find(|(read, _)| read == id)
                .and_then(|(_, walk)| walk.steps.first())
                .map(|step| step.read.finding())
                .or_else(|| {
                    hinted
                        .iter()
                        .find(|(read, _)| read == id)
                        .map(|(_, read)| read.finding())
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
        self.ledger
            .record_research(policy, &round, &searched, saved)
            .await?;
        let stopped =
            passes::record_readings(policy, &mut self.ledger, &mut self.checks, readings).await?;
        passes::record_hinted(&mut self.ledger, &mut self.checks, hinted).await?;
        let findings = passes::latest_findings(&self.ledger, &mut self.checks, pass).await?;
        let report = passes::report(
            self.llm,
            active,
            &self.candidate,
            pass,
            findings,
            &self.checks,
        );
        self.report = Some(report);
        plan_moved(policy, &self.ledger, &self.candidate);
        Ok(Researched::Searched { saved, stopped })
    }

    async fn recite(&mut self, policy: &dyn AcceptancePolicy) -> Result<bool> {
        let active = self.expanded.as_ref().unwrap_or(self.references);
        let report = self.report.as_ref().ok_or_else(|| out_of_order("report"))?;
        let recited = recite(
            &mut self.ledger,
            policy,
            (&mut self.candidate, report),
            (&mut *self.numbered, &mut self.cited_where_supported),
            active,
        )
        .await?;
        if recited {
            // The added sources are saved before the draft that cites them.
            let added: Vec<&str> = self
                .numbered
                .iter()
                .skip(self.writers)
                .map(|source| source.id.as_str())
                .collect();
            crate::lesson_drafts::record_checkpoint(NUMBERED_KEY.as_str(), json!(added)).await?;
            self.schema = self.schema.numbered(self.shape, self.numbered.len());
            let cited = self.candidate.to_string();
            trace::changed(Stage::ClaimRepair, &Cause::none(), &self.raw, &cited);
            self.raw = cited;
            crate::lesson_drafts::save(&self.raw).await?;
        }
        Ok(recited)
    }

    async fn uncite(&mut self, policy: &dyn AcceptancePolicy) -> Result<bool> {
        let active = self.expanded.as_ref().unwrap_or(self.references);
        if !uncite(&mut self.ledger, policy, &mut self.candidate, active).await? {
            return Ok(false);
        }
        let uncited = self.candidate.to_string();
        trace::changed(Stage::ClaimRepair, &Cause::none(), &self.raw, &uncited);
        self.raw = uncited;
        crate::lesson_drafts::save(&self.raw).await?;
        Ok(true)
    }

    fn decide(&self, policy: &dyn AcceptancePolicy, stopped: &[ClaimId]) -> claim_ledger::Decision {
        self.ledger.decide(policy, stopped)
    }

    fn hinted_waiting(&self) -> Vec<claim_ledger::Waiting> {
        self.ledger.hinted_waiting()
    }

    fn again(&mut self, waiting: &[claim_ledger::Waiting]) {
        crate::lesson_progress::phase(
            crate::lesson_progress::Phase::Evidence,
            if waiting.iter().any(|waits| waits.step == Step::Research) {
                "Searching again for the statements the last round of research could not reach"
            } else {
                "Reading the statements the last pass left waiting"
            },
        );
    }

    async fn repair(&mut self, policy: &dyn AcceptancePolicy, targets: &[ClaimId]) -> Result<()> {
        let active = self.expanded.as_ref().unwrap_or(self.references);
        let report = self.report.as_ref().ok_or_else(|| out_of_order("report"))?;
        let escalated = targets.iter().any(|id| self.ledger.cuts_next(policy, id));
        crate::lesson_progress::phase(
            crate::lesson_progress::Phase::Repair,
            if escalated {
                "Removing a repeatedly unsupported statement"
            } else {
                "Correcting defects found by evidence checks"
            },
        );
        trace::decided(
            Stage::ClaimRepair,
            "lesson",
            if escalated {
                "repair escalated to a cut"
            } else {
                "repair started"
            },
            &format!(
                "claims {:?}",
                targets.iter().map(|id| id.0.as_str()).collect::<Vec<_>>()
            ),
        );
        let mut context = repair_context(
            self.prompt,
            &self.candidate,
            report,
            (&self.ledger, policy),
            targets,
            active,
        )?;
        if let Some(requirements) = context.get_mut("requirements") {
            LessonShape::new(&self.schema, self.shape).state_in(requirements);
        }
        repair::number_evidence(&mut context, self.numbered);
        self.ledger
            .begin_repairs(policy, &self.candidate, targets)
            .await?;
        let pending = PendingRepair {
            fingerprint: repair_fingerprint(
                self.llm,
                self.prompt,
                self.schema.value(),
                &self.candidate,
                active,
            ),
            defects: targets.len(),
            context: context.clone(),
        };
        crate::lesson_drafts::record_pending_repair(
            &self.candidate.to_string(),
            serde_json::to_string(&pending)?,
        )
        .await?;
        self.raw = crate::lesson_progress::checks(repair::candidate(
            self.llm,
            policy,
            self.prompt,
            LessonShape::new(&self.schema, self.shape),
            context,
            self.output_tokens,
            (active, &*self.numbered),
            &mut self.ledger,
        ))
        .await?;
        Ok(())
    }

    async fn still_publishes(&mut self, policy: &dyn AcceptancePolicy) -> Result<bool> {
        self.ledger.sync().await?;
        Ok(
            !review_pass::has_user_changes(&self.ledger, &self.candidate)
                && matches!(
                    self.ledger.decide(policy, &[]),
                    claim_ledger::Decision::Publish
                ),
        )
    }

    async fn settle(&mut self) -> Result<Settled> {
        let active = self.expanded.as_ref().unwrap_or(self.references);
        let settled = crate::lesson_progress::checks(sections::settle(
            self.llm,
            (
                self.prompt,
                LessonShape::new(&self.schema, self.shape),
                self.output_tokens,
            ),
            (active, &*self.numbered),
            &self.raw,
            &mut self.ledger,
        ))
        .await?;
        match settled {
            sections::Settled::Again(next) => {
                self.raw = next;
                Ok(Settled::Again)
            }
            sections::Settled::Stopped(error) => Err(error),
            sections::Settled::Done => Ok(Settled::Done),
        }
    }

    async fn publish_unsettled(&mut self, unsettled: Unsettled) -> Result<usize> {
        self.ledger.sync().await?;
        self.ledger
            .publish_unsettled(unsettled, &self.candidate)
            .await
    }

    fn ended_but_standing(&self) -> Option<(StopReason, usize)> {
        self.ledger.ended_but_standing(&self.candidate)
    }

    async fn excise(&mut self) -> Result<()> {
        let report = self.report.take().ok_or_else(|| out_of_order("report"))?;
        let before = std::mem::take(&mut self.raw);
        let (raw, candidate, report) = sections::excise(
            self.llm,
            &mut self.ledger,
            before.clone(),
            std::mem::take(&mut self.candidate),
            report,
        )
        .await?;
        trace::changed(Stage::ClaimRepair, &Cause::none(), &before, &raw);
        self.raw = raw;
        self.candidate = candidate;
        self.report = Some(report);
        Ok(())
    }

    async fn audit(&mut self) -> Result<Audit> {
        Ok(match self.ledger.audit(&self.raw, &self.candidate).await? {
            claim_ledger::Audited::Off => Audit::Off,
            claim_ledger::Audited::Agreed => {
                trace::decided(
                    Stage::Audit,
                    "lesson",
                    "agreed",
                    "the Verifier agrees with every claim",
                );
                Audit::Agreed
            }
            claim_ledger::Audited::Hold(reason) => {
                self.ledger.sync().await?;
                let claims = self.ledger.awaiting_user();
                trace::decided(
                    Stage::Audit,
                    "lesson",
                    "held for the user",
                    &format!(
                        "{claims} claims wait; {}",
                        reason
                            .as_ref()
                            .map_or("the audit could not finish", |stop| stop.message.as_str())
                    ),
                );
                Audit::Hold { claims, reason }
            }
        })
    }

    async fn finish(&mut self, verdict: Verdict) -> Result<(String, LessonVerificationReport)> {
        match verdict {
            Verdict::Publish => {
                trace::decided(
                    Stage::ClaimVerification,
                    "lesson",
                    "every claim is resolved",
                    "the lesson's verification is complete and it goes on to publish",
                );
                let report = self.report.take().ok_or_else(|| out_of_order("report"))?;
                Ok((std::mem::take(&mut self.raw), self.ledger.bind(report)))
            }
            Verdict::Hold { claims, reason } => {
                Err(hold(&mut self.ledger, &self.raw, claims, reason).await?)
            }
            Verdict::Stop(reason) => Err(stop(&mut self.ledger, &self.raw, reason).await?),
            Verdict::NoProgress {
                waiting,
                unresearched,
            } => {
                trace::decided(
                    Stage::ClaimVerification,
                    "lesson",
                    "a round moved nothing",
                    &format!("{} claims wait at their nodes", waiting.len()),
                );
                let error = self
                    .ledger
                    .stop_no_progress(&self.raw, &waiting, unresearched)
                    .await?;
                crate::lesson_progress::phase(
                    crate::lesson_progress::Phase::Evidence,
                    error.to_string(),
                );
                Err(error)
            }
        }
    }
}

/// The lesson stops for `reason`: the draft is saved, the review row says so,
/// and the returned error unwinds the preparation.
async fn stop(ledger: &mut ClaimLedger, raw: &str, reason: StopReason) -> Result<AppError> {
    crate::lesson_drafts::save(raw).await?;
    let error = ledger.stop(raw, reason).await?;
    crate::lesson_progress::phase(crate::lesson_progress::Phase::Evidence, error.to_string());
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
    (numbered, tried): (
        &mut Vec<LearningSourceDto>,
        &mut HashSet<(usize, String, String)>,
    ),
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
    let kept = ledger.live_sentences_but(&reciting);
    let cited = repair::cite_where_supported(candidate, &sent, &kept, numbered, references, tried)?;
    let mut moved = false;
    for ((id, finding), cited) in findings.iter().zip(cited) {
        match cited {
            Cited::Recited(event) => {
                ledger.append(id, *event).await?;
                moved |= !waiting.contains(id);
            }
            Cited::Cannot(unwritten) => {
                trace::citation_kept(id, &finding.statement, unwritten.why());
            }
            Cited::Deferred => continue,
        }
        if waiting.contains(id) {
            moved |= ledger
                .record_step(policy, id, Step::Recite, Outcome::Done)
                .await?;
        }
    }
    if moved {
        crate::lesson_progress::phase(
            crate::lesson_progress::Phase::Repair,
            "Citing the passages that support statements their sections cited elsewhere",
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
    let sentences: Vec<(usize, String)> = waiting
        .iter()
        .filter_map(|id| ledger.entry(id))
        .map(|entry| (entry.location().unit_index, entry.location().quote.clone()))
        .collect();
    let kept = ledger.live_sentences_but(&waiting);
    crate::lesson_progress::phase(
        crate::lesson_progress::Phase::Repair,
        "Dropping the citation of statements whose every part is common knowledge",
    );
    let dropped = repair::uncite(candidate, &sentences, &kept, references)?;
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

/// Tells the call plan where the ledger has its statements now in
/// `candidate`, and takes into it the claims of the latest pass that continue
/// none of its own; a job with no plan makes it from them. Its own are the
/// claims it took in, in its order, each followed to the claim that carries
/// its sentence on.
pub(super) fn plan_moved(policy: &dyn AcceptancePolicy, ledger: &ClaimLedger, candidate: &Value) {
    let Some(mut claims) = crate::lesson_progress::plan_claims() else {
        return;
    };
    let held = claims.len();
    let joining = ledger.unplanned(policy, candidate, &mut claims);
    let moved = ledger.planned(policy, candidate, claims.get(..held).unwrap_or_default());
    crate::lesson_progress::statements_moved(
        policy,
        ledger.finding_sections(),
        &claims,
        &moved,
        &joining,
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
    crate::lesson_drafts::save(raw).await?;
    let message = match &stop_reason {
        Some(stop) => stop.message.clone(),
        None => format!(
            "Needs your review: {claims} statement{} could not be verified",
            if claims == 1 { "" } else { "s" }
        ),
    };
    ledger.stop_for_review(raw, stop_reason).await?;
    crate::lesson_progress::phase(crate::lesson_progress::Phase::Repair, message);
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
