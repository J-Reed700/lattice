//! The lesson as a subject of the review.
use super::super::contract;
use super::super::passes::{Next, Outcome, Passes};
use super::super::quotes::lesson_quote_checks;
use super::super::repair::{self, Repaired};
use super::super::standing::Standing;
use super::super::{
    holds_up_review, quotation_findings, structural_findings, whole_repairs,
    LESSON_REVIEW_EVIDENCE_SCOPE,
};
use super::{
    review_instructions, review_prompt, settle_lesson_quotes, Decision, Found, Pass, Settled, Step,
    Subject, LESSON_PRIORITY, REVIEW_SYSTEM,
};
use crate::lessons::content_verification::Protected;
use crate::lessons::copied_wording::Wording;
use crate::lessons::text_edits::{self, Named};
use crate::outline_progress::OutlineProgress;
use crate::reference_collection::ReferenceCollection;
use crate::review_findings::{
    new_sentences, part_at, sentences_of, FindingId, FindingSite, Held, Origin, ReviewFinding,
};
use crate::teaching_review::LengthRule;
use lattice_core::{
    application::ports::LLMPort,
    shared::error::{AppError, Result},
};
use serde_json::{json, Value};
use std::collections::BTreeMap;

/// A lesson: it publishes with what the repairs could not settle waiting for
/// the user, and holds what its live claims stand on.
pub(in crate::lessons::teaching) struct LessonSubject<'a> {
    passes: Passes,
    receipts: contract::Receipts,
    protected: &'a [Protected],
    references: &'a ReferenceCollection<'a>,
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
        protected: &'a [Protected],
        held: Held,
    ) -> Self {
        let standing = Standing::new(&held);
        Self {
            passes: Passes::new(held),
            receipts: contract::Receipts::default(),
            protected,
            references,
            wording: Wording::of(references),
            standing,
            held_up: false,
        }
    }

    /// Each reference's id and the digest of its excerpt.
    fn reference_versions(&self) -> Vec<Value> {
        self.references.sources.iter().map(|source| {
            json!({"id":source.id,"contentHash":crate::content_verification::digest(&source.excerpt)})
        }).collect()
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

    async fn resume(&mut self, candidate: &str) -> Result<()> {
        // A review cut short resumes with the repairs it had made to this draft.
        if let Some(saved) = crate::lesson_drafts::teaching_passes(candidate).await? {
            self.passes.resume(&saved);
            self.standing.resume(&saved);
        }
        Ok(())
    }

    fn given(&mut self, llm: &dyn LLMPort, authoring: &Value) {
        let inputs = json!({"authoringContext":authoring,"correctnessPriority":LESSON_PRIORITY,"referenceVersions":self.reference_versions()});
        self.standing.under(&crate::teaching_review::basis(
            llm,
            REVIEW_SYSTEM,
            LESSON_REVIEW_EVIDENCE_SCOPE,
            &inputs,
        ));
    }

    async fn save(&self, candidate: &str) -> Result<()> {
        let mut saved = self.passes.saved()?;
        if let Some(fields) = saved.as_object_mut() {
            fields.insert("reviewer".into(), self.standing.saved()?);
        }
        crate::lesson_drafts::save_with_passes(candidate, saved.to_string()).await
    }

    fn settle_quotes(&self, candidate: &mut Value, authoring: &Value, attempt: usize) -> bool {
        settle_lesson_quotes(candidate, authoring, attempt)
    }

    fn quote_checks(
        &self,
        candidate: &Value,
        authoring: &Value,
        _attempt: usize,
    ) -> Result<Vec<Value>> {
        Ok(lesson_quote_checks(candidate, authoring))
    }

    async fn find(&mut self, llm: &dyn LLMPort, pass: &Pass<'_>) -> Result<Found> {
        let review_prompt = review_prompt(pass, LESSON_PRIORITY, self.reference_versions());
        let structure = structural_findings(pass.schema, pass.candidate, &self.wording)?;
        // A rewrite of the whole lesson changes the sentences its checked
        // claims stand on, so once there are any, what its shape asks waits
        // for the user (claim classes design §3.10).
        if !self.protected.is_empty() {
            self.passes
                .hold(whole_repairs(&structure).map(ReviewFinding::id));
        }
        let blocking = holds_up_review(&structure, Some(&self.passes));
        self.held_up = blocking;
        let issues = if !blocking {
            self.step(Step::CheckingKeys);
            let answer_issues = crate::key_review::check(llm, pass.value).await?;
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
            );
            let (judged, plan_findings) = match &round {
                Some(round) => {
                    let instructions = review_instructions(
                        REVIEW_SYSTEM,
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
                        LengthRule::DropBlank,
                    );
                    let (judged, plan) = futures::try_join!(review, against_plan)?;
                    (Some(judged), plan)
                }
                // The reviewer has seen exactly this lesson: its findings stand.
                None => (None, against_plan.await?),
            };
            let mut raised = answer_issues;
            if let (Some(round), Some(judged)) = (&round, judged) {
                raised.extend(self.standing.judged(pass.value, round, judged));
            }
            let established = crate::review_evidence::check(
                llm,
                &raised,
                pass.authoring,
                pass.value,
                self.references,
                None,
            )
            .await?;
            let (new, mut issues): (Vec<_>, Vec<_>) = established
                .into_iter()
                .partition(|finding| matches!(finding.origin, Origin::Reviewer { .. }));
            if round.is_some() {
                self.standing.admit(pass.value, new);
            }
            issues.extend(self.standing.open().iter().cloned());
            issues.extend(plan_findings);
            // Stopped structure stays among the findings, so it is still
            // set aside when the review ends.
            issues.extend(structure);
            issues
        } else {
            structure
        };
        // Every pass rechecks quotations; a mismatch is a finding like
        // any other, placed at the part that cites it.
        let mut findings = quotation_findings(pass.quote_checks);
        findings.extend(issues);
        findings.retain(|finding| !finding.note.trim().is_empty());
        // A finding about a sentence a live claim stands on waits for
        // the user: the review does not rewrite what passed (§10.3).
        self.passes.hold(super::super::protected::on_protected(
            pass.value,
            &findings,
            self.protected,
        ));
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
        edits: bool,
        result: Result<Repaired>,
    ) -> Result<Settled> {
        let made = match result {
            Ok(Repaired::Patch(raw)) => self.settle_patch(previous, send, &raw),
            Ok(Repaired::Whole(text)) => self.settle_whole(previous, send, &text),
            Err(error) => Err(error),
        };
        match made {
            Ok(Made::Changed(text)) => Ok(Settled::Changed(text)),
            Ok(Made::Nothing) => {
                tracing::warn!(
                    sent = send.len(),
                    "The teaching repair changed nothing that could stay"
                );
                Ok(Settled::Unchanged)
            }
            // An answer that cannot be used, or a request that does not fit
            // the window, is a repair that could not be made, not a lesson
            // that cannot be published: it fails no job. Only edits are
            // halved: a rewrite is the whole lesson whatever it is sent.
            Err(
                error @ (AppError::ModelAnswerUnusable(_)
                | AppError::PromptExceedsWindow { .. }
                | AppError::InvalidConfig(_)),
            ) => {
                if edits && send.len() > 1 {
                    let (first, second) = send.split_at(send.len() / 2);
                    tracing::info!(%error, findings = send.len(), "The teaching repair could not be made for these findings together; each half is repaired on its own");
                    return Ok(Settled::Split(first.to_vec(), second.to_vec()));
                }
                match error {
                    AppError::ModelAnswerUnusable(reason) => {
                        tracing::warn!(%reason, "The teaching repair could not be used");
                        for finding in send {
                            self.passes
                                .charge(finding, Outcome::Unusable(reason.clone()));
                        }
                    }
                    error => {
                        tracing::warn!(%error, "The teaching repair did not fit the window; its findings wait for the user");
                        self.passes.hold(send.iter().map(ReviewFinding::id));
                    }
                }
                Ok(Settled::Unchanged)
            }
            Err(error) => Err(error),
        }
    }

    fn last_attempt(&self, finding: &ReviewFinding) -> Option<String> {
        self.passes.last_attempt(&finding.id())
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
        raw: &str,
    ) -> Result<Made> {
        let applied = text_edits::apply_for(
            previous,
            raw,
            &Value::Null,
            &self.wording,
            &repair::reach(send),
        )
        .map_err(|error| AppError::ModelAnswerUnusable(error.to_string()))?;
        // After a claim repair or a user edit, the repair changes only the
        // sentences its findings name.
        let value = if self.protected.is_empty() {
            applied.value.clone()
        } else {
            super::super::protected::confine_review(
                previous,
                &applied.value,
                &touched_by_edits(send, &applied.named),
                self.protected,
            )
            .map_err(|error| AppError::ModelAnswerUnusable(error.to_string()))?
        };
        let outcomes: Vec<Outcome> = send
            .iter()
            .map(|finding| outcome_of(finding, &applied.named, previous, &value))
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
                self.worked_on(&id, words);
            }
        }
        Ok(if value == *previous {
            Made::Nothing
        } else {
            Made::Changed(value.to_string())
        })
    }

    /// A repair that rewrote the whole lesson names no finding: a finding is
    /// charged as applied when its part changed.
    fn settle_whole(
        &mut self,
        previous: &Value,
        send: &[ReviewFinding],
        text: &str,
    ) -> Result<Made> {
        let repaired = match crate::generation::parse_json::<Value>(text) {
            Ok(repaired) => repaired,
            Err(error) if !self.protected.is_empty() => {
                return Err(AppError::ModelAnswerUnusable(error.to_string()))
            }
            Err(_) => {
                for finding in send {
                    self.passes.charge(finding, Outcome::Applied);
                }
                return Ok(Made::Changed(text.to_owned()));
            }
        };
        let (value, kept) = if self.protected.is_empty() {
            (repaired, text.to_owned())
        } else {
            let value = super::super::protected::confine_review(
                previous,
                &repaired,
                &touched_by_sites(previous, send),
                self.protected,
            )
            .map_err(|error| AppError::ModelAnswerUnusable(error.to_string()))?;
            let kept = value.to_string();
            (value, kept)
        };
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
    fn worked_on(&mut self, id: &FindingId, sentences: Vec<String>) {
        self.standing.worked_on(id, sentences.iter().cloned());
        self.passes.worked_on(id, sentences);
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
                self.worked_on(&owner.id(), sentences.clone());
            }
        }
        for finding in send {
            let site = finding.site.unwrap_or(FindingSite::Lesson);
            if part_at(previous, site) != part_at(repaired, site) {
                self.worked_on(&finding.id(), sentences_of(previous, finding));
            }
        }
    }
}

/// What became of the edits that named `finding`, in the lesson `value` they
/// made from `previous`.
fn outcome_of(
    finding: &ReviewFinding,
    named: &BTreeMap<String, Named>,
    previous: &Value,
    value: &Value,
) -> Outcome {
    let Some(named) = named.get(finding.id().as_str()) else {
        return Outcome::Unnamed;
    };
    if named.applied.is_empty() {
        Outcome::NotApplied(named.problems.clone())
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
