use std::sync::Arc;

use async_trait::async_trait;
use lattice_core::shared::edits::{
    Change, ContentKind, EditRegistry, Editability, FieldProblem, FieldSpec, Loaded, TextFormat,
};
use lattice_core::shared::error::{AppError, Result};
use lattice_core::shared::persistence::{Reading, WriteTx};
use serde_json::{json, Value};

use super::keys;
use super::lesson::OUTLINE_DRAFT;
use crate::planning::curriculum_repository::corrections::{
    correct_module, module_row, ModuleCorrection,
};

pub(crate) struct ModuleKind;

pub(crate) fn register(registry: &mut EditRegistry) -> Result<()> {
    registry.add_kind(Arc::new(ModuleKind))
}

#[async_trait]
impl ContentKind for ModuleKind {
    fn name(&self) -> &'static str {
        keys::MODULE
    }

    fn affects(&self) -> &'static [&'static str] {
        &[keys::affected::PROGRAM, keys::affected::PLAN]
    }

    async fn load(&self, unit: &mut Reading<'_>, key: &str) -> Result<Option<Loaded>> {
        let Some(row) = module_row(unit, key).await? else {
            return Ok(None);
        };
        Ok(Some(Loaded {
            label: format!("Module: {}", row.title),
            scope: keys::learning_scope(&row.program_id),
            editability: if row.draft {
                Editability::NotNow {
                    reason: OUTLINE_DRAFT.into(),
                }
            } else {
                Editability::Editable
            },
            form: vec![
                FieldSpec::Text {
                    path: "/title".into(),
                    label: "Title".into(),
                    format: TextFormat::Line,
                    required: true,
                },
                FieldSpec::Text {
                    path: "/summary".into(),
                    label: "Summary".into(),
                    format: TextFormat::Plain,
                    required: true,
                },
            ],
            document: json!({
                "title": row.title,
                "summary": row.summary,
            }),
            notes: Vec::new(),
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
            .ok_or_else(|| AppError::InvalidInput("Module title must be a string.".into()))?;
        let summary = change
            .after
            .get("summary")
            .and_then(Value::as_str)
            .ok_or_else(|| AppError::InvalidInput("Module summary must be a string.".into()))?;
        correct_module(
            tx,
            &change.scope.container,
            &ModuleCorrection {
                module_id: &change.content.key,
                title,
                summary,
                at: change.at,
            },
        )
        .await
    }
}
