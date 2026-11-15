//! Material that fails closed after its repairs.
use super::super::passes::REPAIRS_PER_FINDING;
use super::super::quotes::{lesson_quote_checks, QuoteCheck};
use super::super::repair;
use super::super::standing::Standing;
use super::super::{
    holds_up_review, quotation_findings, structural_findings, MATERIAL_REVIEW_EVIDENCE_SCOPE,
};
use super::{
    review_instructions, review_prompt, settle_lesson_quotes, Decision, Found, Pass, ReviewTexts,
    Settled, Step, Subject, MATERIAL_PRIORITY, REVIEW_SYSTEM,
};
use crate::lessons::copied_wording::Wording;
use crate::model_call::ErrorClass;
use crate::review_findings::{Held, ReviewFinding};
use lattice_core::{
    application::ports::LLMPort,
    shared::error::{AppError, Result},
};
use serde_json::{json, Value};
use std::borrow::Cow;

fn quiz_step(step: Step) {
    crate::lesson_progress::stage(match step {
        Step::CheckingKeys => "Checking the quiz questions' answer keys",
        Step::Reviewing => "Reviewing the quiz questions",
        Step::Rechecking => "Rechecking the corrected quiz questions",
        Step::Repairing => "Correcting the quiz questions from review findings",
    });
}

/// Notes what the reviewer of material that fails closed is given.
fn closed_given(standing: &mut Standing, llm: &dyn LLMPort, authoring: &Value) {
    let inputs = json!({"authoringContext":authoring,"correctnessPriority":MATERIAL_PRIORITY,"referenceVersions":Vec::<Value>::new()});
    standing.under(&crate::teaching_review::basis(
        llm,
        REVIEW_SYSTEM,
        MATERIAL_REVIEW_EVIDENCE_SCOPE,
        &inputs,
    ));
}

/// The checks of material that fails closed: the answer keys, then the reviewer.
async fn closed_find(
    standing: &mut Standing,
    llm: &dyn LLMPort,
    pass: &Pass<'_>,
    wording: &Wording,
) -> Result<Found> {
    let structure = structural_findings(pass.schema, pass.candidate, wording)?;
    let blocking = holds_up_review(&structure, None);
    let found = if !blocking {
        quiz_step(Step::CheckingKeys);
        let mut findings = crate::question_review::check(
            llm,
            pass.value,
            crate::question_review::Outcome::of_lesson(pass.authoring),
        )
        .await?;
        if let Some(round) = standing.round(pass.value) {
            quiz_step(if round.first {
                Step::Reviewing
            } else {
                Step::Rechecking
            });
            let texts = ReviewTexts {
                first: Cow::Borrowed(REVIEW_SYSTEM),
                ..ReviewTexts::GENERAL
            };
            let instructions = review_instructions(&texts, &round, MATERIAL_REVIEW_EVIDENCE_SCOPE);
            let judged = crate::teaching_review::review(
                llm,
                &instructions,
                review_prompt(pass, MATERIAL_PRIORITY, Vec::new()),
                pass.value,
                round.scope(),
                crate::teaching_review::Remit::Open,
            )
            .await?;
            let new = standing.judged(pass.value, &round, judged);
            standing.admit(pass.value, new);
        }
        findings.extend(standing.open().iter().cloned());
        findings.extend(structure);
        findings
    } else {
        structure
    };
    let mut findings = quotation_findings(pass.quote_checks);
    findings.extend(found);
    Ok(Found {
        findings,
        edits: !blocking,
    })
}

/// Material that fails closed repairs as a lesson's finding is repaired; a
/// defect that survives its repairs is an error.
fn closed_decide(attempt: usize, found: Vec<ReviewFinding>) -> Result<Decision> {
    if found.is_empty() {
        return Ok(Decision::Publish(Vec::new()));
    }
    if attempt >= usize::from(REPAIRS_PER_FINDING) {
        let issues: Vec<&str> = found.iter().map(|finding| finding.note.as_str()).collect();
        return Err(AppError::InvalidInput(format!(
            "The teaching review still found an issue: {} No material was published. Try again.",
            issues.join(" ")
        )));
    }
    Ok(Decision::Repair(found))
}

/// A repair that could not be made fails the material, as the kind of
/// failure it was.
fn closed_unmade(class: ErrorClass, reason: String) -> Result<Settled> {
    Err(match class {
        ErrorClass::DoesNotFit => AppError::WindowTooSmall(reason),
        _ => AppError::ModelAnswerUnusable(reason),
    })
}

/// Other material reviewed inside a job (a quiz batch): it fails closed
/// after its repairs.
pub(in crate::lessons::teaching) struct MaterialSubject {
    wording: Wording,
    standing: Standing,
}

impl MaterialSubject {
    pub(in crate::lessons::teaching) fn new() -> Self {
        Self {
            wording: Wording::default(),
            standing: Standing::new(&Held::default()),
        }
    }
}

#[async_trait::async_trait]
impl Subject for MaterialSubject {
    fn wording(&self) -> &Wording {
        &self.wording
    }

    fn step(&self, step: Step) {
        quiz_step(step);
    }

    async fn save(&self, candidate: &str) -> Result<()> {
        crate::lesson_drafts::save(candidate).await
    }

    fn settle_quotes(&self, candidate: &mut Value, authoring: &Value, attempt: usize) -> bool {
        settle_lesson_quotes(candidate, authoring, attempt)
    }

    fn quote_checks(&self, candidate: &Value, authoring: &Value) -> Vec<QuoteCheck> {
        lesson_quote_checks(candidate, authoring)
    }

    fn given(&mut self, llm: &dyn LLMPort, authoring: &Value) {
        closed_given(&mut self.standing, llm, authoring);
    }

    async fn find(&mut self, llm: &dyn LLMPort, pass: &Pass<'_>) -> Result<Found> {
        closed_find(&mut self.standing, llm, pass, &self.wording).await
    }

    fn decide(
        &mut self,
        attempt: usize,
        _candidate: &Value,
        found: Vec<ReviewFinding>,
    ) -> Result<Decision> {
        closed_decide(attempt, found)
    }

    fn settle_mended(
        &mut self,
        _previous: &Value,
        _send: &[ReviewFinding],
        mended: repair::Mended,
    ) -> Result<Settled> {
        if mended.changed {
            return Ok(Settled::Changed(mended.candidate.to_string()));
        }
        Err(AppError::ModelAnswerUnusable(mended.unmade()))
    }

    fn settle_rewrite(
        &mut self,
        _previous: &Value,
        _send: &[ReviewFinding],
        text: &str,
    ) -> Result<Settled> {
        Ok(Settled::Changed(text.to_owned()))
    }

    fn unmade(
        &mut self,
        _send: &[ReviewFinding],
        class: ErrorClass,
        reason: String,
    ) -> Result<Settled> {
        closed_unmade(class, reason)
    }
}
