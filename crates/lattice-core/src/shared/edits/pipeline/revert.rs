//! Reverting content edits and tracking standing/unbroken edit chains.

use std::sync::Arc;

use serde_json::Value;

use super::super::kind::{Change, ContentKind, Loaded};
use super::super::ops::apply_ops;
use super::super::registry::EditRegistry;
use super::super::revision::revision_of;
use super::super::rules::form_problems;
use super::super::store::{self, NewEditRow, StoredEditRow};
use super::super::types::{
    Consequence, ContentEditOutcome, ContentRef, EditOrigin, Editability,
    RevertContentEditRequest, Uncommitted,
};
use super::{
    now_ms, push_unique, request_digest, require_kind, CONFLICT_MESSAGE, MISSING_ENTITY,
};
use crate::shared::error::{AppError, Result};
use crate::shared::persistence::WriteTx;

const REPLACED_MESSAGE: &str =
    "This was replaced after that change, so the change can't be undone.";
const ALREADY_UNDONE_MESSAGE: &str = "That change was already undone.";
const LATER_CHANGES_MESSAGE: &str =
    "Later changes were made to this. Undo them first, or undo them together.";
const CHANGED_AFTER_UNDO_MESSAGE: &str =
    "This changed after that was undone. Undo the later changes first, then redo it.";

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

pub(super) fn unbroken_from(rows: &[StoredEditRow], index: usize, current_revision: &str) -> bool {
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

struct RevertCtx<'a> {
    registry: &'a EditRegistry,
    kind: &'a Arc<dyn ContentKind>,
    app_version: &'a str,
    content: &'a ContentRef,
    loaded: &'a Loaded,
}

struct RevertState {
    doc: Value,
    rev: String,
    next_seq: i64,
    affected: Vec<String>,
    consequences: Vec<Consequence>,
    follow_ups: Vec<(String, Value)>,
}

struct RevertStep<'a> {
    new_edit_id: &'a str,
    target_row: &'a StoredEditRow,
    request_sha256: &'a str,
}

async fn revert_one(
    tx: &mut WriteTx,
    ctx: &RevertCtx<'_>,
    state: &mut RevertState,
    step: RevertStep<'_>,
) -> Result<()> {
    let reverted_receipts =
        store::receipts_for_edit(&mut tx.reading(), &step.target_row.id).await?;
    let (after, new_inverse) = apply_ops(
        &state.doc,
        &ctx.loaded.form,
        &step.target_row.inverse,
    )?;
    let mut problems = form_problems(&ctx.loaded.form, &after);
    problems.extend(
        ctx.kind
            .check(&mut tx.reading(), &ctx.content.key, &state.doc, &after)
            .await?,
    );
    if let Some(first) = problems.into_iter().next() {
        return Err(AppError::InvalidInput(first.message));
    }

    let after_revision = revision_of(&after);
    let origin = EditOrigin::Revert {
        edit_id: step.target_row.id.clone(),
    };
    let at = now_ms();
    let change = Change {
        edit_id: step.new_edit_id,
        content: ctx.content,
        scope: &ctx.loaded.scope,
        origin: &origin,
        target: None,
        context: None,
        ops: &step.target_row.inverse,
        before: &state.doc,
        after: &after,
        reverted_receipt: None,
        at,
    };

    ctx.kind.store(tx, &change).await?;
    let reloaded = ctx
        .kind
        .load(&mut tx.reading(), &ctx.content.key)
        .await?
        .ok_or_else(|| AppError::InvalidState("Content disappeared while reverting.".into()))?;
    if reloaded.document != after {
        return Err(AppError::InvalidState(
            "The stored content does not match the reverted edit.".into(),
        ));
    }

    let new_row = NewEditRow {
        id: step.new_edit_id,
        content: ctx.content,
        scope_feature: &ctx.loaded.scope.feature,
        scope_container: &ctx.loaded.scope.container,
        seq: state.next_seq,
        origin: &origin,
        target: None,
        ops: &step.target_row.inverse,
        inverse: &new_inverse,
        before_revision: &state.rev,
        after_revision: &after_revision,
        reverts_edit_id: Some(&step.target_row.id),
        request_sha256: step.request_sha256,
        app_version: ctx.app_version,
        created_at: at,
    };
    store::insert_edit(tx, &new_row).await?;
    state.next_seq += 1;

    for reactor in ctx.registry.reactors_for(&ctx.content.kind) {
        let reactor_change = Change {
            edit_id: step.new_edit_id,
            content: ctx.content,
            scope: &ctx.loaded.scope,
            origin: &origin,
            target: None,
            context: None,
            ops: &step.target_row.inverse,
            before: &state.doc,
            after: &after,
            reverted_receipt: reverted_receipts.get(reactor.name()),
            at,
        };
        let reaction = reactor.apply(tx, &reactor_change).await?;
        if let Some(receipt) = reaction.receipt.as_ref() {
            store::insert_receipt(tx, step.new_edit_id, reactor.name(), receipt).await?;
        }
        for name in reaction.affected {
            push_unique(&mut state.affected, name);
        }
        state.consequences.extend(reaction.consequences);
        if let Some(follow_up) = reaction.follow_up {
            state
                .follow_ups
                .push((reactor.name().to_owned(), follow_up));
        }
    }

    state.doc = after;
    state.rev = after_revision;
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

    let current_rev = revision_of(&loaded.document);
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

    let ctx = RevertCtx {
        registry,
        kind,
        app_version,
        content: &content,
        loaded: &loaded,
    };
    let mut state = RevertState {
        doc: loaded.document.clone(),
        rev: current_rev,
        next_seq: rows.last().map_or(1, |row| row.seq + 1),
        affected: Vec::new(),
        consequences: Vec::new(),
        follow_ups: Vec::new(),
    };
    for &name in kind.affects() {
        push_unique(&mut state.affected, name.to_owned());
    }
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
            &ctx,
            &mut state,
            RevertStep {
                new_edit_id: &new_edit_id,
                target_row: row,
                request_sha256: row_sha,
            },
        )
        .await?;
        reverted.push(row.id.clone());
    }

    let outcome = ContentEditOutcome {
        edit_id: request.revert_id.clone(),
        changed: true,
        content,
        scope: loaded.scope,
        revision: state.rev,
        affected: state.affected,
        consequences: state.consequences,
        reverted,
    };
    store::set_outcome(tx, &request.revert_id, &outcome).await?;
    Ok(Uncommitted {
        outcome,
        follow_ups: state.follow_ups,
    })
}
