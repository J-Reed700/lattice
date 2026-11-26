use std::sync::Arc;

use async_trait::async_trait;
use lattice_core::shared::edits::{
    Change, ContentKind, EditOrigin, EditRegistry, Editability, FieldProblem, FieldSpec, Loaded,
    TextFormat,
};
use lattice_core::shared::error::{AppError, Result};
use lattice_core::shared::persistence::{Reading, WriteTx};
use serde_json::{json, Value};

use super::keys;
use crate::portability_dto::{LearningRecallCardFormat, LearningRecallContentDto};
use crate::recall::recall_content::validate_content;
use crate::recall::recall_repository::card_text::{self, CardRow};

const CHOOSE_RIGHT: &str = "Choose the right answer.";

pub(crate) struct CardKind;

pub(crate) fn register(registry: &mut EditRegistry) -> Result<()> {
    registry.add_kind(Arc::new(CardKind))
}

fn card_format(row: &CardRow) -> LearningRecallCardFormat {
    match &row.profile {
        Some(profile) => profile.format,
        None if row.multiple_choice => LearningRecallCardFormat::MultipleChoice,
        None => LearningRecallCardFormat::QuestionAnswer,
    }
}

fn card_content(row: &CardRow) -> LearningRecallContentDto {
    match &row.profile {
        Some(profile) => profile.content.clone(),
        None => LearningRecallContentDto {
            prompt: row.question.clone(),
            answer: row.answer.clone(),
            explanation: row.explanation.clone(),
            options: if row.multiple_choice {
                row.options.clone()
            } else {
                Vec::new()
            },
            correct_option_index: row.multiple_choice.then_some(row.correct_index),
            language: None,
            cloze_deletions: Vec::new(),
        },
    }
}

fn compose_content(
    format: LearningRecallCardFormat,
    stored: &LearningRecallContentDto,
    document: &Value,
) -> Result<std::result::Result<LearningRecallContentDto, FieldProblem>> {
    let prompt = document
        .get("question")
        .and_then(Value::as_str)
        .ok_or_else(|| AppError::InvalidState("Card question must be a string.".into()))?
        .to_owned();
    let explanation = document
        .get("explanation")
        .and_then(Value::as_str)
        .ok_or_else(|| AppError::InvalidState("Card explanation must be a string.".into()))?
        .to_owned();

    let (answer, options, correct_option_index) = if format
        == LearningRecallCardFormat::MultipleChoice
    {
        let choices = document
            .get("choices")
            .and_then(Value::as_array)
            .ok_or_else(|| AppError::InvalidState("Card choices must be an array.".into()))?;
        let mut options = Vec::with_capacity(choices.len());
        let mut correct_index = None;
        for (index, item) in choices.iter().enumerate() {
            let text = item
                .get("text")
                .and_then(Value::as_str)
                .ok_or_else(|| AppError::InvalidState("Card choice text must be a string.".into()))?
                .to_owned();
            let correct = item
                .get("correct")
                .and_then(Value::as_bool)
                .ok_or_else(|| {
                    AppError::InvalidState("Card choice flag must be a boolean.".into())
                })?;
            if correct && correct_index.is_none() {
                correct_index = Some(index);
            }
            options.push(text);
        }
        let Some(chosen) = correct_index else {
            return Ok(Err(FieldProblem {
                path: Some("/choices".into()),
                message: CHOOSE_RIGHT.into(),
            }));
        };
        let answer = options.get(chosen).cloned().ok_or_else(|| {
            AppError::InvalidState("Selected card choice index out of range.".into())
        })?;
        (answer, options, Some(chosen))
    } else {
        let answer = document
            .get("answer")
            .and_then(Value::as_str)
            .ok_or_else(|| AppError::InvalidState("Card answer must be a string.".into()))?
            .to_owned();
        (answer, stored.options.clone(), stored.correct_option_index)
    };

    let cloze_deletions = if format == LearningRecallCardFormat::Cloze {
        let deletions = document
            .get("deletions")
            .and_then(Value::as_array)
            .ok_or_else(|| AppError::InvalidState("Cloze deletions must be an array.".into()))?;
        let mut out = Vec::with_capacity(deletions.len());
        for item in deletions {
            let text = item
                .get("text")
                .and_then(Value::as_str)
                .ok_or_else(|| {
                    AppError::InvalidState("Cloze deletion text must be a string.".into())
                })?
                .to_owned();
            out.push(text);
        }
        out
    } else {
        stored.cloze_deletions.clone()
    };

    let language = if format == LearningRecallCardFormat::CodePrediction {
        let value = document
            .get("language")
            .and_then(Value::as_str)
            .ok_or_else(|| AppError::InvalidState("Card language must be a string.".into()))?;
        if value.is_empty() {
            None
        } else {
            Some(value.to_owned())
        }
    } else {
        stored.language.clone()
    };

    Ok(Ok(LearningRecallContentDto {
        prompt,
        answer,
        explanation,
        options,
        correct_option_index,
        language,
        cloze_deletions,
    }))
}

#[async_trait]
impl ContentKind for CardKind {
    fn name(&self) -> &'static str {
        keys::CARD
    }

    fn affects(&self) -> &'static [&'static str] {
        &[keys::affected::DECKS]
    }

    async fn load(&self, unit: &mut Reading<'_>, key: &str) -> Result<Option<Loaded>> {
        let Some(row) = card_text::card_row(unit, key).await? else {
            return Ok(None);
        };
        let format = card_format(&row);
        let content = card_content(&row);
        let first_line = content.prompt.lines().next().unwrap_or("");

        let mut form = vec![FieldSpec::Text {
            path: "/question".into(),
            label: "Question".into(),
            format: TextFormat::Markdown,
            required: true,
        }];
        let mut doc = serde_json::Map::new();
        doc.insert("question".into(), json!(content.prompt));

        if format == LearningRecallCardFormat::MultipleChoice {
            form.push(FieldSpec::List {
                path: "/choices".into(),
                label: "Choices".into(),
                item_label: "Choice".into(),
                min_items: 2,
                max_items: Some(8),
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
                        exclusive: true,
                    },
                ],
            });
            let choices: Vec<Value> = content
                .options
                .iter()
                .enumerate()
                .map(|(index, text)| {
                    json!({
                        "text": text,
                        "correct": content.correct_option_index == Some(index),
                    })
                })
                .collect();
            doc.insert("choices".into(), Value::Array(choices));
        } else {
            form.push(FieldSpec::Text {
                path: "/answer".into(),
                label: "Answer".into(),
                format: TextFormat::Markdown,
                required: true,
            });
            doc.insert("answer".into(), json!(content.answer));
        }

        if format == LearningRecallCardFormat::Cloze {
            form.push(FieldSpec::List {
                path: "/deletions".into(),
                label: "Hidden words".into(),
                item_label: "Hidden word".into(),
                min_items: 1,
                max_items: None,
                item: vec![FieldSpec::Text {
                    path: "/text".into(),
                    label: "Hidden word".into(),
                    format: TextFormat::Plain,
                    required: true,
                }],
            });
            let deletions: Vec<Value> = content
                .cloze_deletions
                .iter()
                .map(|text| json!({ "text": text }))
                .collect();
            doc.insert("deletions".into(), Value::Array(deletions));
        }

        if format == LearningRecallCardFormat::CodePrediction {
            form.push(FieldSpec::Text {
                path: "/language".into(),
                label: "Language".into(),
                format: TextFormat::Line,
                required: false,
            });
            doc.insert(
                "language".into(),
                json!(content.language.as_deref().unwrap_or("")),
            );
        }

        form.push(FieldSpec::Text {
            path: "/explanation".into(),
            label: "Explanation".into(),
            format: TextFormat::Markdown,
            required: false,
        });
        doc.insert("explanation".into(), json!(content.explanation));

        Ok(Some(Loaded {
            label: format!("Card: {first_line}"),
            scope: keys::study_scope(&row.deck_id),
            editability: Editability::Editable,
            form,
            document: Value::Object(doc),
            notes: Vec::new(),
        }))
    }

    async fn check(
        &self,
        unit: &mut Reading<'_>,
        key: &str,
        _before: &Value,
        after: &Value,
    ) -> Result<Vec<FieldProblem>> {
        let Some(row) = card_text::card_row(unit, key).await? else {
            return Ok(Vec::new());
        };
        let format = card_format(&row);
        let stored = card_content(&row);
        let composed = match compose_content(format, &stored, after)? {
            Ok(composed) => composed,
            Err(problem) => return Ok(vec![problem]),
        };
        if let Err(error) = validate_content(format, &composed) {
            let message = match error {
                AppError::InvalidInput(message) => message,
                other => other.to_string(),
            };
            return Ok(vec![FieldProblem {
                path: None,
                message,
            }]);
        }
        Ok(Vec::new())
    }

    async fn store(&self, tx: &mut WriteTx, change: &Change<'_>) -> Result<()> {
        let row = card_text::card_row(&mut tx.reading(), &change.content.key)
            .await?
            .ok_or_else(|| AppError::NotFound("Study card no longer exists".into()))?;
        let format = card_format(&row);
        let stored = card_content(&row);
        let composed = match compose_content(format, &stored, change.after)? {
            Ok(composed) => composed,
            Err(problem) => return Err(AppError::InvalidInput(problem.message)),
        };
        let reason = if matches!(change.origin, EditOrigin::Revert { .. }) {
            card_text::UNDID
        } else {
            card_text::CORRECTED
        };
        let profile = row.profile.as_ref().map(|_| (&composed, reason));
        card_text::write_card_text(
            tx,
            &change.content.key,
            &card_text::CardText {
                question: &composed.prompt,
                answer: &composed.answer,
                options: &composed.options,
                correct_index: composed.correct_option_index.unwrap_or(0),
                explanation: &composed.explanation,
                profile,
            },
            change.at,
        )
        .await
    }
}
