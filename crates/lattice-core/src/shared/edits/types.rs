//! Wire and internal contracts for content addresses, forms, operations, previews, outcomes, and history.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::shared::text::selector::TextSelector;

/// Identifies one editable entity by its registered kind and feature-owned key.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ContentRef {
    pub kind: String,
    pub key: String,
}

/// Points to an entity, an optional field inside its document, and an optional text fragment.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ContentAddress {
    pub content: ContentRef,
    pub path: Option<String>,
    pub fragment: Option<TextSelector>,
}

/// Names the container an edit belongs to, used to filter history and invalidate frontend queries.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct EditScope {
    pub feature: String,
    pub container: String,
}

/// Display and normalization mode for a text field.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum TextFormat {
    Line,
    Plain,
    Markdown,
}

/// Describes one field in an entity's schema-driven editor form.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum FieldSpec {
    Text {
        path: String,
        label: String,
        format: TextFormat,
        required: bool,
    },
    Flag {
        path: String,
        label: String,
        exclusive: bool,
    },
    Choice {
        path: String,
        label: String,
        options: Vec<String>,
    },
    Number {
        path: String,
        label: String,
        min: i64,
        unit: String,
    },
    List {
        path: String,
        label: String,
        #[serde(rename = "itemLabel")]
        item_label: String,
        #[serde(rename = "minItems")]
        min_items: u32,
        #[serde(rename = "maxItems")]
        max_items: Option<u32>,
        item: Vec<FieldSpec>,
    },
}

/// A single patch operation against a loaded document.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum EditOp {
    Replace { path: String, value: Value },
    Add { path: String, value: Value },
    Remove { path: String },
}

/// Where a caller-initiated edit request comes from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum EditSource {
    Learner,
    Proposal {
        #[serde(rename = "proposalId")]
        proposal_id: String,
        model: String,
    },
    Action {
        name: String,
    },
}

/// Recorded origin of an edit row, including pipeline reverts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum EditOrigin {
    Learner,
    Proposal {
        #[serde(rename = "proposalId")]
        proposal_id: String,
        model: String,
    },
    Action {
        name: String,
    },
    Revert {
        #[serde(rename = "editId")]
        edit_id: String,
    },
}

impl From<EditSource> for EditOrigin {
    fn from(source: EditSource) -> Self {
        match source {
            EditSource::Learner => Self::Learner,
            EditSource::Proposal { proposal_id, model } => Self::Proposal { proposal_id, model },
            EditSource::Action { name } => Self::Action { name },
        }
    }
}

/// Whether an entity may currently be edited or must wait.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum Editability {
    Editable,
    NotNow { reason: String },
}

/// Loaded entity returned by `open_content_edit`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct EditableContent {
    pub content: ContentRef,
    pub label: String,
    pub scope: EditScope,
    pub editable: Editability,
    pub form: Vec<FieldSpec>,
    pub document: Value,
    pub revision: String,
    pub notes: Vec<String>,
    pub edited_paths: Vec<String>,
}

/// Request payload for `preview_content_edit` and `apply_content_edit`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ContentEditRequest {
    pub edit_id: String,
    pub content: ContentRef,
    pub expected_revision: String,
    pub ops: Vec<EditOp>,
    pub source: EditSource,
    pub target: Option<ContentAddress>,
}

/// Validation problem on a field or the whole document.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct FieldProblem {
    pub path: Option<String>,
    pub message: String,
}

/// Non-blocking Markdown structure notice on a field.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct FieldNotice {
    pub path: String,
    pub code: String,
    pub message: String,
    pub line: Option<u32>,
}

/// Another piece of content related to a reactor consequence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct RelatedContent {
    pub content: ContentRef,
    pub label: String,
}

/// Derived-data consequence surfaced on preview and save outcomes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct Consequence {
    pub code: String,
    pub message: String,
    pub related: Vec<RelatedContent>,
}

/// Dry-run output from `preview_content_edit`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ContentEditPreview {
    pub current: bool,
    pub changed: bool,
    pub problems: Vec<FieldProblem>,
    pub notices: Vec<FieldNotice>,
    pub consequences: Vec<Consequence>,
}

/// Result returned after `apply_content_edit` or `revert_content_edit`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ContentEditOutcome {
    pub edit_id: String,
    pub changed: bool,
    pub content: ContentRef,
    pub scope: EditScope,
    pub revision: String,
    pub affected: Vec<String>,
    pub consequences: Vec<Consequence>,
    pub reverted: Vec<String>,
}

/// Request payload for `revert_content_edit`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct RevertContentEditRequest {
    pub revert_id: String,
    pub edit_id: String,
    pub expected_revision: String,
    pub and_later: bool,
}

/// One changed field entry shown in an entity's edit history.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct FieldChange {
    pub path: String,
    pub before: Option<Value>,
    pub after: Option<Value>,
}

/// One edit log row returned in `ContentEditHistory`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ContentEditRecord {
    pub id: String,
    pub seq: i64,
    pub origin: EditOrigin,
    pub target: Option<ContentAddress>,
    pub changes: Vec<FieldChange>,
    pub created_at: i64,
    pub reverts_edit_id: Option<String>,
    pub reverted_by: Option<String>,
    pub standing: bool,
    pub revertible: bool,
    pub later: u32,
}

/// Full revision-ordered history for one content entity.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ContentEditHistory {
    pub content: ContentRef,
    pub revision: String,
    pub edits: Vec<ContentEditRecord>,
}

/// Filter parameters for `list_content_edits`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ContentEditQuery {
    pub scope: Option<EditScope>,
    pub contents: Vec<ContentRef>,
    pub since: Option<i64>,
}

/// Compact summary row returned by `list_content_edits`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ContentEditSummary {
    pub id: String,
    pub content: ContentRef,
    pub seq: i64,
    pub origin: EditOrigin,
    pub paths: Vec<String>,
    pub created_at: i64,
    pub reverts_edit_id: Option<String>,
}

/// Revision mark captured when a derived job reads existing content.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReadMark {
    pub content: ContentRef,
    pub revision: String,
}

/// Newest standing edit and reactor receipt for one entity.
#[derive(Debug, Clone, PartialEq)]
pub struct NewestEdit {
    pub content: ContentRef,
    pub edit_id: String,
    pub origin: EditOrigin,
    pub unbroken: bool,
    pub revision: String,
    pub receipt: Option<Value>,
}

/// In-transaction outcome paired with post-commit reactor follow-ups.
#[derive(Debug, Clone)]
pub struct Uncommitted {
    pub outcome: ContentEditOutcome,
    pub(crate) follow_ups: Vec<(String, Value)>,
}
