//! Applying validated content edits and extracting field paths and notices.

use serde_json::Value;

use super::super::kind::{Change, ContentKind};
use super::super::notices::markdown_notices;
use super::super::ops::apply_ops;
use super::super::registry::EditRegistry;
use super::super::revision::revision_of;
use super::super::rules::form_problems;
use super::super::store::{self, NewEditRow};
use super::super::types::{
    ContentEditOutcome, ContentEditRequest, EditOp, EditOrigin, Editability, FieldNotice,
    FieldSpec, TextFormat, Uncommitted,
};
use super::{
    now_ms, push_unique, request_digest, require_kind, CONFLICT_MESSAGE, MISSING_ENTITY,
};
use crate::shared::error::{AppError, Result};
use crate::shared::persistence::WriteTx;

fn list_path_of(path: &str) -> String {
    path.rsplit_once('/')
        .map(|(prefix, _)| prefix)
        .filter(|prefix| !prefix.is_empty())
        .unwrap_or(path)
        .to_owned()
}

pub(super) fn extract_paths(ops: &[EditOp], inverse: &[EditOp]) -> Vec<String> {
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

pub(super) fn extract_edited_field_paths(ops: &[EditOp]) -> Vec<String> {
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

pub(super) fn collect_notices(form: &[FieldSpec], document: &Value) -> Vec<FieldNotice> {
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
