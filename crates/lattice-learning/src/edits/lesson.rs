use std::sync::Arc;

use async_trait::async_trait;
use lattice_core::shared::edits::{
    Change, ContentKind, EditRegistry, Editability, FieldProblem, FieldSpec, Loaded, TextFormat,
};
use lattice_core::shared::error::{AppError, Result};
use lattice_core::shared::persistence::{Reading, WriteTx};
use serde_json::{json, Value};

use super::keys;
use crate::planning::curriculum_repository::corrections::{
    correct_lesson, lesson_row, LessonCorrection,
};

pub(crate) const OUTLINE_DRAFT: &str =
    "This course's outline is still a draft. You can edit it once the course has started.";
const FIXED_TIME_NOTE: &str =
    "The time of a completed lesson, or of one with a started quiz or practice, can't change.";

pub(crate) struct LessonKind;

pub(crate) fn register(registry: &mut EditRegistry) -> Result<()> {
    registry.add_kind(Arc::new(LessonKind))
}

#[async_trait]
impl ContentKind for LessonKind {
    fn name(&self) -> &'static str {
        keys::LESSON
    }

    fn affects(&self) -> &'static [&'static str] {
        &[keys::affected::PROGRAM, keys::affected::PLAN]
    }

    async fn load(&self, unit: &mut Reading<'_>, key: &str) -> Result<Option<Loaded>> {
        let Some(row) = lesson_row(unit, key).await? else {
            return Ok(None);
        };
        let mut form = vec![
            FieldSpec::Text {
                path: "/title".into(),
                label: "Title".into(),
                format: TextFormat::Line,
                required: true,
            },
            FieldSpec::Text {
                path: "/objective".into(),
                label: "Objective".into(),
                format: TextFormat::Plain,
                required: true,
            },
        ];
        let mut document = json!({
            "title": row.title,
            "objective": row.objective,
        });
        let mut notes = Vec::new();
        if row.plan_fixed {
            notes.push(FIXED_TIME_NOTE.into());
        } else {
            form.push(FieldSpec::Number {
                path: "/estimatedMinutes".into(),
                label: "Time".into(),
                min: 1,
                unit: "minutes".into(),
            });
            if let Some(map) = document.as_object_mut() {
                map.insert("estimatedMinutes".into(), json!(row.estimated_minutes));
            }
        }
        Ok(Some(Loaded {
            label: format!("Lesson: {}", row.title),
            scope: keys::learning_scope(&row.program_id),
            editability: if row.draft {
                Editability::NotNow {
                    reason: OUTLINE_DRAFT.into(),
                }
            } else {
                Editability::Editable
            },
            form,
            document,
            notes,
        }))
    }

    async fn check(
        &self,
        _unit: &mut Reading<'_>,
        _key: &str,
        _before: &Value,
        _after: &Value,
    ) -> Result<Vec<FieldProblem>> {
        Ok(Vec::new())
    }

    async fn store(&self, tx: &mut WriteTx, change: &Change<'_>) -> Result<()> {
        let title = change
            .after
            .get("title")
            .and_then(Value::as_str)
            .ok_or_else(|| AppError::InvalidInput("Lesson title must be a string.".into()))?;
        let objective = change
            .after
            .get("objective")
            .and_then(Value::as_str)
            .ok_or_else(|| AppError::InvalidInput("Lesson objective must be a string.".into()))?;
        let estimated_minutes = match change.after.get("estimatedMinutes") {
            Some(value) => {
                let raw = value.as_i64().ok_or_else(|| {
                    AppError::InvalidInput("Lesson time must be a positive integer.".into())
                })?;
                let mins = u32::try_from(raw)
                    .ok()
                    .filter(|minutes| *minutes >= 1)
                    .ok_or_else(|| {
                        AppError::InvalidInput("Lesson time must be at least 1 minute.".into())
                    })?;
                Some(mins)
            }
            None => None,
        };
        correct_lesson(
            tx,
            &change.scope.container,
            &LessonCorrection {
                lesson_id: &change.content.key,
                title,
                objective,
                estimated_minutes,
                at: change.at,
            },
        )
        .await
    }
}
