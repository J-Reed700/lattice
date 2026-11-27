//! Core pipeline steps for opening, previewing, applying, reverting, and inspecting edits.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use serde::Serialize;
use serde_json::Value;

use super::kind::{Change, ContentKind, Loaded};
use super::notices::markdown_notices;
use super::ops::{apply_ops, apply_ops_preview};
use super::registry::EditRegistry;
use super::revision::{canonical_json, revision_of};
use super::rules::form_problems;
use super::store::{self, NewEditRow, StoredEditRow};
use super::types::{
    Consequence, ContentEditHistory, ContentEditOutcome, ContentEditPreview, ContentEditQuery,
    ContentEditRecord, ContentEditRequest, ContentEditSummary, ContentRef, EditOp, EditOrigin,
    Editability, EditableContent, FieldChange, FieldNotice, FieldSpec, NewestEdit, ReadMark,
    RevertContentEditRequest, TextFormat, Uncommitted,
};
use crate::shared::error::{AppError, Result};
use crate::shared::hash::sha256_hex;
use crate::shared::persistence::{Reading, WriteTx};

const UNKNOWN_KIND: &str = "Nothing of that kind can be edited.";
const MISSING_ENTITY: &str = "That content no longer exists.";
const CONFLICT_MESSAGE: &str = "This changed since you opened it.";
const REPLACED_MESSAGE: &str =
    "This was replaced after that change, so the change can't be undone.";
const ALREADY_UNDONE_MESSAGE: &str = "That change was already undone.";
const LATER_CHANGES_MESSAGE: &str =
    "Later changes were made to this. Undo them first, or undo them together.";
const CHANGED_AFTER_UNDO_MESSAGE: &str =
    "This changed after that was undone. Undo the later changes first, then redo it.";

fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

fn request_digest<T: Serialize>(request: &T) -> Result<String> {
    let value = serde_json::to_value(request)?;
    Ok(sha256_hex(canonical_json(&value)))
}

fn require_kind<'a>(registry: &'a EditRegistry, name: &str) -> Result<&'a Arc<dyn ContentKind>> {
    registry
        .kind(name)
        .ok_or_else(|| AppError::NotFound(UNKNOWN_KIND.into()))
}

fn push_unique(target: &mut Vec<String>, item: String) {
    if !target.contains(&item) {
        target.push(item);
    }
}

fn list_path_of(path: &str) -> String {
    path.rsplit_once('/')
        .map(|(prefix, _)| prefix)
        .filter(|prefix| !prefix.is_empty())
        .unwrap_or(path)
        .to_owned()
}

fn extract_paths(ops: &[EditOp], inverse: &[EditOp]) -> Vec<String> {
    let mut paths = Vec::new();
    for (op, inv) in ops.iter().zip(inverse.iter().rev()) {
        let path = match (op, inv) {
            (EditOp::Replace { path, .. }, _) => path.clone(),
            (EditOp::Add { path, .. }, EditOp::Remove { path: actual_path }) => {
                let _ = path;
                actual_path.clone()
            }
            (EditOp::Add { path, .. }, _) | (EditOp::Remove { path }, _) => path.clone(),
        };
        push_unique(&mut paths, path);
    }
    paths
}

fn extract_edited_field_paths(ops: &[EditOp]) -> Vec<String> {
    let mut paths = Vec::new();
    for op in ops {
        let path = match op {
            EditOp::Replace { path, .. } => path.clone(),
            EditOp::Add { path, .. } | EditOp::Remove { path } => list_path_of(path),
        };
        push_unique(&mut paths, path);
    }
    paths
}

fn collect_notices(form: &[FieldSpec], document: &Value) -> Vec<FieldNotice> {
    let mut notices = Vec::new();
    let Some(obj) = document.as_object() else {
        return notices;
    };
    for spec in form {
        match spec {
            FieldSpec::Text {
                path,
                format: TextFormat::Markdown,
                ..
            } => {
                if let Some(text) = path
                    .strip_prefix('/')
                    .and_then(|key| obj.get(key))
                    .and_then(Value::as_str)
                {
                    notices.extend(markdown_notices(path, text));
                }
            }
            FieldSpec::List { path, item, .. } => {
                let Some(members) = path
                    .strip_prefix('/')
                    .and_then(|key| obj.get(key))
                    .and_then(Value::as_array)
                else {
                    continue;
                };
                for (index, member) in members.iter().enumerate() {
                    let Some(member_obj) = member.as_object() else {
                        continue;
                    };
                    for item_spec in item {
                        if let FieldSpec::Text {
                            path: item_path,
                            format: TextFormat::Markdown,
                            ..
                        } = item_spec
                        {
                            if let Some(text) = item_path
                                .strip_prefix('/')
                                .and_then(|key| member_obj.get(key))
                                .and_then(Value::as_str)
                            {
                                let full_path = format!("{path}/{index}{item_path}");
                                notices.extend(markdown_notices(&full_path, text));
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }
    notices
}

pub(super) fn standing_edits(rows: &[StoredEditRow]) -> Vec<usize> {
    let mut stack: Vec<usize> = Vec::new();
    for (index, row) in rows.iter().enumerate() {
        let pops_top = row.reverts_edit_id.as_deref().is_some_and(|reverted_id| {
            stack
                .last()
                .and_then(|&top_idx| rows.get(top_idx))
                .is_some_and(|top| top.id == reverted_id)
        });
        if pops_top {
            stack.pop();
        } else {
            stack.push(index);
        }
    }
    stack
}

fn unbroken_from(rows: &[StoredEditRow], index: usize, current_revision: &str) -> bool {
    let Some(last) = rows.last() else {
        return false;
    };
    if last.after_revision != current_revision {
        return false;
    }
    let Some(tail) = rows.get(index..) else {
        return false;
    };
    tail.windows(2).all(|pair| {
        let (Some(prev), Some(next)) = (pair.first(), pair.get(1)) else {
            return false;
        };
        prev.after_revision == next.before_revision
    })
}

async fn edited_paths_at(
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

pub(super) async fn open_in(
    unit: &mut Reading<'_>,
    registry: &EditRegistry,
    content: &ContentRef,
) -> Result<EditableContent> {
    let kind = require_kind(registry, &content.kind)?;
    let loaded = kind
        .load(unit, &content.key)
        .await?
        .ok_or_else(|| AppError::NotFound(MISSING_ENTITY.into()))?;
    let revision = revision_of(&loaded.document);
    let edited_paths = edited_paths_at(unit, content, &revision).await?;
    Ok(EditableContent {
        content: content.clone(),
        label: loaded.label,
        scope: loaded.scope,
        editable: loaded.editability,
        form: loaded.form,
        document: loaded.document,
        revision,
        notes: loaded.notes,
        edited_paths,
    })
}

pub(super) async fn preview(
    unit: &mut Reading<'_>,
    registry: &EditRegistry,
    request: &ContentEditRequest,
) -> Result<ContentEditPreview> {
    let kind = require_kind(registry, &request.content.kind)?;
    let loaded = kind
        .load(unit, &request.content.key)
        .await?
        .ok_or_else(|| AppError::NotFound(MISSING_ENTITY.into()))?;
    if let Editability::NotNow { reason } = loaded.editability {
        return Err(AppError::InvalidState(reason));
    }
    let current_revision = revision_of(&loaded.document);
    if current_revision != request.expected_revision {
        return Ok(ContentEditPreview {
            current: false,
            changed: false,
            problems: Vec::new(),
            notices: Vec::new(),
            consequences: Vec::new(),
        });
    }

    let (after, mut problems) = apply_ops_preview(&loaded.document, &loaded.form, &request.ops);
    let ops_ok = problems.is_empty();
    let changed = after != loaded.document;
    problems.extend(form_problems(&loaded.form, &after));
    if ops_ok {
        problems.extend(
            kind.check(unit, &request.content.key, &loaded.document, &after)
                .await?,
        );
    }
    let notices = collect_notices(&loaded.form, &after);

    let mut consequences = Vec::new();
    if changed && problems.is_empty() {
        let origin = EditOrigin::from(request.source.clone());
        let change = Change {
            edit_id: &request.edit_id,
            content: &request.content,
            scope: &loaded.scope,
            origin: &origin,
            target: request.target.as_ref(),
            context: None,
            ops: &request.ops,
            before: &loaded.document,
            after: &after,
            reverted_receipt: None,
            at: now_ms(),
        };
        for reactor in registry.reactors_for(&request.content.kind) {
            consequences.extend(reactor.preview(unit, &change).await?);
        }
    }

    Ok(ContentEditPreview {
        current: true,
        changed,
        problems,
        notices,
        consequences,
    })
}

pub(super) async fn apply_in(
    tx: &mut WriteTx,
    registry: &EditRegistry,
    app_version: &str,
    request: &ContentEditRequest,
    context: Option<&Value>,
) -> Result<Uncommitted> {
    let request_sha256 = request_digest(request)?;
    if let Some(existing) = store::find_edit_by_id(&mut tx.reading(), &request.edit_id).await? {
        if existing.request_sha256 == request_sha256 {
            if let Some(outcome) = existing.outcome {
                return Ok(Uncommitted {
                    outcome,
                    follow_ups: Vec::new(),
                });
            }
        }
        return Err(AppError::InvalidInput(
            "This edit id was already used for another request.".into(),
        ));
    }

    let kind = require_kind(registry, &request.content.kind)?;
    let loaded = kind
        .load(&mut tx.reading(), &request.content.key)
        .await?
        .ok_or_else(|| AppError::NotFound(MISSING_ENTITY.into()))?;
    if let Editability::NotNow { reason } = loaded.editability {
        return Err(AppError::InvalidState(reason));
    }

    let before_revision = revision_of(&loaded.document);
    if before_revision != request.expected_revision {
        return Err(AppError::Conflict(CONFLICT_MESSAGE.into()));
    }

    let (after, inverse) = apply_ops(&loaded.document, &loaded.form, &request.ops)?;
    if after == loaded.document {
        return Ok(Uncommitted {
            outcome: ContentEditOutcome {
                edit_id: request.edit_id.clone(),
                changed: false,
                content: request.content.clone(),
                scope: loaded.scope,
                revision: before_revision,
                affected: Vec::new(),
                consequences: Vec::new(),
                reverted: Vec::new(),
            },
            follow_ups: Vec::new(),
        });
    }

    let mut problems = form_problems(&loaded.form, &after);
    problems.extend(
        kind.check(
            &mut tx.reading(),
            &request.content.key,
            &loaded.document,
            &after,
        )
        .await?,
    );
    if let Some(first) = problems.into_iter().next() {
        return Err(AppError::InvalidInput(first.message));
    }

    let after_revision = revision_of(&after);
    let origin = EditOrigin::from(request.source.clone());
    let at = now_ms();
    let change = Change {
        edit_id: &request.edit_id,
        content: &request.content,
        scope: &loaded.scope,
        origin: &origin,
        target: request.target.as_ref(),
        context,
        ops: &request.ops,
        before: &loaded.document,
        after: &after,
        reverted_receipt: None,
        at,
    };

    kind.store(tx, &change).await?;
    let reloaded = kind
        .load(&mut tx.reading(), &request.content.key)
        .await?
        .ok_or_else(|| AppError::InvalidState("Content disappeared while saving.".into()))?;
    if reloaded.document != after {
        return Err(AppError::InvalidState(
            "The stored content does not match the saved edit.".into(),
        ));
    }

    let seq = store::next_seq(&mut tx.reading(), &request.content).await?;
    let new_row = NewEditRow {
        id: &request.edit_id,
        content: &request.content,
        scope_feature: &loaded.scope.feature,
        scope_container: &loaded.scope.container,
        seq,
        origin: &origin,
        target: request.target.as_ref(),
        ops: &request.ops,
        inverse: &inverse,
        before_revision: &before_revision,
        after_revision: &after_revision,
        reverts_edit_id: None,
        request_sha256: &request_sha256,
        app_version,
        created_at: at,
    };
    store::insert_edit(tx, &new_row).await?;

    let mut affected = Vec::new();
    for &name in kind.affects() {
        push_unique(&mut affected, name.to_owned());
    }
    let mut consequences = Vec::new();
    let mut follow_ups = Vec::new();
    for reactor in registry.reactors_for(&request.content.kind) {
        let reaction = reactor.apply(tx, &change).await?;
        if let Some(receipt) = reaction.receipt.as_ref() {
            store::insert_receipt(tx, &request.edit_id, reactor.name(), receipt).await?;
        }
        for name in reaction.affected {
            push_unique(&mut affected, name);
        }
        consequences.extend(reaction.consequences);
        if let Some(follow_up) = reaction.follow_up {
            follow_ups.push((reactor.name().to_owned(), follow_up));
        }
    }

    let outcome = ContentEditOutcome {
        edit_id: request.edit_id.clone(),
        changed: true,
        content: request.content.clone(),
        scope: loaded.scope,
        revision: after_revision,
        affected,
        consequences,
        reverted: Vec::new(),
    };
    store::set_outcome(tx, &request.edit_id, &outcome).await?;
    Ok(Uncommitted {
        outcome,
        follow_ups,
    })
}

#[allow(clippy::too_many_arguments)]
async fn revert_one(
    tx: &mut WriteTx,
    registry: &EditRegistry,
    kind: &Arc<dyn ContentKind>,
    app_version: &str,
    content: &ContentRef,
    loaded: &Loaded,
    current_doc: &mut Value,
    current_rev: &mut String,
    next_seq: &mut i64,
    new_edit_id: &str,
    target_row: &StoredEditRow,
    request_sha256: &str,
    affected: &mut Vec<String>,
    consequences: &mut Vec<Consequence>,
    follow_ups: &mut Vec<(String, Value)>,
) -> Result<()> {
    let reverted_receipts = store::receipts_for_edit(&mut tx.reading(), &target_row.id).await?;
    let (after, new_inverse) = apply_ops(current_doc, &loaded.form, &target_row.inverse)?;
    let mut problems = form_problems(&loaded.form, &after);
    problems.extend(
        kind.check(&mut tx.reading(), &content.key, current_doc, &after)
            .await?,
    );
    if let Some(first) = problems.into_iter().next() {
        return Err(AppError::InvalidInput(first.message));
    }

    let after_revision = revision_of(&after);
    let origin = EditOrigin::Revert {
        edit_id: target_row.id.clone(),
    };
    let at = now_ms();
    let change = Change {
        edit_id: new_edit_id,
        content,
        scope: &loaded.scope,
        origin: &origin,
        target: None,
        context: None,
        ops: &target_row.inverse,
        before: current_doc,
        after: &after,
        reverted_receipt: None,
        at,
    };

    kind.store(tx, &change).await?;
    let reloaded = kind
        .load(&mut tx.reading(), &content.key)
        .await?
        .ok_or_else(|| AppError::InvalidState("Content disappeared while reverting.".into()))?;
    if reloaded.document != after {
        return Err(AppError::InvalidState(
            "The stored content does not match the reverted edit.".into(),
        ));
    }

    let new_row = NewEditRow {
        id: new_edit_id,
        content,
        scope_feature: &loaded.scope.feature,
        scope_container: &loaded.scope.container,
        seq: *next_seq,
        origin: &origin,
        target: None,
        ops: &target_row.inverse,
        inverse: &new_inverse,
        before_revision: current_rev,
        after_revision: &after_revision,
        reverts_edit_id: Some(&target_row.id),
        request_sha256,
        app_version,
        created_at: at,
    };
    store::insert_edit(tx, &new_row).await?;
    *next_seq += 1;

    for reactor in registry.reactors_for(&content.kind) {
        let reactor_change = Change {
            edit_id: new_edit_id,
            content,
            scope: &loaded.scope,
            origin: &origin,
            target: None,
            context: None,
            ops: &target_row.inverse,
            before: current_doc,
            after: &after,
            reverted_receipt: reverted_receipts.get(reactor.name()),
            at,
        };
        let reaction = reactor.apply(tx, &reactor_change).await?;
        if let Some(receipt) = reaction.receipt.as_ref() {
            store::insert_receipt(tx, new_edit_id, reactor.name(), receipt).await?;
        }
        for name in reaction.affected {
            push_unique(affected, name);
        }
        consequences.extend(reaction.consequences);
        if let Some(follow_up) = reaction.follow_up {
            follow_ups.push((reactor.name().to_owned(), follow_up));
        }
    }

    *current_doc = after;
    *current_rev = after_revision;
    Ok(())
}

pub(super) async fn revert_in(
    tx: &mut WriteTx,
    registry: &EditRegistry,
    app_version: &str,
    request: &RevertContentEditRequest,
) -> Result<Uncommitted> {
    let request_sha256 = request_digest(request)?;
    if let Some(existing) = store::find_edit_by_id(&mut tx.reading(), &request.revert_id).await? {
        if existing.request_sha256 == request_sha256 {
            if let Some(outcome) = existing.outcome {
                return Ok(Uncommitted {
                    outcome,
                    follow_ups: Vec::new(),
                });
            }
        }
        return Err(AppError::InvalidInput(
            "This revert id was already used for another request.".into(),
        ));
    }

    let target = store::find_edit_by_id(&mut tx.reading(), &request.edit_id)
        .await?
        .ok_or_else(|| AppError::NotFound("That edit no longer exists.".into()))?;
    let content = ContentRef {
        kind: target.kind.clone(),
        key: target.content_key.clone(),
    };
    let kind = require_kind(registry, &content.kind)?;
    let loaded = kind
        .load(&mut tx.reading(), &content.key)
        .await?
        .ok_or_else(|| AppError::NotFound(MISSING_ENTITY.into()))?;
    if let Editability::NotNow { reason } = &loaded.editability {
        return Err(AppError::InvalidState(reason.clone()));
    }

    let mut current_rev = revision_of(&loaded.document);
    if current_rev != request.expected_revision {
        return Err(AppError::Conflict(CONFLICT_MESSAGE.into()));
    }

    let rows = store::load_entity_edits(&mut tx.reading(), &content).await?;
    let Some(target_idx) = rows.iter().position(|row| row.id == target.id) else {
        return Err(AppError::NotFound("That edit no longer exists.".into()));
    };
    if rows
        .iter()
        .any(|row| row.reverts_edit_id.as_deref() == Some(target.id.as_str()))
    {
        return Err(AppError::InvalidState(ALREADY_UNDONE_MESSAGE.into()));
    }
    if !unbroken_from(&rows, target_idx, &current_rev) {
        return Err(AppError::InvalidState(REPLACED_MESSAGE.into()));
    }

    let standing = standing_edits(&rows);
    let to_revert: Vec<usize> = if standing.contains(&target_idx) {
        let later: Vec<usize> = standing
            .iter()
            .copied()
            .filter(|&idx| idx > target_idx)
            .rev()
            .collect();
        if !later.is_empty() && !request.and_later {
            return Err(AppError::InvalidState(LATER_CHANGES_MESSAGE.into()));
        }
        let mut order = later;
        order.push(target_idx);
        order
    } else {
        if target.after_revision != current_rev {
            return Err(AppError::InvalidState(CHANGED_AFTER_UNDO_MESSAGE.into()));
        }
        vec![target_idx]
    };

    let mut current_doc = loaded.document.clone();
    let mut next_seq = rows.last().map_or(1, |row| row.seq + 1);
    let mut affected = Vec::new();
    for &name in kind.affects() {
        push_unique(&mut affected, name.to_owned());
    }
    let mut consequences = Vec::new();
    let mut follow_ups = Vec::new();
    let mut reverted = Vec::with_capacity(to_revert.len());

    for (pos, idx) in to_revert.into_iter().enumerate() {
        let Some(row) = rows.get(idx) else {
            return Err(AppError::NotFound("That edit no longer exists.".into()));
        };
        let (new_edit_id, row_sha) = if pos == 0 {
            (request.revert_id.clone(), request_sha256.as_str())
        } else {
            (uuid::Uuid::new_v4().to_string(), "")
        };
        revert_one(
            tx,
            registry,
            kind,
            app_version,
            &content,
            &loaded,
            &mut current_doc,
            &mut current_rev,
            &mut next_seq,
            &new_edit_id,
            row,
            row_sha,
            &mut affected,
            &mut consequences,
            &mut follow_ups,
        )
        .await?;
        reverted.push(row.id.clone());
    }

    let outcome = ContentEditOutcome {
        edit_id: request.revert_id.clone(),
        changed: true,
        content,
        scope: loaded.scope,
        revision: current_rev,
        affected,
        consequences,
        reverted,
    };
    store::set_outcome(tx, &request.revert_id, &outcome).await?;
    Ok(Uncommitted {
        outcome,
        follow_ups,
    })
}

pub(super) async fn run_after_commit(registry: &EditRegistry, follow_ups: &[(String, Value)]) {
    for (reactor_name, value) in follow_ups {
        if let Some(reactor) = registry.reactor(reactor_name) {
            if let Err(err) = reactor.after_commit(value).await {
                tracing::warn!(
                    reactor = %reactor_name,
                    error = %err,
                    "edit reactor after_commit follow-up failed"
                );
            }
        }
    }
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
