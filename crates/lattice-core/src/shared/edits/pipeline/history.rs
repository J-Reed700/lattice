//! Edit history, summary queries, read marks, and field-level diffs.

use std::collections::{HashMap, HashSet};

use super::super::registry::EditRegistry;
use super::super::revision::revision_of;
use super::super::store;
use super::super::types::{
    ContentEditHistory, ContentEditQuery, ContentEditRecord, ContentEditSummary, ContentRef,
    EditOp, FieldChange, NewestEdit, ReadMark,
};
use super::apply::{extract_edited_field_paths, extract_paths};
use super::revert::{standing_edits, unbroken_from};
use super::{push_unique, require_kind, MISSING_ENTITY};
use crate::shared::error::{AppError, Result};
use crate::shared::persistence::Reading;

pub(super) async fn edited_paths_at(
    unit: &mut Reading<'_>,
    content: &ContentRef,
    current_revision: &str,
) -> Result<Vec<String>> {
    let rows = store::load_entity_edits(unit, content).await?;
    let standing = standing_edits(&rows);
    let mut paths = Vec::new();
    for idx in standing {
        if !unbroken_from(&rows, idx, current_revision) {
            continue;
        }
        if let Some(row) = rows.get(idx) {
            for path in extract_edited_field_paths(&row.ops) {
                push_unique(&mut paths, path);
            }
        }
    }
    Ok(paths)
}

fn field_changes(ops: &[EditOp], inverse: &[EditOp]) -> Vec<FieldChange> {
    let mut changes = Vec::with_capacity(ops.len());
    for (op, inv) in ops.iter().zip(inverse.iter().rev()) {
        let change = match (op, inv) {
            (EditOp::Replace { path, value: after }, EditOp::Replace { value: before, .. }) => {
                FieldChange {
                    path: path.clone(),
                    before: Some(before.clone()),
                    after: Some(after.clone()),
                }
            }
            (
                EditOp::Add { value: after, .. },
                EditOp::Remove {
                    path: concrete_path,
                },
            ) => FieldChange {
                path: concrete_path.clone(),
                before: None,
                after: Some(after.clone()),
            },
            (EditOp::Remove { path }, EditOp::Add { value: before, .. }) => FieldChange {
                path: path.clone(),
                before: Some(before.clone()),
                after: None,
            },
            (EditOp::Replace { path, value }, _) => FieldChange {
                path: path.clone(),
                before: None,
                after: Some(value.clone()),
            },
            (EditOp::Add { path, value }, _) => FieldChange {
                path: path.clone(),
                before: None,
                after: Some(value.clone()),
            },
            (EditOp::Remove { path }, _) => FieldChange {
                path: path.clone(),
                before: None,
                after: None,
            },
        };
        changes.push(change);
    }
    changes
}

pub(super) async fn history(
    unit: &mut Reading<'_>,
    registry: &EditRegistry,
    content: &ContentRef,
) -> Result<ContentEditHistory> {
    let kind = require_kind(registry, &content.kind)?;
    let loaded = kind
        .load(unit, &content.key)
        .await?
        .ok_or_else(|| AppError::NotFound(MISSING_ENTITY.into()))?;
    let current_revision = revision_of(&loaded.document);
    let rows = store::load_entity_edits(unit, content).await?;
    let standing_indices: HashSet<usize> = standing_edits(&rows).into_iter().collect();

    let mut reverted_by: HashMap<&str, &str> = HashMap::new();
    for row in &rows {
        if let Some(target_id) = row.reverts_edit_id.as_deref() {
            reverted_by.insert(target_id, row.id.as_str());
        }
    }

    let mut edits = Vec::with_capacity(rows.len());
    for (index, row) in rows.iter().enumerate().rev() {
        let standing = standing_indices.contains(&index);
        let rev_by = reverted_by.get(row.id.as_str()).map(|&id| id.to_owned());
        let unbroken = unbroken_from(&rows, index, &current_revision);
        let revertible =
            unbroken && rev_by.is_none() && (standing || row.after_revision == current_revision);
        let later = if revertible && standing {
            u32::try_from(standing_indices.iter().filter(|&&idx| idx > index).count())
                .unwrap_or(u32::MAX)
        } else {
            0
        };
        edits.push(ContentEditRecord {
            id: row.id.clone(),
            seq: row.seq,
            origin: row.origin.clone(),
            target: row.target.clone(),
            changes: field_changes(&row.ops, &row.inverse),
            created_at: row.created_at,
            reverts_edit_id: row.reverts_edit_id.clone(),
            reverted_by: rev_by,
            standing,
            revertible,
            later,
        });
    }

    Ok(ContentEditHistory {
        content: content.clone(),
        revision: current_revision,
        edits,
    })
}

pub(super) async fn list(
    unit: &mut Reading<'_>,
    query: &ContentEditQuery,
) -> Result<Vec<ContentEditSummary>> {
    let rows = store::query_edits(unit, query).await?;
    Ok(rows
        .into_iter()
        .map(|row| ContentEditSummary {
            id: row.id,
            content: ContentRef {
                kind: row.kind,
                key: row.content_key,
            },
            seq: row.seq,
            origin: row.origin,
            paths: extract_paths(&row.ops, &row.inverse),
            created_at: row.created_at,
            reverts_edit_id: row.reverts_edit_id,
        })
        .collect())
}

pub(super) async fn newest(
    unit: &mut Reading<'_>,
    registry: &EditRegistry,
    contents: &[ContentRef],
    reactor: &str,
) -> Result<Vec<NewestEdit>> {
    let mut found = Vec::new();
    for content in contents {
        let rows = store::load_entity_edits(unit, content).await?;
        let standing = standing_edits(&rows);
        let Some(&newest_idx) = standing.last() else {
            continue;
        };
        let Some(row) = rows.get(newest_idx) else {
            continue;
        };
        let kind = require_kind(registry, &content.kind)?;
        let (revision, unbroken) = match kind.load(unit, &content.key).await? {
            Some(loaded) => {
                let rev = revision_of(&loaded.document);
                let ok = unbroken_from(&rows, newest_idx, &rev);
                (rev, ok)
            }
            None => (String::new(), false),
        };
        let receipt = store::receipt_for_edit_and_reactor(unit, &row.id, reactor).await?;
        found.push(NewestEdit {
            content: content.clone(),
            edit_id: row.id.clone(),
            origin: row.origin.clone(),
            unbroken,
            revision,
            receipt,
        });
    }
    Ok(found)
}

pub(super) async fn edited_paths(
    unit: &mut Reading<'_>,
    registry: &EditRegistry,
    content: &ContentRef,
) -> Result<Vec<String>> {
    let kind = require_kind(registry, &content.kind)?;
    let Some(loaded) = kind.load(unit, &content.key).await? else {
        return Ok(Vec::new());
    };
    let rev = revision_of(&loaded.document);
    edited_paths_at(unit, content, &rev).await
}

pub(super) async fn marks(
    unit: &mut Reading<'_>,
    registry: &EditRegistry,
    contents: &[ContentRef],
) -> Result<Vec<ReadMark>> {
    let mut out = Vec::with_capacity(contents.len());
    for content in contents {
        let kind = require_kind(registry, &content.kind)?;
        let loaded = kind
            .load(unit, &content.key)
            .await?
            .ok_or_else(|| AppError::NotFound(MISSING_ENTITY.into()))?;
        out.push(ReadMark {
            content: content.clone(),
            revision: revision_of(&loaded.document),
        });
    }
    Ok(out)
}

pub(super) async fn moved(
    unit: &mut Reading<'_>,
    registry: &EditRegistry,
    marks: &[ReadMark],
) -> Result<Vec<ContentRef>> {
    let mut out = Vec::new();
    for mark in marks {
        let kind = require_kind(registry, &mark.content.kind)?;
        let changed = match kind.load(unit, &mark.content.key).await? {
            Some(loaded) => revision_of(&loaded.document) != mark.revision,
            None => true,
        };
        if changed {
            out.push(mark.content.clone());
        }
    }
    Ok(out)
}
