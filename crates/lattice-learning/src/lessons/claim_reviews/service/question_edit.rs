//! The user's rewrite of one question of a published lesson: the statements on
//! text they changed end, the ones on text they left stay as they are, and
//! each sentence they wrote becomes a statement they confirmed. One
//! transaction writes the question, its key, the ledger, the stored report
//! and the teaching notes' marker, so a refusal writes nothing.

use std::collections::HashSet;

use lattice_core::domain::learning::claim_review::{
    ClaimLedgerEntry, ClaimState, Ledger, UnitKind,
};
use lattice_core::domain::provenance::ClaimKind;
use lattice_core::shared::error::{AppError, Result};
use lattice_core::shared::persistence::Transactions;

use super::section_edit::{count, endings, record_edit, Ending};
use super::{provenance, ClaimReviewService};
use crate::assessment_engine::{
    self as engine, normalize_answer, LearningAssessmentKey, LearningItemFormat,
};
use crate::assessment_generation::StoredItem;
use crate::assessment_repository::{content_hash, LearningAssessmentRepository};
use crate::content_verification::Rewritten;
use crate::lessons::claim_reviews::published_edit::update_report;
use crate::lessons::claim_reviews::published_rewrite::{plan, FieldPlan, Written};
use crate::lessons::claim_reviews::repository::{
    blocks, bump_revision, check_revision, questions, teaching_findings,
};
use crate::persistence::{db, now};
use crate::quiz_blueprints::parse_format;

const QUESTION_CHANGED: &str =
    "This question changed since you opened it. Copy your changes, close the editor and open it again.";
const NOT_IN_LESSON: &str = "This question is not in this lesson.";
const NO_PROMPT: &str = "A question needs a prompt.";
const BLANK_CHOICE: &str = "Each choice needs some text.";
const SAME_CHOICES: &str = "Each choice needs different text.";
const NO_RIGHT: &str = "Choose the right answer.";
const SAME_QUESTION: &str = "Another question in this course has the same prompt and choices.";
const BAD_REASONS: &str = "The reasons must match the choices, one for each.";

fn too_few() -> String {
    let least = *engine::SINGLE_CHOICE_OPTIONS.start();
    match least {
        2 => "A question needs at least two choices.".into(),
        _ => format!("A question needs at least {least} choices."),
    }
}

fn too_many() -> String {
    let most = *engine::SINGLE_CHOICE_OPTIONS.end();
    format!("A question can have at most {most} choices.")
}

/// A question as the editor shows it: its prompt, its choices in order, the
/// right choice, its explanation and the reason each wrong choice fails.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuestionText {
    pub kind: LearningItemFormat,
    pub prompt: String,
    /// Empty for a kind without choices.
    pub choices: Vec<String>,
    /// The index of the right choice; none for a kind without one.
    pub right: Option<usize>,
    pub explanation: String,
    /// One reason per choice, the right choice's empty; empty when the
    /// question has none.
    pub rationales: Vec<String>,
}

/// A question's new text, with the one the user opened.
pub struct QuestionEdit {
    pub program_id: String,
    pub lesson_id: String,
    pub question_index: usize,
    pub before: QuestionText,
    pub after: QuestionText,
    pub dry_run: bool,
}

/// What a rewrite does, or did, to the statements on the question.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuestionEditOutcome {
    pub changed: bool,
    pub statements_kept: u32,
    pub statements_changed: u32,
    pub statements_removed: u32,
    pub sentences_written: u32,
    /// Changed or removed statements that were verified against a source.
    pub checked_statements_affected: u32,
    pub revision: Option<i64>,
}

struct Stored {
    id: String,
    text: QuestionText,
}

/// The reasons as stored: none at all when every one is blank, so a question
/// written without them reads the same as one whose empty fields were saved.
fn compact(reasons: Vec<String>) -> Vec<String> {
    if reasons.iter().all(|reason| reason.trim().is_empty()) {
        Vec::new()
    } else {
        reasons
    }
}

/// Whether the kind's choices are the question's own and its key is one of them.
fn has_choices(kind: LearningItemFormat) -> bool {
    matches!(
        kind,
        LearningItemFormat::SingleChoice | LearningItemFormat::TrueFalse
    )
}

fn stored_from(row: Option<questions::QuestionRow>) -> Result<Option<Stored>> {
    let Some(row) = row else {
        return Ok(None);
    };
    let kind = parse_format(&row.format)?;
    let (choices, right) = if has_choices(kind) {
        let choices: Vec<String> = serde_json::from_str(&row.options_json)?;
        let right = match serde_json::from_str::<LearningAssessmentKey>(&row.key_json)? {
            LearningAssessmentKey::Choice(index) => Some(index),
            _ => None,
        };
        (choices, right)
    } else {
        (Vec::new(), None)
    };
    Ok(Some(Stored {
        id: row.id,
        text: QuestionText {
            kind,
            prompt: row.prompt,
            choices,
            right,
            explanation: row.explanation,
            rationales: compact(serde_json::from_str(&row.rationales_json)?),
        },
    }))
}

fn refuse(text: impl Into<String>) -> Result<()> {
    Err(AppError::InvalidInput(text.into()))
}

fn validate(kind: LearningItemFormat, after: &QuestionText) -> Result<()> {
    if after.prompt.trim().is_empty() {
        return refuse(NO_PROMPT);
    }
    match kind {
        LearningItemFormat::SingleChoice => {
            let choices = after.choices.len();
            if choices < *engine::SINGLE_CHOICE_OPTIONS.start() {
                return refuse(too_few());
            }
            if choices > *engine::SINGLE_CHOICE_OPTIONS.end() {
                return refuse(too_many());
            }
            if after.choices.iter().any(|choice| choice.trim().is_empty()) {
                return refuse(BLANK_CHOICE);
            }
            let distinct: HashSet<String> = after
                .choices
                .iter()
                .map(|choice| normalize_answer(choice))
                .collect();
            if distinct.len() != choices {
                return refuse(SAME_CHOICES);
            }
            if !after.rationales.is_empty() && after.rationales.len() != choices {
                return refuse(BAD_REASONS);
            }
            if !after.right.is_some_and(|right| right < choices) {
                return refuse(NO_RIGHT);
            }
        }
        LearningItemFormat::TrueFalse if !after.right.is_some_and(|right| right < 2) => {
            return refuse(NO_RIGHT);
        }
        _ => {}
    }
    Ok(())
}

/// What the edit writes: the choices and their reasons change only for a
/// single-choice question, the right answer only where the kind has one. The
/// right choice has no reason; a kind whose reasons the editor does not show
/// keeps them unless its right answer moved.
fn effective(before: &QuestionText, after: QuestionText) -> QuestionText {
    let right = if has_choices(before.kind) {
        after.right
    } else {
        before.right
    };
    let rationales = if before.kind == LearningItemFormat::SingleChoice {
        let mut reasons = after.rationales;
        if let Some(reason) = right.and_then(|right| reasons.get_mut(right)) {
            reason.clear();
        }
        compact(reasons)
    } else if right == before.right {
        before.rationales.clone()
    } else {
        Vec::new()
    };
    QuestionText {
        kind: before.kind,
        prompt: after.prompt,
        choices: if before.kind == LearningItemFormat::SingleChoice {
            after.choices
        } else {
            before.choices.clone()
        },
        right,
        explanation: after.explanation,
        rationales,
    }
}

/// The stored bank item with the edit applied, as the quiz engine will read it.
fn edited_candidate(
    stored: engine::LearningAssessmentCandidate,
    new: &QuestionText,
) -> engine::LearningAssessmentCandidate {
    let mut candidate = stored;
    candidate.prompt.clone_from(&new.prompt);
    candidate.explanation.clone_from(&new.explanation);
    if new.kind == LearningItemFormat::SingleChoice {
        candidate.options.clone_from(&new.choices);
    }
    if let Some(right) = new.right.filter(|_| has_choices(new.kind)) {
        candidate.key = LearningAssessmentKey::Choice(right);
    }
    candidate.distractor_rationales.clone_from(&new.rationales);
    candidate
}

/// The unit's text as the report stores it: its choices, explanation and
/// reasons joined by blank lines, in the order of `unit_texts`.
fn unit_text(question: &QuestionText) -> String {
    question
        .choices
        .iter()
        .map(String::as_str)
        .chain(std::iter::once(question.explanation.as_str()))
        .chain(question.rationales.iter().map(String::as_str))
        .collect::<Vec<_>>()
        .join("\n\n")
}

/// What a rewrite of one field does to the claims on it.
struct FieldChange {
    field: String,
    kind: UnitKind,
    old: String,
    plan: FieldPlan,
}

/// `replaces_only`: a new sentence is written only where it replaces one a
/// statement stood on, as for a section's title.
fn field_change(
    claims: &[ClaimLedgerEntry],
    unit: usize,
    field: String,
    kind: UnitKind,
    old: &str,
    new: &str,
    replaces_only: bool,
) -> FieldChange {
    let mut changed = plan(claims, unit, &field, old, new);
    if replaces_only && changed.edited.is_empty() {
        changed.written.clear();
    }
    FieldChange {
        field,
        kind,
        old: old.to_owned(),
        plan: changed,
    }
}

impl ClaimReviewService {
    /// A published lesson's question, as the editor opens it.
    pub async fn question_text(
        &self,
        program_id: &str,
        lesson_id: &str,
        question_index: usize,
    ) -> Result<QuestionText> {
        self.ensure_in_course(program_id, lesson_id).await?;
        let mut read = self.pool().begin_read().await.map_err(db)?;
        let row = questions::question_at(&mut read, program_id, lesson_id, question_index).await?;
        read.commit().await.map_err(db)?;
        stored_from(row)?
            .map(|stored| stored.text)
            .ok_or_else(|| AppError::NotFound(NOT_IN_LESSON.into()))
    }

    /// Writes the question's new text, ends the statements that stood on what
    /// changed, and keeps the rest. A dry run writes nothing. A new prompt or
    /// choice is written as a statement only when it replaces one. A question
    /// the bank had dropped is agreed again by a valid save. A save stores the
    /// new content's token and is refused when another question has the same content.
    pub async fn edit_question(&self, edit: QuestionEdit) -> Result<QuestionEditOutcome> {
        let QuestionEdit {
            program_id,
            lesson_id,
            question_index,
            before,
            after,
            dry_run,
        } = edit;
        let ledger = self.editable_ledger(&program_id, &lesson_id).await?;
        validate(before.kind, &after)?;
        let mut read = self.pool().begin_read().await.map_err(db)?;
        let row =
            questions::question_at(&mut read, &program_id, &lesson_id, question_index).await?;
        let stored = stored_from(row)?.ok_or_else(|| AppError::NotFound(NOT_IN_LESSON.into()))?;
        let bank_item =
            LearningAssessmentRepository::stored_candidate_in(&mut read, &program_id, &stored.id)
                .await?;
        let sections = blocks::section_count(&mut read, &lesson_id).await?;
        read.commit().await.map_err(db)?;
        if stored.text != before {
            return Err(AppError::InvalidInput(QUESTION_CHANGED.into()));
        }
        let new = effective(&before, after);
        if new == stored.text {
            return Ok(QuestionEditOutcome {
                changed: false,
                statements_kept: 0,
                statements_changed: 0,
                statements_removed: 0,
                sentences_written: 0,
                checked_statements_affected: 0,
                revision: None,
            });
        }
        let edited = edited_candidate(bank_item.candidate, &new);
        engine::validate_candidate(&edited)?;
        let token = content_hash(&StoredItem::from(edited));
        if self.token_repeated(&program_id, &stored.id, &token).await? {
            return Err(AppError::InvalidInput(SAME_QUESTION.into()));
        }

        let unit = usize::try_from(sections)
            .unwrap_or_default()
            .saturating_add(question_index);
        let claims: &[ClaimLedgerEntry] = if ledger.review.is_some() {
            &ledger.claims
        } else {
            &[]
        };
        let old = &stored.text;
        let mut changes = vec![field_change(
            claims,
            unit,
            "prompt".into(),
            UnitKind::Question,
            &old.prompt,
            &new.prompt,
            true,
        )];
        for slot in 0..old.choices.len().max(new.choices.len()) {
            changes.push(field_change(
                claims,
                unit,
                format!("options/{slot}"),
                UnitKind::Question,
                old.choices.get(slot).map_or("", String::as_str),
                new.choices.get(slot).map_or("", String::as_str),
                true,
            ));
        }
        changes.push(field_change(
            claims,
            unit,
            "explanation".into(),
            UnitKind::AnswerKey,
            &old.explanation,
            &new.explanation,
            false,
        ));
        for slot in 0..old.rationales.len().max(new.rationales.len()) {
            changes.push(field_change(
                claims,
                unit,
                format!("rationales/{slot}"),
                UnitKind::AnswerKey,
                old.rationales.get(slot).map_or("", String::as_str),
                new.rationales.get(slot).map_or("", String::as_str),
                false,
            ));
        }
        let mut ending = Vec::new();
        for change in &changes {
            ending.extend(endings(claims, &change.plan, &change.old));
        }
        let written: Vec<_> = changes
            .iter()
            .flat_map(|change| {
                change
                    .plan
                    .written
                    .iter()
                    .map(move |item| (change.field.as_str(), change.kind, item))
            })
            .collect();
        let checked = ending
            .iter()
            .filter(|item| matches!(item.entry.state(), ClaimState::ResolvedByEvidence { .. }))
            .count();
        let total = |of: fn(&FieldPlan) -> usize| {
            count(changes.iter().map(|change| of(&change.plan)).sum())
        };
        let mut outcome = QuestionEditOutcome {
            changed: true,
            statements_kept: total(|plan| plan.kept.len()),
            statements_changed: total(|plan| plan.edited.len()),
            statements_removed: total(|plan| plan.removed.len()),
            sentences_written: count(written.len()),
            checked_statements_affected: count(checked),
            revision: None,
        };
        if dry_run {
            return Ok(outcome);
        }

        let write = QuestionWrite {
            program_id: &program_id,
            lesson_id: &lesson_id,
            question_index,
            unit,
            stored: &stored,
            token: &token,
            new: &new,
            written: &written,
            ending: &ending,
            at: now(),
        };
        let moved = self.write_edit(&ledger, &write).await?;
        outcome.revision = self.settled_revision(&ledger, &lesson_id, moved).await?;
        Ok(outcome)
    }

    /// Whether another question of the program has this content token.
    async fn token_repeated(
        &self,
        program_id: &str,
        candidate_id: &str,
        token: &str,
    ) -> Result<bool> {
        let mut read = self.pool().begin_read().await.map_err(db)?;
        let repeated =
            questions::token_taken_by_another(&mut read, program_id, candidate_id, token).await?;
        read.commit().await.map_err(db)?;
        Ok(repeated)
    }

    /// The edit and its settlement in one unit of work: the question, its
    /// key, the ledger's events, the stored report, the teaching notes'
    /// marker and what each ended statement gives the checker examples and
    /// calibration. Returns the kinds whose bar the settlement moved.
    async fn write_edit(
        &self,
        ledger: &Ledger,
        write: &QuestionWrite<'_>,
    ) -> Result<Vec<ClaimKind>> {
        let QuestionWrite {
            program_id,
            lesson_id,
            question_index,
            unit,
            stored,
            token,
            new,
            written,
            ending,
            at,
        } = *write;
        let user = ledger.review.as_ref().map(provenance);
        let mut tx = self.pool().begin_write().await.map_err(db)?;
        if ledger.review.is_some() {
            check_revision(&mut tx, lesson_id, ledger.revision).await?;
        }
        let row = questions::question_at(&mut tx, program_id, lesson_id, question_index).await?;
        let now_stored = stored_from(row)?;
        if !now_stored
            .as_ref()
            .is_some_and(|now| now.id == stored.id && now.text == stored.text)
        {
            return Err(AppError::InvalidInput(QUESTION_CHANGED.into()));
        }
        if questions::token_taken_by_another(&mut tx, program_id, &stored.id, token).await? {
            return Err(AppError::InvalidInput(SAME_QUESTION.into()));
        }
        let planned = match &user {
            Some(user) => record_edit(&mut tx, lesson_id, unit, written, ending, user).await?,
            None => Vec::new(),
        };
        let single = new.kind == LearningItemFormat::SingleChoice;
        let options = if single {
            Some(serde_json::to_string(&new.choices)?)
        } else {
            None
        };
        let key = match new.right {
            Some(right) if has_choices(new.kind) => Some(serde_json::to_string(
                &LearningAssessmentKey::Choice(right),
            )?),
            _ => None,
        };
        let reasons = serde_json::to_string(&new.rationales)?;
        questions::write_question(&mut tx, &stored.id, &new.prompt, options.as_deref(), token)
            .await?;
        questions::write_answer_key(
            &mut tx,
            &stored.id,
            &new.explanation,
            key.as_deref(),
            &reasons,
        )
        .await?;
        let ended: HashSet<String> = ending
            .iter()
            .map(|item| item.entry.claim_id().0.clone())
            .collect();
        let report_written: Vec<(&str, &_)> = if user.is_some() {
            written
                .iter()
                .map(|(_, _, item)| (item.text.as_str(), &item.successor))
                .collect()
        } else {
            Vec::new()
        };
        let text = unit_text(new);
        update_report(&mut tx, program_id, lesson_id, |report| {
            report.rewritten(&Rewritten {
                unit,
                new_title: &new.prompt,
                new_body: &text,
                ended: &ended,
                written: &report_written,
                at,
            })
        })
        .await?;
        teaching_findings::mark_part_changed(&mut tx, lesson_id, "question", question_index, at)
            .await?;
        if ledger.review.is_some() {
            bump_revision(&mut tx, lesson_id).await?;
        }
        let moved = self
            .settle_planned_in(&mut tx, ledger, lesson_id, &planned)
            .await?;
        tx.commit().await.map_err(db)?;
        Ok(moved)
    }
}

/// What a question edit writes: the question it opened, its new text and
/// content token, the statements it writes and the ones it ends.
#[derive(Clone, Copy)]
struct QuestionWrite<'a> {
    program_id: &'a str,
    lesson_id: &'a str,
    question_index: usize,
    unit: usize,
    stored: &'a Stored,
    token: &'a str,
    new: &'a QuestionText,
    written: &'a [(&'a str, UnitKind, &'a Written)],
    ending: &'a [Ending<'a>],
    at: i64,
}
