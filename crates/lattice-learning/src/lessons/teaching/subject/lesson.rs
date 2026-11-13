//! The lesson as a subject of the review.
use super::super::contract;
use super::super::passes::{Next, Outcome, Passes};
use super::super::quotes::{numbered, quote_checks_in, QuoteCheck};
use super::super::repair::{self, Repaired};
use super::super::standing::{versions, Standing};
use super::super::{
    holds_up_review, quotation_findings, structural_findings, whole_repairs,
    LESSON_REVIEW_EVIDENCE_SCOPE, RULES_DECIDE,
};
use super::{
    review_instructions, review_prompt, settle_quotes_in, Decision, Found, Pass, ReviewTexts,
    Settled, Step, Subject, LESSON_PRIORITY,
};
use crate::dto::LearningSourceDto;
use crate::lessons::content_verification::Protected;
use crate::lessons::copied_wording::Wording;
use crate::lessons::text_edits::{self, AppliedFor, Named, Patch};
use crate::model_call::ErrorClass;
use crate::outline_progress::OutlineProgress;
use crate::reference_collection::ReferenceCollection;
use crate::review_evidence::Checked;
use crate::review_findings::{
    new_sentences, part_at, sentences_of, FindingSite, Held, ReviewFinding, SetAside,
};
use lattice_core::{
    application::ports::LLMPort,
    shared::error::{AppError, Result},
};
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashSet};

/// Why a finding about a sentence a live claim stands on is held.
const PROTECTED_SENTENCE: &str = "a sentence a live claim stands on";

/// What the lesson's reviewer is given besides the material, by check.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Beside {
    QuestionKeys,
    Plan,
    Rules,
}

impl Beside {
    /// The checks that read a lesson beside the reviewer, in the order the gate is given them.
    const ALL: [Self; 3] = [Self::QuestionKeys, Self::Plan, Self::Rules];

    fn decides(self) -> &'static str {
        match self {
            Self::QuestionKeys => crate::question_review::DECIDES,
            Self::Plan => contract::DECIDES,
            Self::Rules => RULES_DECIDE,
        }
    }
}

/// The plan findings of a pass: the first ask's, then those of the second
/// that the first did not give.
fn plan_findings_of(first: Vec<ReviewFinding>, second: Vec<ReviewFinding>) -> Vec<ReviewFinding> {
    let given: HashSet<_> = first.iter().map(ReviewFinding::id).collect();
    let mut all = first;
    all.extend(
        second
            .into_iter()
            .filter(|finding| !given.contains(&finding.id())),
    );
    all
}

/// A lesson: it publishes with what the repairs could not settle waiting for
/// the user, and holds what its live claims stand on.
pub(in crate::lessons::teaching) struct LessonSubject<'a> {
    passes: Passes,
    receipts: contract::Receipts,
    protected: &'a [Protected],
    references: &'a ReferenceCollection<'a>,
    /// The lesson's numbered sources: what its `sourceIndex`es name.
    numbered: &'a [LearningSourceDto],
    wording: Wording,
    standing: Standing,
    /// Whether structure held up the latest pass's content review.
    held_up: bool,
}

impl<'a> LessonSubject<'a> {
    /// `protected` are the sentences live claims stand on after a claim repair
    /// or a user edit (claim sources design §10.3); `held` the findings the
    /// user already has, never repaired again.
    pub(in crate::lessons::teaching) fn new(
        references: &'a ReferenceCollection<'a>,
        numbered: &'a [LearningSourceDto],
        protected: &'a [Protected],
        held: Held,
    ) -> Self {
        let standing = Standing::new(&held);
        Self {
            passes: Passes::new(held),
            receipts: contract::Receipts::default(),
            protected,
            references,
            numbered,
            wording: Wording::of(references),
            standing,
            held_up: false,
        }
    }

    /// One line for each finding the repairs left for the user: what was tried.
    fn log_waiting(&self, set_aside: &[SetAside]) {
        let lesson = crate::lesson_drafts::scope()
            .map(|scope| scope.lesson_id)
            .unwrap_or_default();
        for waiting in set_aside {
            let account = self.passes.account(&waiting.finding);
            tracing::info!(
                lesson = %lesson,
                finding = %waiting.finding.id().as_str(),
                part = %waiting.finding.site.unwrap_or(FindingSite::Lesson).plain(),
                repairs = waiting.repairs,
                inherited = account.inherited,
                held = %account.held,
                attempts = %account.attempts,
                "A teaching finding waits for the user"
            );
        }
    }

    /// Each reference's id and the digest of its excerpt.
    fn reference_pairs(&self) -> Vec<(String, String)> {
        versions(&self.references.sources)
    }

    /// Each reference's id and the digest of its excerpt, as the reviewer is shown them.
    fn reference_versions(&self) -> Vec<Value> {
        self.reference_pairs()
            .into_iter()
            .map(|(id, hash)| json!({"id":id,"contentHash":hash}))
            .collect()
    }
}

#[async_trait::async_trait]
impl Subject for LessonSubject<'_> {
    fn progress(&self) -> Option<&OutlineProgress> {
        None
    }

    fn wording(&self) -> &Wording {
        &self.wording
    }

    fn step(&self, step: Step) {
        let phase = crate::lesson_progress::Phase::Review;
        match step {
            Step::CheckingKeys => {
                crate::lesson_progress::phase(phase, "Checking assessment answer keys")
            }
            Step::Reviewing => {
                crate::lesson_progress::phase(phase, "Reviewing every teaching section")
            }
            Step::Rechecking => {
                crate::lesson_progress::phase(phase, "Rechecking the revised lesson")
            }
            Step::Repairing => {
                crate::lesson_progress::stage("Correcting the lesson from review findings")
            }
        }
    }

    async fn resume(&mut self, _candidate: &str) -> Result<()> {
        // A review takes up what the job's earlier reviews judged and spent, whichever draft they saved it with.
        if let Some(saved) = crate::lesson_drafts::teaching_passes().await? {
            self.passes.resume(&saved);
            self.standing.resume(&saved);
            self.receipts.resume(&saved);
        }
        Ok(())
    }

    fn given(&mut self, llm: &dyn LLMPort, authoring: &Value) {
        let inputs = json!({"authoringContext":authoring,"correctnessPriority":LESSON_PRIORITY});
        self.standing.under(&crate::teaching_review::basis(
            llm,
            &ReviewTexts::LESSON.joined(),
            LESSON_REVIEW_EVIDENCE_SCOPE,
            &inputs,
        ));
        let references = self.reference_pairs();
        self.standing.against(&references);
    }

    async fn save(&self, candidate: &str) -> Result<()> {
        let mut saved = self.passes.saved()?;
        if let Some(fields) = saved.as_object_mut() {
            fields.insert("reviewer".into(), self.standing.saved()?);
            fields.insert("receipts".into(), self.receipts.saved()?);
        }
        crate::lesson_drafts::save_with_passes(candidate, saved.to_string()).await
    }

    fn settle_quotes(&self, candidate: &mut Value, _authoring: &Value, attempt: usize) -> bool {
        settle_quotes_in(candidate, &numbered(self.numbered), attempt)
    }

    fn quote_checks(&self, candidate: &Value, _authoring: &Value) -> Vec<QuoteCheck> {
        quote_checks_in(candidate, &numbered(self.numbered))
    }

    async fn find(&mut self, llm: &dyn LLMPort, pass: &Pass<'_>) -> Result<Found> {
        let review_prompt = review_prompt(pass, LESSON_PRIORITY, self.reference_versions());
        let structure = structural_findings(pass.schema, pass.candidate, &self.wording)?;
        // A rewrite of the whole lesson changes the sentences its checked
        // claims stand on, so once there are any, what only a rewrite can
        // answer waits for the user (claim classes design §3.10).
        if !self.protected.is_empty() {
            self.passes.hold(
                whole_repairs(&structure).map(ReviewFinding::id),
                PROTECTED_SENTENCE,
            );
        }
        let blocking = holds_up_review(&structure, Some(&self.passes));
        self.held_up = blocking;
        let issues = if !blocking {
            self.step(Step::CheckingKeys);
            let question_issues = crate::question_review::check(
                llm,
                pass.value,
                crate::question_review::Outcome::of_lesson(pass.authoring),
            )
            .await?;
            let round = self.standing.round(pass.value);
            self.step(if round.as_ref().is_some_and(|round| round.first) {
                Step::Reviewing
            } else {
                Step::Rechecking
            });
            let against_plan = contract::check(
                llm,
                pass.authoring,
                pass.value,
                self.protected,
                &mut self.receipts,
                &[],
            );
            let (judged, plan_findings) = match &round {
                Some(round) => {
                    let instructions = review_instructions(
                        &ReviewTexts::LESSON,
                        round,
                        true,
                        LESSON_REVIEW_EVIDENCE_SCOPE,
                    );
                    let review = crate::teaching_review::review(
                        llm,
                        &instructions,
                        review_prompt,
                        pass.value,
                        round.scope(),
                        None,
                    );
                    let (judged, plan) = futures::try_join!(review, against_plan)?;
                    (Some(judged), plan.findings)
                }
                // The reviewer has seen exactly this lesson: its findings stand.
                None => (None, against_plan.await?.findings),
            };
            let mut raised = Vec::new();
            if let (Some(round), Some(judged)) = (&round, judged) {
                raised.extend(self.standing.judged(pass.value, round, judged));
            }
            let quotations = quotation_findings(pass.quote_checks);
            let recorded: Vec<ReviewFinding> = quotations
                .iter()
                .chain(&question_issues)
                .chain(&plan_findings)
                .chain(&structure)
                .cloned()
                .collect();
            let checked = crate::review_evidence::check(
                llm,
                &raised,
                crate::review_evidence::Beside {
                    decides: &Beside::ALL.map(Beside::decides),
                    recorded: &recorded,
                },
                pass.authoring,
                pass.value,
                self.references,
                None,
            )
            .await?;
            let Checked {
                mut established,
                left,
            } = checked;
            let gate_established = established.len();
            let (plan_lefts, other_lefts): (Vec<_>, Vec<_>) = left
                .iter()
                .partition(|left| Beside::ALL.get(left.check) == Some(&Beside::Plan));
            established.extend(
                other_lefts
                    .iter()
                    .filter_map(|left| left.established.clone()),
            );
            let reported: Vec<&ReviewFinding> =
                plan_lefts.iter().map(|left| &left.finding).collect();
            let plan_findings = if reported.is_empty() {
                plan_findings
            } else {
                let ruled = contract::check(
                    llm,
                    pass.authoring,
                    pass.value,
                    self.protected,
                    &mut self.receipts,
                    &reported,
                )
                .await?;
                let before = plan_findings.len();
                let all = plan_findings_of(plan_findings, ruled.findings);
                tracing::info!(
                    reported = reported.len(),
                    held = all.len().saturating_sub(before),
                    outside = ruled.outside.len(),
                    "The check against the course plan ruled on what the review reported"
                );
                established.extend(ruled.outside.iter().filter_map(|index| {
                    plan_lefts
                        .get(*index)
                        .and_then(|left| left.established.clone())
                }));
                all
            };
            if !left.is_empty() {
                tracing::info!(
                    left = left.len(),
                    returned = established.len().saturating_sub(gate_established),
                    "Findings left to a check that did not rule on them stand on the gate's verdict"
                );
            }
            if round.is_some() {
                self.standing.admit(pass.value, established);
            }
            let mut issues = question_issues;
            issues.extend(self.standing.open().iter().cloned());
            issues.extend(plan_findings);
            // Stopped structure stays among the findings, so it is still
            // set aside when the review ends.
            issues.extend(structure);
            // Every non-blocking pass rechecks quotations, placed at the part that cites them.
            let mut findings = quotations;
            findings.extend(issues);
            findings
        } else {
            whole_repairs(&structure).cloned().collect()
        };
        let mut findings = issues;
        findings.retain(|finding| !finding.note.trim().is_empty());
        // A finding about a sentence a live claim stands on waits for
        // the user: the review does not rewrite what passed (§10.3).
        self.passes.hold(
            super::super::protected::on_protected(pass.value, &findings, self.protected),
            PROTECTED_SENTENCE,
        );
        Ok(Found {
            findings,
            edits: !blocking,
        })
    }

    fn decide(
        &mut self,
        attempt: usize,
        candidate: &Value,
        found: Vec<ReviewFinding>,
    ) -> Result<Decision> {
        match self.passes.next(candidate, found) {
            Next::Done(_) if self.held_up => {
                tracing::info!(
                    passes = attempt + 1,
                    "The structure that held up the content review has stopped; the content is reviewed now"
                );
                Ok(Decision::Review)
            }
            Next::Done(set_aside) => {
                if !set_aside.is_empty() {
                    tracing::info!(
                        findings = set_aside.len(),
                        passes = attempt + 1,
                        "Teaching findings the repairs did not settle wait for the user"
                    );
                    self.log_waiting(&set_aside);
                }
                Ok(Decision::Publish(set_aside))
            }
            Next::Repair(send) => Ok(Decision::Repair(send)),
        }
    }

    fn settle_repair(
        &mut self,
        previous: &Value,
        send: &[ReviewFinding],
        repaired: Repaired,
    ) -> Result<Settled> {
        let made = match repaired {
            Repaired::Patch { patch, plans } => self.settle_patch(previous, send, &patch, &plans),
            Repaired::Declined { plans } => Ok(self.settle_declined(send, &plans)),
            Repaired::Whole(text) => self.settle_whole(previous, send, &text),
        }?;
        Ok(match made {
            Made::Changed(text) => Settled::Changed(text),
            Made::Nothing => {
                tracing::warn!(
                    sent = send.len(),
                    "The teaching repair changed nothing that could stay"
                );
                Settled::Unchanged
            }
        })
    }

    fn unmade(
        &mut self,
        send: &[ReviewFinding],
        class: ErrorClass,
        reason: String,
    ) -> Result<Settled> {
        match class {
            ErrorClass::DoesNotFit => {
                tracing::warn!(%reason, "The teaching repair did not fit the window; its findings wait for the user");
                self.passes.hold(
                    send.iter().map(ReviewFinding::id),
                    "its repair did not fit the window",
                );
            }
            _ => {
                tracing::warn!(%reason, "The teaching repair could not be used");
                for finding in send {
                    self.passes
                        .charge(finding, Outcome::Unusable(reason.clone()));
                }
            }
        }
        Ok(Settled::Unchanged)
    }

    fn last_attempt(&self, finding: &ReviewFinding) -> Option<String> {
        self.passes.last_attempt(&finding.id())
    }
}

/// A candidate a repair made is used only if it still reads as a lesson.
fn reads_as_lesson_or_unusable(candidate: &Value) -> Result<()> {
    if crate::generation::reads_as_lesson(candidate) {
        Ok(())
    } else {
        Err(AppError::ModelAnswerUnusable(
            "the repaired material no longer reads as a lesson.".into(),
        ))
    }
}

/// What a repair made of the lesson.
enum Made {
    /// The candidate after the repair.
    Changed(String),
    /// The repair left the lesson as it was.
    Nothing,
}

impl LessonSubject<'_> {
    /// A repair's edits: each finding is charged with what became of the
    /// edits that named it.
    fn settle_patch(
        &mut self,
        previous: &Value,
        send: &[ReviewFinding],
        patch: &str,
        plans: &BTreeMap<String, String>,
    ) -> Result<Made> {
        let patch = text_edits::read_patch(patch)
            .map_err(|error| AppError::ModelAnswerUnusable(error.to_string()))?;
        // After a claim repair or a user edit, a finding's correction applies
        // whole or not at all: it may change only the sentences it names.
        let (applied, refused) = if self.protected.is_empty() {
            let applied = text_edits::apply_patch_for(
                previous,
                patch,
                &Value::Null,
                &self.wording,
                &repair::reach(send),
            )
            .map_err(|error| AppError::ModelAnswerUnusable(error.to_string()))?;
            (applied, BTreeMap::new())
        } else {
            apply_protected(previous, send, patch, &self.wording, self.protected)?
        };
        let value = applied.value.clone();
        reads_as_lesson_or_unusable(&value)?;
        let outcomes: Vec<Outcome> = send
            .iter()
            .map(|finding| match refused.get(finding.id().as_str()) {
                Some(reasons) => refusal(reasons),
                None => outcome_of(finding, &applied.named, plans, previous, &value),
            })
            .collect();
        for (finding, outcome) in send.iter().zip(&outcomes) {
            self.passes.charge(finding, outcome.clone());
        }
        for (finding, outcome) in send.iter().zip(&outcomes) {
            if *outcome == Outcome::Applied {
                let id = finding.id();
                let written = applied
                    .written
                    .iter()
                    .filter(|(named, path, sentence)| {
                        named == id.as_str()
                            && value
                                .pointer(path)
                                .and_then(Value::as_str)
                                .is_some_and(|text| text.contains(sentence.as_str()))
                    })
                    .map(|(_, _, sentence)| sentence.clone());
                let words: Vec<String> = written.chain(sentences_of(previous, finding)).collect();
                self.worked_on(finding, words);
            }
        }
        Ok(if value == *previous {
            Made::Nothing
        } else {
            Made::Changed(value.to_string())
        })
    }

    /// A repair that gave a plan for each finding and no change: each is
    /// charged as declined, with the plan it gave.
    fn settle_declined(
        &mut self,
        send: &[ReviewFinding],
        plans: &BTreeMap<String, String>,
    ) -> Made {
        for finding in send {
            let outcome = plans
                .get(finding.id().as_str())
                .map_or(Outcome::Unnamed, |plan| Outcome::Declined(plan.clone()));
            self.passes.charge(finding, outcome);
        }
        Made::Nothing
    }

    /// A repair that rewrote the whole lesson names no finding: a finding is
    /// charged as applied when its part changed.
    fn settle_whole(
        &mut self,
        previous: &Value,
        send: &[ReviewFinding],
        text: &str,
    ) -> Result<Made> {
        let repaired = crate::generation::parse_json::<Value>(text)
            .map_err(|error| AppError::ModelAnswerUnusable(error.to_string()))?;
        let (value, kept) = if self.protected.is_empty() {
            (repaired, text.to_owned())
        } else {
            let value = match super::super::protected::confine_review(
                previous,
                &repaired,
                &touched_by_sites(previous, send),
                self.protected,
            )
            .map_err(|error| AppError::ModelAnswerUnusable(error.to_string()))?
            {
                Ok(value) => value,
                Err(reasons) => {
                    for finding in send {
                        self.passes.charge(finding, refusal(&reasons));
                    }
                    return Ok(Made::Nothing);
                }
            };
            let kept = value.to_string();
            (value, kept)
        };
        reads_as_lesson_or_unusable(&value)?;
        for finding in send {
            let site = finding.site.unwrap_or(FindingSite::Lesson);
            let outcome = if part_at(previous, site) == part_at(&value, site) {
                Outcome::Unnamed
            } else {
                Outcome::Applied
            };
            self.passes.charge(finding, outcome);
        }
        self.record_whole_work(previous, send, &value);
        Ok(if value == *previous {
            Made::Nothing
        } else {
            Made::Changed(kept)
        })
    }

    /// Tells the budgets and the reviewer's findings what an applied repair
    /// worked on for `id`.
    fn worked_on(&mut self, finding: &ReviewFinding, sentences: Vec<String>) {
        self.standing
            .worked_on(&finding.id(), sentences.iter().cloned());
        self.passes.worked_on(finding, sentences);
    }

    /// What a rewrite puts its findings' words through: the sentences it wrote
    /// count for the findings it was sent for, and each finding whose part it
    /// changed counts the words it was applied for.
    fn record_whole_work(&mut self, previous: &Value, send: &[ReviewFinding], repaired: &Value) {
        for (site, sentences) in new_sentences(previous, repaired) {
            let owners: Vec<&ReviewFinding> = send
                .iter()
                .filter(|finding| {
                    matches!(finding.site, None | Some(FindingSite::Lesson))
                        || finding.site == Some(site)
                })
                .collect();
            let owners = if owners.is_empty() {
                send.iter().collect()
            } else {
                owners
            };
            for owner in owners {
                self.worked_on(owner, sentences.clone());
            }
        }
        for finding in send {
            let site = finding.site.unwrap_or(FindingSite::Lesson);
            if part_at(previous, site) != part_at(repaired, site) {
                self.worked_on(finding, sentences_of(previous, finding));
            }
        }
    }
}

/// What became of the edits that named `finding`, in the lesson `value` they
/// made from `previous`; `plans` are the repair's plan for each finding it
/// answered.
fn outcome_of(
    finding: &ReviewFinding,
    named: &BTreeMap<String, Named>,
    plans: &BTreeMap<String, String>,
    previous: &Value,
    value: &Value,
) -> Outcome {
    let id = finding.id();
    let Some(named) = named.get(id.as_str()) else {
        return plans
            .get(id.as_str())
            .map_or(Outcome::Unnamed, |plan| Outcome::Declined(plan.clone()));
    };
    if named.applied.is_empty() {
        if named.deferred {
            Outcome::Deferred
        } else {
            Outcome::NotApplied(named.problems.clone())
        }
    } else if named
        .applied
        .iter()
        .all(|pointer| previous.pointer(pointer) == value.pointer(pointer))
    {
        Outcome::Reverted
    } else {
        Outcome::Applied
    }
}

/// A finding whose correction was refused whole, charged with why.
fn refusal(reasons: &[String]) -> Outcome {
    Outcome::Refused(reasons.iter().map(|reason| format!("{reason}.")).collect())
}

/// `previous` as a repair that changed nothing leaves it.
fn unchanged(previous: &Value) -> AppliedFor {
    AppliedFor {
        value: previous.clone(),
        named: BTreeMap::new(),
        written: Vec::new(),
    }
}

/// Applies `patch` while sentences are protected: each finding's correction is
/// judged alone and refused whole if it leaves what it may change, the rest
/// apply together, and that result is confined once more, since what the
/// corrections do together is not seen alone. Gives the application and the
/// reasons for each refused finding.
fn apply_protected(
    previous: &Value,
    send: &[ReviewFinding],
    patch: Patch,
    wording: &Wording,
    protected: &[Protected],
) -> Result<(AppliedFor, BTreeMap<String, Vec<String>>)> {
    let apply = |patch: Patch| {
        text_edits::apply_patch_for(previous, patch, &Value::Null, wording, &repair::reach(send))
            .map_err(|error| AppError::ModelAnswerUnusable(error.to_string()))
    };
    let mut judged = judge_each(previous, send, &patch, wording, protected)?;
    let applied = if judged.refused.is_empty() {
        apply(patch)?
    } else if judged.accepted.is_empty() {
        unchanged(previous)
    } else {
        apply(patch.only(&judged.accepted))?
    };
    let together = super::super::protected::confine_review(
        previous,
        &applied.value,
        &touched_by_edits(send, &applied.named),
        protected,
    )
    .map_err(|error| AppError::ModelAnswerUnusable(error.to_string()))?;
    let Err(reasons) = together else {
        return Ok((applied, judged.refused));
    };
    for finding in send {
        let id = finding.id();
        let wrote = applied
            .named
            .get(id.as_str())
            .is_some_and(|named| !named.applied.is_empty());
        if wrote {
            judged
                .refused
                .entry(id.as_str().to_owned())
                .or_insert_with(|| reasons.clone());
        }
    }
    Ok((unchanged(previous), judged.refused))
}

/// What each named finding's correction comes to when it is judged on its own.
struct Judged {
    /// The findings whose corrections stay inside what they may change.
    accepted: Vec<String>,
    /// The findings whose corrections do not, with why.
    refused: BTreeMap<String, Vec<String>>,
}

/// Applies each named finding's changes alone to `previous` and confines the
/// result to what that finding may change; a finding whose result is not
/// wholly inside it is refused with its reasons.
fn judge_each(
    previous: &Value,
    send: &[ReviewFinding],
    patch: &Patch,
    wording: &Wording,
    protected: &[Protected],
) -> Result<Judged> {
    let reach = repair::reach(send);
    let mut judged = Judged {
        accepted: Vec::new(),
        refused: BTreeMap::new(),
    };
    for id in patch.findings() {
        let Some(finding) = send.iter().find(|finding| finding.id().as_str() == id) else {
            continue;
        };
        let alone = patch.only(std::slice::from_ref(&id));
        // A correction none of whose changes can be applied is for the
        // final application to report.
        let Ok(applied) =
            text_edits::apply_patch_for(previous, alone, &Value::Null, wording, &reach)
        else {
            judged.accepted.push(id);
            continue;
        };
        let confined = super::super::protected::confine_review(
            previous,
            &applied.value,
            &touched_by_edits(std::slice::from_ref(finding), &applied.named),
            protected,
        )
        .map_err(|error| AppError::ModelAnswerUnusable(error.to_string()))?;
        match confined {
            Ok(_) => judged.accepted.push(id),
            Err(reasons) => {
                judged.refused.insert(id, reasons);
            }
        }
    }
    Ok(judged)
}

/// For each part (`/blocks/N`, `/questions/N`), the findings whose applied
/// changes touched it.
fn touched_by_edits<'a>(
    send: &'a [ReviewFinding],
    named: &BTreeMap<String, Named>,
) -> BTreeMap<String, Vec<&'a ReviewFinding>> {
    let mut touched = BTreeMap::<String, Vec<&ReviewFinding>>::new();
    for finding in send {
        let Some(named) = named.get(finding.id().as_str()) else {
            continue;
        };
        for pointer in &named.applied {
            if let Some(part) = FindingSite::of_pointer(pointer).pointer() {
                let here = touched.entry(part).or_default();
                if !here.iter().any(|known| known.id() == finding.id()) {
                    here.push(finding);
                }
            }
        }
    }
    touched
}

/// For each part, the findings a rewrite of the whole lesson may touch it
/// for: those sited there and those about the whole lesson.
fn touched_by_sites<'a>(
    previous: &Value,
    send: &'a [ReviewFinding],
) -> BTreeMap<String, Vec<&'a ReviewFinding>> {
    ["blocks", "questions"]
        .into_iter()
        .flat_map(|list| {
            let count = previous
                .get(list)
                .and_then(Value::as_array)
                .map_or(0, Vec::len);
            (0..count).map(move |index| format!("/{list}/{index}"))
        })
        .map(|part| {
            let here: Vec<&ReviewFinding> = send
                .iter()
                .filter(|finding| {
                    finding
                        .site
                        .and_then(FindingSite::pointer)
                        .is_none_or(|own| own == part)
                })
                .collect();
            (part, here)
        })
        .collect()
}

#[cfg(test)]
#[path = "lesson_tests.rs"]
mod tests;
