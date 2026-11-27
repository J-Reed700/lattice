//! The `learning.question` content kind: opening, validating, and writing
//! one bank or lesson question by its candidate ID.

use std::collections::{BTreeSet, HashSet};
use std::sync::Arc;

use async_trait::async_trait;
use serde_json::{json, Value};

use crate::assessment_engine::checks::{
    repeated_option, validate_candidate, MULTI_SELECT_OPTIONS, SINGLE_CHOICE_OPTIONS, TRUE_FALSE,
};
use crate::assessment_engine::{
    LearningAssessmentCandidate, LearningAssessmentKey, LearningItemFormat,
};
use crate::assessment_generation::StoredItem;
use crate::assessment_repository::{
    content_hash, CandidateReviewStatus, LearningAssessmentRepository, StoredCandidate,
};
use crate::edits::keys::{self, affected, learning_scope};
use crate::edits::BEING_PREPARED;
use crate::lessons::claim_reviews::repository::{blocks, questions};
use lattice_core::domain::learning::claim_review::LessonReviewState;
use lattice_core::shared::edits::{
    Change, ContentKind, EditRegistry, Editability, FieldProblem, FieldSpec, Loaded, TextFormat,
};
use lattice_core::shared::error::{AppError, Result};
use lattice_core::shared::persistence::{Reading, WriteTx};

pub const IN_A_QUIZ: &str =
    "This question is in a quiz you have started. Finish or submit the quiz before editing it.";
pub const KEPT_AS_ASKED: &str = "Quizzes already submitted keep the question as it was asked.";
pub const REPORTED: &str = "This question was reported.";
pub const NO_RIGHT: &str = "Mark a choice as the right answer.";
pub const SAME_CHOICES: &str = "Two choices say the same thing.";
pub const SAME_QUESTION: &str = "Another question in this course has the same prompt and choices.";

struct QuestionKind;

pub(crate) fn register(registry: &mut EditRegistry) -> Result<()> {
    registry.add_kind(Arc::new(QuestionKind))
}

/// Where a lesson's own question sits in the claim ledger, if it belongs to one.
pub(crate) struct LedgerUnit {
    pub lesson_id: String,
    pub unit_index: usize,
    pub format: LearningItemFormat,
}

pub(crate) async fn ledger_unit(
    unit: &mut Reading<'_>,
    candidate_id: &str,
) -> Result<Option<LedgerUnit>> {
    let Some(home) = questions::question_home(&mut *unit, candidate_id).await? else {
        return Ok(None);
    };
    let Some(lesson_id) = home.lesson_id else {
        return Ok(None);
    };
    let lesson_items = questions::lesson_questions(&mut *unit, &lesson_id).await?;
    let Some(position) = lesson_items.iter().position(|item| item.id == candidate_id) else {
        return Ok(None);
    };
    let Some(stored) = LearningAssessmentRepository::find_stored_candidate_in(
        &mut *unit,
        &home.program_id,
        candidate_id,
    )
    .await?
    else {
        return Ok(None);
    };
    let sections =
        usize::try_from(blocks::section_count(&mut *unit, &lesson_id).await?).unwrap_or(usize::MAX);
    Ok(Some(LedgerUnit {
        lesson_id,
        unit_index: sections.saturating_add(position),
        format: stored.candidate.format,
    }))
}

/// Maps a claim location field name to the JSON Pointer inside `learning.question`.
pub(crate) fn document_path(format: LearningItemFormat, field: &str) -> String {
    match field {
        "prompt" => "/prompt".into(),
        "explanation" => "/explanation".into(),
        _ => {
            if let Some(index) = field.strip_prefix("options/") {
                match format {
                    LearningItemFormat::SingleChoice | LearningItemFormat::MultiSelect => {
                        return format!("/choices/{index}/text");
                    }
                    LearningItemFormat::TrueFalse => return "/answer".into(),
                    _ => {}
                }
            }
            format!("/{field}")
        }
    }
}

pub(crate) fn document_of(stored: &StoredCandidate) -> Value {
    let candidate = &stored.candidate;
    match candidate.format {
        LearningItemFormat::SingleChoice | LearningItemFormat::MultiSelect => {
            let choices: Vec<Value> = candidate
                .options
                .iter()
                .enumerate()
                .map(|(index, text)| {
                    let correct = match &candidate.key {
                        LearningAssessmentKey::Choice(right) => *right == index,
                        LearningAssessmentKey::Choices(rights) => rights.contains(&index),
                        _ => false,
                    };
                    let rationale = candidate
                        .distractor_rationales
                        .get(index)
                        .cloned()
                        .unwrap_or_default();
                    json!({
                        "text": text,
                        "correct": correct,
                        "rationale": rationale,
                    })
                })
                .collect();
            json!({
                "prompt": candidate.prompt,
                "choices": choices,
                "explanation": candidate.explanation,
            })
        }
        LearningItemFormat::TrueFalse => {
            let answer = match candidate.key {
                LearningAssessmentKey::Choice(0) => 0usize,
                _ => 1usize,
            };
            json!({
                "prompt": candidate.prompt,
                "answer": answer,
                "explanation": candidate.explanation,
            })
        }
        _ => json!({
            "prompt": candidate.prompt,
            "explanation": candidate.explanation,
        }),
    }
}

pub(crate) struct Composed {
    pub prompt: String,
    pub options: Option<Vec<String>>,
    pub key: Option<LearningAssessmentKey>,
    pub explanation: String,
    pub rationales: Vec<String>,
}

pub(crate) fn composed(
    stored: &StoredCandidate,
    document: &Value,
) -> std::result::Result<Composed, Vec<FieldProblem>> {
    let mut problems = Vec::new();
    let prompt = match document.get("prompt").and_then(Value::as_str) {
        Some(text) => text.to_owned(),
        None => {
            problems.push(FieldProblem {
                path: Some("/prompt".into()),
                message: "Question prompt must be text.".into(),
            });
            String::new()
        }
    };
    let explanation = match document.get("explanation").and_then(Value::as_str) {
        Some(text) => text.to_owned(),
        None => {
            problems.push(FieldProblem {
                path: Some("/explanation".into()),
                message: "Question explanation must be text.".into(),
            });
            String::new()
        }
    };

    let mut options = None;
    let mut key = None;
    let mut rationales = stored.candidate.distractor_rationales.clone();

    match stored.candidate.format {
        LearningItemFormat::SingleChoice | LearningItemFormat::MultiSelect => {
            let Some(raw_choices) = document.get("choices").and_then(Value::as_array) else {
                problems.push(FieldProblem {
                    path: Some("/choices".into()),
                    message: "Choices must be a list.".into(),
                });
                return Err(problems);
            };
            let mut texts = Vec::with_capacity(raw_choices.len());
            let mut rights = Vec::new();
            let mut parsed_rationales = Vec::with_capacity(raw_choices.len());
            for (index, item) in raw_choices.iter().enumerate() {
                let text = item
                    .get("text")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_owned();
                let correct = item
                    .get("correct")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                let rationale = item
                    .get("rationale")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_owned();
                texts.push(text);
                if correct {
                    rights.push(index);
                }
                parsed_rationales.push(rationale);
            }

            if rights.is_empty() {
                problems.push(FieldProblem {
                    path: Some("/choices".into()),
                    message: NO_RIGHT.into(),
                });
            } else if stored.candidate.format == LearningItemFormat::SingleChoice
                && rights.len() > 1
            {
                problems.push(FieldProblem {
                    path: Some("/choices".into()),
                    message: "Only one choice can be marked right.".into(),
                });
            }

            if repeated_option(&texts).is_some() {
                let mut seen = HashSet::new();
                for (index, text) in texts.iter().enumerate() {
                    let normalized = text.split_whitespace().collect::<Vec<_>>().join(" ");
                    if !seen.insert(normalized) {
                        problems.push(FieldProblem {
                            path: Some(format!("/choices/{index}/text")),
                            message: SAME_CHOICES.into(),
                        });
                    }
                }
                if !problems.iter().any(|p| p.message == SAME_CHOICES) {
                    problems.push(FieldProblem {
                        path: Some("/choices".into()),
                        message: SAME_CHOICES.into(),
                    });
                }
            }

            if problems.is_empty() {
                key = Some(match stored.candidate.format {
                    LearningItemFormat::SingleChoice => {
                        let Some(&right) = rights.first() else {
                            return Err(vec![FieldProblem {
                                path: Some("/choices".into()),
                                message: NO_RIGHT.into(),
                            }]);
                        };
                        LearningAssessmentKey::Choice(right)
                    }
                    _ => {
                        LearningAssessmentKey::Choices(rights.into_iter().collect::<BTreeSet<_>>())
                    }
                });
                if stored.candidate.distractor_rationales.is_empty()
                    && parsed_rationales.iter().all(|r| r.is_empty())
                {
                    rationales = Vec::new();
                } else {
                    rationales = parsed_rationales;
                }
                options = Some(texts);
            }
        }
        LearningItemFormat::TrueFalse => match document.get("answer").and_then(Value::as_u64) {
            Some(0) => {
                key = Some(LearningAssessmentKey::Choice(0));
            }
            Some(1) => {
                key = Some(LearningAssessmentKey::Choice(1));
            }
            _ => {
                problems.push(FieldProblem {
                    path: Some("/answer".into()),
                    message: NO_RIGHT.into(),
                });
            }
        },
        _ => {}
    }

    if !problems.is_empty() {
        return Err(problems);
    }

    Ok(Composed {
        prompt,
        options,
        key,
        explanation,
        rationales,
    })
}

fn candidate_with(
    base: &LearningAssessmentCandidate,
    edited: &Composed,
) -> LearningAssessmentCandidate {
    let mut updated = base.clone();
    updated.prompt = edited.prompt.clone();
    if let Some(options) = &edited.options {
        updated.options = options.clone();
    }
    if let Some(key) = &edited.key {
        updated.key = key.clone();
    }
    updated.explanation = edited.explanation.clone();
    updated.distractor_rationales = edited.rationales.clone();
    updated
}

fn question_form(format: LearningItemFormat) -> Vec<FieldSpec> {
    let mut form = vec![FieldSpec::Text {
        path: "/prompt".into(),
        label: "Prompt".into(),
        format: TextFormat::Plain,
        required: true,
    }];
    match format {
        LearningItemFormat::SingleChoice | LearningItemFormat::MultiSelect => {
            let (min_items, max_items) = match format {
                LearningItemFormat::SingleChoice => (
                    u32::try_from(*SINGLE_CHOICE_OPTIONS.start()).unwrap_or(2),
                    u32::try_from(*SINGLE_CHOICE_OPTIONS.end()).unwrap_or(8),
                ),
                _ => (
                    u32::try_from(*MULTI_SELECT_OPTIONS.start()).unwrap_or(4),
                    u32::try_from(*MULTI_SELECT_OPTIONS.end()).unwrap_or(6),
                ),
            };
            form.push(FieldSpec::List {
                path: "/choices".into(),
                label: "Choices".into(),
                item_label: "Choice".into(),
                min_items,
                max_items: Some(max_items),
                item: vec![
                    FieldSpec::Text {
                        path: "/text".into(),
                        label: "Choice".into(),
                        format: TextFormat::Plain,
                        required: true,
                    },
                    FieldSpec::Flag {
                        path: "/correct".into(),
                        label: "Right answer".into(),
                        exclusive: format == LearningItemFormat::SingleChoice,
                    },
                    FieldSpec::Text {
                        path: "/rationale".into(),
                        label: "Why wrong".into(),
                        format: TextFormat::Plain,
                        required: false,
                    },
                ],
            });
        }
        LearningItemFormat::TrueFalse => {
            form.push(FieldSpec::Choice {
                path: "/answer".into(),
                label: "Right answer".into(),
                options: TRUE_FALSE.iter().map(|s| (*s).to_string()).collect(),
            });
        }
        _ => {}
    }
    form.push(FieldSpec::Text {
        path: "/explanation".into(),
        label: "Explanation".into(),
        format: TextFormat::Markdown,
        required: true,
    });
    form
}

#[async_trait]
impl ContentKind for QuestionKind {
    fn name(&self) -> &'static str {
        keys::QUESTION
    }

    fn affects(&self) -> &'static [&'static str] {
        &[affected::PROGRAM, affected::QUIZZES]
    }

    async fn load(&self, unit: &mut Reading<'_>, key: &str) -> Result<Option<Loaded>> {
        let Some(home) = questions::question_home(&mut *unit, key).await? else {
            return Ok(None);
        };
        let Some(stored) = LearningAssessmentRepository::find_stored_candidate_in(
            &mut *unit,
            &home.program_id,
            key,
        )
        .await?
        else {
            return Ok(None);
        };
        let place = questions::place_of(&mut *unit, key).await?;

        let editability = if home
            .review_state
            .as_deref()
            .is_some_and(|state| state != LessonReviewState::Ready.as_str())
        {
            Editability::NotNow {
                reason: BEING_PREPARED.into(),
            }
        } else if place.in_open_quiz {
            Editability::NotNow {
                reason: IN_A_QUIZ.into(),
            }
        } else {
            Editability::Editable
        };

        let mut notes = Vec::new();
        if place.on_a_submitted_quiz {
            notes.push(KEPT_AS_ASKED.into());
        }
        if home.review_status == CandidateReviewStatus::Reported || home.has_reports {
            notes.push(REPORTED.into());
        }

        Ok(Some(Loaded {
            label: format!("Question: {}", stored.candidate.prompt),
            scope: learning_scope(&home.program_id),
            editability,
            form: question_form(stored.candidate.format),
            document: document_of(&stored),
            notes,
        }))
    }

    async fn check(
        &self,
        unit: &mut Reading<'_>,
        key: &str,
        _before: &Value,
        after: &Value,
    ) -> Result<Vec<FieldProblem>> {
        let Some(home) = questions::question_home(&mut *unit, key).await? else {
            return Ok(Vec::new());
        };
        let Some(stored) = LearningAssessmentRepository::find_stored_candidate_in(
            &mut *unit,
            &home.program_id,
            key,
        )
        .await?
        else {
            return Ok(Vec::new());
        };
        let edited = match composed(&stored, after) {
            Ok(edited) => edited,
            Err(problems) => return Ok(problems),
        };
        let candidate = candidate_with(&stored.candidate, &edited);
        if let Err(err) = validate_candidate(&candidate) {
            return match err {
                AppError::InvalidInput(message) | AppError::InvalidState(message) => {
                    Ok(vec![FieldProblem {
                        path: None,
                        message,
                    }])
                }
                other => Err(other),
            };
        }
        let token = content_hash(&StoredItem::from(candidate));
        if questions::token_taken_by_another(&mut *unit, &home.program_id, key, &token).await? {
            return Ok(vec![FieldProblem {
                path: None,
                message: SAME_QUESTION.into(),
            }]);
        }
        Ok(Vec::new())
    }

    async fn store(&self, tx: &mut WriteTx, change: &Change<'_>) -> Result<()> {
        let key = &change.content.key;
        let home = questions::question_home(&mut *tx, key)
            .await?
            .ok_or_else(|| AppError::NotFound("Question not found.".into()))?;
        let stored =
            LearningAssessmentRepository::find_stored_candidate_in(&mut *tx, &home.program_id, key)
                .await?
                .ok_or_else(|| AppError::NotFound("Question not found.".into()))?;
        let edited = composed(&stored, change.after).map_err(|problems| {
            let message = problems
                .into_iter()
                .map(|p| p.message)
                .collect::<Vec<_>>()
                .join(" ");
            AppError::InvalidInput(message)
        })?;
        let candidate = candidate_with(&stored.candidate, &edited);
        let token = content_hash(&StoredItem::from(candidate));
        let options_json = edited
            .options
            .as_ref()
            .map(serde_json::to_string)
            .transpose()?;
        let key_json = edited.key.as_ref().map(serde_json::to_string).transpose()?;
        let rationales_json = serde_json::to_string(&edited.rationales)?;
        questions::write_question(tx, key, &edited.prompt, options_json.as_deref(), &token).await?;
        questions::write_answer_key(
            tx,
            key,
            &edited.explanation,
            key_json.as_deref(),
            &rationales_json,
        )
        .await?;
        Ok(())
    }
}
