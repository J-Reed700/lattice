//! The `learning.section` content kind: opening, validating, and writing
//! one section of a published lesson.

use std::sync::Arc;

use async_trait::async_trait;
use serde_json::{json, Value};

use crate::edits::keys::{self, affected, learning_scope, section_key};
use crate::edits::BEING_PREPARED;
use crate::lessons::claim_reviews::repository::blocks;
use crate::lessons::generation::split_check;
use lattice_core::shared::edits::{
    Change, ContentKind, EditRegistry, Editability, FieldProblem, FieldSpec, Loaded, TextFormat,
};
use lattice_core::shared::error::{AppError, Result};
use lattice_core::shared::persistence::{Reading, WriteTx};

/// Refusal when a check section loses or duplicates its single reveal heading.
pub const KEEP_CHECK: &str =
    "This section is a check: keep the one prompt-and-answer heading line and a blank line between them.";

struct SectionKind;

pub(crate) fn register(registry: &mut EditRegistry) -> Result<()> {
    registry.add_kind(Arc::new(SectionKind))
}

fn section_form() -> Vec<FieldSpec> {
    vec![
        FieldSpec::Text {
            path: "/title".into(),
            label: "Title".into(),
            format: TextFormat::Line,
            required: true,
        },
        FieldSpec::Text {
            path: "/body".into(),
            label: "Body".into(),
            format: TextFormat::Markdown,
            required: true,
        },
    ]
}

#[async_trait]
impl ContentKind for SectionKind {
    fn name(&self) -> &'static str {
        keys::SECTION
    }

    fn affects(&self) -> &'static [&'static str] {
        &[affected::PROGRAM]
    }

    async fn load(&self, unit: &mut Reading<'_>, key: &str) -> Result<Option<Loaded>> {
        let Some((lesson_id, ordinal)) = section_key(key) else {
            return Ok(None);
        };
        let Some(row) = blocks::section_row(&mut *unit, lesson_id, ordinal).await? else {
            return Ok(None);
        };
        let editability = if row
            .review_state
            .as_deref()
            .is_some_and(|state| state != "ready")
        {
            Editability::NotNow {
                reason: BEING_PREPARED.into(),
            }
        } else {
            Editability::Editable
        };
        Ok(Some(Loaded {
            label: format!("Section {}: {}", ordinal + 1, row.title),
            scope: learning_scope(&row.program_id),
            editability,
            form: section_form(),
            document: json!({
                "title": row.title,
                "body": row.body,
            }),
            notes: Vec::new(),
        }))
    }

    async fn check(
        &self,
        unit: &mut Reading<'_>,
        key: &str,
        before: &Value,
        after: &Value,
    ) -> Result<Vec<FieldProblem>> {
        let Some((lesson_id, ordinal)) = section_key(key) else {
            return Ok(Vec::new());
        };
        let Some(row) = blocks::section_row(&mut *unit, lesson_id, ordinal).await? else {
            return Ok(Vec::new());
        };
        if row.kind != "check" {
            return Ok(Vec::new());
        }
        let before_body = before.get("body").and_then(Value::as_str).unwrap_or("");
        let after_body = after.get("body").and_then(Value::as_str).unwrap_or("");
        if (after_body != before_body || split_check(before_body).is_some())
            && split_check(after_body).is_none()
        {
            return Ok(vec![FieldProblem {
                path: Some("/body".into()),
                message: KEEP_CHECK.into(),
            }]);
        }
        Ok(Vec::new())
    }

    async fn store(&self, tx: &mut WriteTx, change: &Change<'_>) -> Result<()> {
        let Some((lesson_id, ordinal)) = section_key(&change.content.key) else {
            return Err(AppError::NotFound("Section not found.".into()));
        };
        let title = change
            .after
            .get("title")
            .and_then(Value::as_str)
            .ok_or_else(|| AppError::InvalidInput("Section title must be text.".into()))?;
        let body = change
            .after
            .get("body")
            .and_then(Value::as_str)
            .ok_or_else(|| AppError::InvalidInput("Section body must be text.".into()))?;
        blocks::write_section(tx, lesson_id, ordinal, title, body).await?;
        Ok(())
    }
}
