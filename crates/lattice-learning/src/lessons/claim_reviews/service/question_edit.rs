//! The user's rewrite of one question of a published lesson: the statements on
//! text they changed end, the ones on text they left stay as they are, and
//! each sentence they wrote becomes a statement they confirmed. One
//! transaction writes the question, its key, the ledger, the stored report
//! and the teaching notes' marker, so a refusal writes nothing.

use std::collections::HashSet;

use lattice_core::domain::learning::claim_review::{ClaimLedgerEntry, ClaimState, UnitKind};
use lattice_core::shared::error::{AppError, Result};
use sqlx::SqliteConnection;

use super::section_edit::{count, endings, record_edit};
use super::{provenance, ClaimReviewService};
use crate::assessment_engine::{
    self as engine, normalize_answer, LearningAssessmentKey, LearningItemFormat,
};
use crate::assessment_generation::StoredItem;
use crate::assessment_repository::{content_hash, LearningAssessmentRepository};
use crate::content_verification::Rewritten;
use crate::lessons::claim_reviews::published_edit::update_report;
use crate::lessons::claim_reviews::published_rewrite::{plan, FieldPlan};
use crate::lessons::claim_reviews::repository::{bump_revision, check_revision};
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
/// right choice and its explanation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuestionText {
    pub kind: LearningItemFormat,
    pub prompt: String,
    /// Empty for a kind without choices.
    pub choices: Vec<String>,
    /// The index of the right choice; none for a kind without one.
    pub right: Option<usize>,
    pub explanation: String,
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

/// Whether the kind's choices are the question's own and its key is one of them.
fn has_choices(kind: LearningItemFormat) -> bool {
    matches!(
        kind,
        LearningItemFormat::SingleChoice | LearningItemFormat::TrueFalse
    )
}

async fn stored_question(
    connection: &mut SqliteConnection,
    program_id: &str,
    lesson_id: &str,
    index: usize,
) -> Result<Option<Stored>> {
    let row = sqlx::query_as::<_, (String, String, String, String, String, String)>(
        "SELECT c.id, c.format, c.prompt, c.options_json, k.key_json, k.explanation \
         FROM learning_assessment_candidates c \
         JOIN learning_assessment_answer_keys k ON k.candidate_id=c.id \
         WHERE c.program_id=? AND c.lesson_id=? AND c.origin IN ('lesson','migrated') \
         ORDER BY c.rowid LIMIT 1 OFFSET ?",
    )
    .bind(program_id)
    .bind(lesson_id)
    .bind(i64::try_from(index).unwrap_or(i64::MAX))
    .fetch_optional(&mut *connection)
    .await
    .map_err(db)?;
    let Some((id, format, prompt, options, key, explanation)) = row else {
        return Ok(None);
    };
    let kind = parse_format(&format)?;
    let (choices, right) = if has_choices(kind) {
        let choices: Vec<String> = serde_json::from_str(&options)?;
        let right = match serde_json::from_str::<LearningAssessmentKey>(&key)? {
            LearningAssessmentKey::Choice(index) => Some(index),
            _ => None,
        };
        (choices, right)
    } else {
        (Vec::new(), None)
    };
    Ok(Some(Stored {
        id,
        text: QuestionText {
            kind,
            prompt,
            choices,
            right,
            explanation,
        },
    }))
}

/// Whether another question of the program has this content token.
async fn repeats_another(
    connection: &mut SqliteConnection,
    program_id: &str,
    candidate_id: &str,
    token: &str,
) -> Result<bool> {
    let other: Option<i64> = sqlx::query_scalar(
        "SELECT 1 FROM learning_assessment_candidates WHERE program_id=? AND content_sha256=? AND id<>?",
    )
    .bind(program_id)
    .bind(token)
    .bind(candidate_id)
    .fetch_optional(&mut *connection)
    .await
    .map_err(db)?;
    Ok(other.is_some())
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

/// What the edit writes: the choices change only for a single-choice
/// question, the right answer only where the kind has one.
fn effective(before: &QuestionText, after: QuestionText) -> QuestionText {
    QuestionText {
        kind: before.kind,
        prompt: after.prompt,
        choices: if before.kind == LearningItemFormat::SingleChoice {
            after.choices
        } else {
            before.choices.clone()
        },
        right: if has_choices(before.kind) {
            after.right
        } else {
            before.right
        },
        explanation: after.explanation,
    }
}

/// Whether the choices or the right answer differ, which makes the stored
/// per-choice rationales stale.
fn answer_changed(old: &QuestionText, new: &QuestionText) -> bool {
    new.choices != old.choices || new.right != old.right
}

/// The stored bank item with the edit applied, as the quiz engine will read it.
fn edited_candidate(
    stored: engine::LearningAssessmentCandidate,
    old: &QuestionText,
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
    if answer_changed(old, new) {
        candidate.distractor_rationales.clear();
    }
    candidate
}

/// The unit's text as the report stores it: its choices and explanation
/// joined by blank lines.
fn unit_text(question: &QuestionText) -> String {
    question
        .choices
        .iter()
        .map(String::as_str)
        .chain(std::iter::once(question.explanation.as_str()))
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
        let mut connection = self.pool().acquire().await.map_err(db)?;
        stored_question(&mut connection, program_id, lesson_id, question_index)
            .await?
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
        let mut connection = self.pool().acquire().await.map_err(db)?;
        let stored = stored_question(&mut connection, &program_id, &lesson_id, question_index)
            .await?
            .ok_or_else(|| AppError::NotFound(NOT_IN_LESSON.into()))?;
        let bank_item = LearningAssessmentRepository::stored_candidate_in(
            &mut connection,
            &program_id,
            &stored.id,
        )
        .await?;
        let sections: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM learning_blocks WHERE lesson_id=?")
                .bind(&lesson_id)
                .fetch_one(&mut *connection)
                .await
                .map_err(db)?;
        drop(connection);
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
        let edited = edited_candidate(bank_item.candidate, &stored.text, &new);
        engine::validate_candidate(&edited)?;
        let token = content_hash(&StoredItem::from(edited));
        let mut connection = self.pool().acquire().await.map_err(db)?;
        let repeated = repeats_another(&mut connection, &program_id, &stored.id, &token).await?;
        drop(connection);
        if repeated {
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

        let at = now();
        let user = ledger.review.as_ref().map(provenance);
        let mut tx = self
            .pool()
            .begin_with("BEGIN IMMEDIATE")
            .await
            .map_err(db)?;
        if ledger.review.is_some() {
            check_revision(&mut tx, &lesson_id, ledger.revision).await?;
        }
        let now_stored = stored_question(&mut tx, &program_id, &lesson_id, question_index).await?;
        if !now_stored
            .as_ref()
            .is_some_and(|now| now.id == stored.id && now.text == stored.text)
        {
            return Err(AppError::InvalidInput(QUESTION_CHANGED.into()));
        }
        if repeats_another(&mut tx, &program_id, &stored.id, &token).await? {
            return Err(AppError::InvalidInput(SAME_QUESTION.into()));
        }
        let planned = match &user {
            Some(user) => record_edit(&mut tx, &lesson_id, unit, &written, &ending, user).await?,
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
        let answer_changed = answer_changed(old, &new);
        sqlx::query(
            "UPDATE learning_assessment_candidates SET prompt=?, options_json=COALESCE(?, options_json), content_sha256=?, review_status=CASE WHEN review_status='dropped' THEN 'agreed' ELSE review_status END WHERE id=?",
        )
        .bind(&new.prompt)
        .bind(options)
        .bind(&token)
        .bind(&stored.id)
        .execute(&mut *tx)
        .await
        .map_err(db)?;
        sqlx::query(
            "UPDATE learning_assessment_answer_keys SET explanation=?, key_json=COALESCE(?, key_json), \
             distractor_rationales_json=CASE WHEN ? THEN '[]' ELSE distractor_rationales_json END \
             WHERE candidate_id=?",
        )
        .bind(&new.explanation)
        .bind(key)
        .bind(answer_changed)
        .bind(&stored.id)
        .execute(&mut *tx)
        .await
        .map_err(db)?;
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
        let text = unit_text(&new);
        update_report(&mut tx, &program_id, &lesson_id, |report| {
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
        sqlx::query(
            "UPDATE learning_lesson_teaching_findings SET part_changed_at=? \
             WHERE lesson_id=? AND site='question' AND unit_index=?",
        )
        .bind(at)
        .bind(&lesson_id)
        .bind(i64::try_from(question_index).unwrap_or(i64::MAX))
        .execute(&mut *tx)
        .await
        .map_err(db)?;
        if ledger.review.is_some() {
            bump_revision(&mut tx, &lesson_id).await?;
        }
        tx.commit().await.map_err(db)?;

        outcome.revision = self.settle_planned(&ledger, &lesson_id, planned).await?;
        Ok(outcome)
    }
}
