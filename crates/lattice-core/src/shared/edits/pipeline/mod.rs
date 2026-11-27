//! Core pipeline steps for opening, previewing, applying, reverting, and inspecting edits.

use std::sync::Arc;

use serde::Serialize;
use serde_json::Value;

use super::kind::{Change, ContentKind};
use super::ops::apply_ops_preview;
use super::registry::EditRegistry;
use super::revision::{canonical_json, revision_of};
use super::rules::form_problems;
use super::types::{
    ContentEditPreview, ContentEditRequest, ContentRef, EditOrigin, Editability, EditableContent,
};
use crate::shared::error::{AppError, Result};
use crate::shared::hash::sha256_hex;
use crate::shared::persistence::Reading;

mod apply;
mod history;
mod revert;

pub(super) use apply::apply_in;
pub(super) use history::{edited_paths, history, list, marks, moved, newest};
pub(super) use revert::revert_in;

const UNKNOWN_KIND: &str = "Nothing of that kind can be edited.";
const MISSING_ENTITY: &str = "That content no longer exists.";
const CONFLICT_MESSAGE: &str = "This changed since you opened it.";

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
    let edited_paths = history::edited_paths_at(unit, content, &revision).await?;
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
    let notices = apply::collect_notices(&loaded.form, &after);

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
