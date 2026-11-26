use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::{json, Value};

use super::types::{
    Consequence, ContentAddress, ContentEditHistory, ContentEditOutcome, ContentEditPreview,
    ContentEditQuery, ContentEditRecord, ContentEditRequest, ContentEditSummary, ContentRef,
    EditOp, EditOrigin, EditScope, EditSource, Editability, EditableContent, FieldChange,
    FieldNotice, FieldProblem, FieldSpec, RelatedContent, RevertContentEditRequest, TextFormat,
};
use crate::shared::text::selector::TextSelector;

fn round_trip<T>(value: &T, expected: Value)
where
    T: Serialize + DeserializeOwned + PartialEq + std::fmt::Debug,
{
    let serialized = serde_json::to_value(value).unwrap();
    assert_eq!(serialized, expected);
    let parsed: T = serde_json::from_value(expected).unwrap();
    assert_eq!(&parsed, value);
}

#[test]
fn content_address_and_scope_wire_shapes_match_spec() {
    let content = ContentRef {
        kind: "learning.section".into(),
        key: "lesson-1/0".into(),
    };
    round_trip(
        &content,
        json!({ "kind": "learning.section", "key": "lesson-1/0" }),
    );

    let selector = TextSelector {
        exact: "Quillet".into(),
        prefix: Some("The ".into()),
        suffix: Some(" Guide".into()),
        start_byte: 4,
        end_byte: 11,
    };
    round_trip(
        &selector,
        json!({
            "exact": "Quillet",
            "prefix": "The ",
            "suffix": " Guide",
            "startByte": 4,
            "endByte": 11
        }),
    );

    let address = ContentAddress {
        content,
        path: Some("/body".into()),
        fragment: Some(selector),
    };
    round_trip(
        &address,
        json!({
            "content": { "kind": "learning.section", "key": "lesson-1/0" },
            "path": "/body",
            "fragment": {
                "exact": "Quillet",
                "prefix": "The ",
                "suffix": " Guide",
                "startByte": 4,
                "endByte": 11
            }
        }),
    );

    let scope = EditScope {
        feature: "learning".into(),
        container: "program-1".into(),
    };
    round_trip(
        &scope,
        json!({ "feature": "learning", "container": "program-1" }),
    );
}

#[test]
fn field_spec_and_text_format_variants_match_spec() {
    for (format, wire) in [
        (TextFormat::Line, json!("line")),
        (TextFormat::Plain, json!("plain")),
        (TextFormat::Markdown, json!("markdown")),
    ] {
        round_trip(&format, wire);
    }

    let specs = [
        (
            FieldSpec::Text {
                path: "/title".into(),
                label: "Title".into(),
                format: TextFormat::Line,
                required: true,
            },
            json!({
                "type": "text",
                "path": "/title",
                "label": "Title",
                "format": "line",
                "required": true
            }),
        ),
        (
            FieldSpec::Flag {
                path: "/correct".into(),
                label: "Right answer".into(),
                exclusive: true,
            },
            json!({
                "type": "flag",
                "path": "/correct",
                "label": "Right answer",
                "exclusive": true
            }),
        ),
        (
            FieldSpec::Choice {
                path: "/answer".into(),
                label: "Right answer".into(),
                options: vec!["True".into(), "False".into()],
            },
            json!({
                "type": "choice",
                "path": "/answer",
                "label": "Right answer",
                "options": ["True", "False"]
            }),
        ),
        (
            FieldSpec::Number {
                path: "/estimatedMinutes".into(),
                label: "Time".into(),
                min: 1,
                unit: "minutes".into(),
            },
            json!({
                "type": "number",
                "path": "/estimatedMinutes",
                "label": "Time",
                "min": 1,
                "unit": "minutes"
            }),
        ),
        (
            FieldSpec::List {
                path: "/choices".into(),
                label: "Choices".into(),
                item_label: "Choice".into(),
                min_items: 2,
                max_items: Some(8),
                item: vec![FieldSpec::Text {
                    path: "/text".into(),
                    label: "Choice".into(),
                    format: TextFormat::Plain,
                    required: true,
                }],
            },
            json!({
                "type": "list",
                "path": "/choices",
                "label": "Choices",
                "itemLabel": "Choice",
                "minItems": 2,
                "maxItems": 8,
                "item": [{
                    "type": "text",
                    "path": "/text",
                    "label": "Choice",
                    "format": "plain",
                    "required": true
                }]
            }),
        ),
    ];

    for (spec, wire) in specs {
        round_trip(&spec, wire);
    }
}

#[test]
fn ops_sources_origins_and_editability_variants_match_spec() {
    for (op, wire) in [
        (
            EditOp::Replace {
                path: "/title".into(),
                value: json!("The Quillet Guide"),
            },
            json!({ "op": "replace", "path": "/title", "value": "The Quillet Guide" }),
        ),
        (
            EditOp::Add {
                path: "/choices/-".into(),
                value: json!({ "text": "Opt C" }),
            },
            json!({ "op": "add", "path": "/choices/-", "value": { "text": "Opt C" } }),
        ),
        (
            EditOp::Remove {
                path: "/choices/1".into(),
            },
            json!({ "op": "remove", "path": "/choices/1" }),
        ),
    ] {
        round_trip(&op, wire);
    }

    for (source, wire) in [
        (EditSource::Learner, json!({ "type": "learner" })),
        (
            EditSource::Proposal {
                proposal_id: "prop-1".into(),
                model: "qwen".into(),
            },
            json!({ "type": "proposal", "proposalId": "prop-1", "model": "qwen" }),
        ),
        (
            EditSource::Action {
                name: "claim.remove".into(),
            },
            json!({ "type": "action", "name": "claim.remove" }),
        ),
    ] {
        round_trip(&source, wire);
    }

    for (origin, wire) in [
        (EditOrigin::Learner, json!({ "type": "learner" })),
        (
            EditOrigin::Proposal {
                proposal_id: "prop-1".into(),
                model: "qwen".into(),
            },
            json!({ "type": "proposal", "proposalId": "prop-1", "model": "qwen" }),
        ),
        (
            EditOrigin::Action {
                name: "claim.edit".into(),
            },
            json!({ "type": "action", "name": "claim.edit" }),
        ),
        (
            EditOrigin::Revert {
                edit_id: "edit-1".into(),
            },
            json!({ "type": "revert", "editId": "edit-1" }),
        ),
    ] {
        round_trip(&origin, wire);
    }

    assert_eq!(EditOrigin::from(EditSource::Learner), EditOrigin::Learner);

    for (editability, wire) in [
        (Editability::Editable, json!({ "state": "editable" })),
        (
            Editability::NotNow {
                reason: "Wait until preparation finishes.".into(),
            },
            json!({ "state": "not_now", "reason": "Wait until preparation finishes." }),
        ),
    ] {
        round_trip(&editability, wire);
    }
}

#[test]
fn request_preview_outcome_and_history_payloads_match_spec() {
    let content = ContentRef {
        kind: "learning.section".into(),
        key: "lesson-1/0".into(),
    };
    let scope = EditScope {
        feature: "learning".into(),
        container: "program-1".into(),
    };

    round_trip(
        &EditableContent {
            content: content.clone(),
            label: "Section 1: Quillet".into(),
            scope: scope.clone(),
            editable: Editability::Editable,
            form: vec![],
            document: json!({ "title": "Quillet" }),
            revision: "rev-1".into(),
            notes: vec!["Note".into()],
            edited_paths: vec!["/title".into()],
        },
        json!({
            "content": { "kind": "learning.section", "key": "lesson-1/0" },
            "label": "Section 1: Quillet",
            "scope": { "feature": "learning", "container": "program-1" },
            "editable": { "state": "editable" },
            "form": [],
            "document": { "title": "Quillet" },
            "revision": "rev-1",
            "notes": ["Note"],
            "editedPaths": ["/title"]
        }),
    );

    round_trip(
        &ContentEditRequest {
            edit_id: "edit-1".into(),
            content: content.clone(),
            expected_revision: "rev-1".into(),
            ops: vec![],
            source: EditSource::Learner,
            target: None,
        },
        json!({
            "editId": "edit-1",
            "content": { "kind": "learning.section", "key": "lesson-1/0" },
            "expectedRevision": "rev-1",
            "ops": [],
            "source": { "type": "learner" },
            "target": null
        }),
    );

    round_trip(
        &ContentEditPreview {
            current: true,
            changed: true,
            problems: vec![FieldProblem {
                path: Some("/title".into()),
                message: "Title can't be empty.".into(),
            }],
            notices: vec![FieldNotice {
                path: "/body".into(),
                code: "open_code_fence".into(),
                message: "Fence unclosed.".into(),
                line: Some(3),
            }],
            consequences: vec![Consequence {
                code: "quiz_bank.quote_removed".into(),
                message: "1 quiz question affected.".into(),
                related: vec![RelatedContent {
                    content: ContentRef {
                        kind: "learning.question".into(),
                        key: "cand-1".into(),
                    },
                    label: "Question: One".into(),
                }],
            }],
        },
        json!({
            "current": true,
            "changed": true,
            "problems": [{ "path": "/title", "message": "Title can't be empty." }],
            "notices": [{
                "path": "/body",
                "code": "open_code_fence",
                "message": "Fence unclosed.",
                "line": 3
            }],
            "consequences": [{
                "code": "quiz_bank.quote_removed",
                "message": "1 quiz question affected.",
                "related": [{
                    "content": { "kind": "learning.question", "key": "cand-1" },
                    "label": "Question: One"
                }]
            }]
        }),
    );

    round_trip(
        &ContentEditOutcome {
            edit_id: "edit-1".into(),
            changed: true,
            content: content.clone(),
            scope: scope.clone(),
            revision: "rev-2".into(),
            affected: vec!["learning.program".into()],
            consequences: vec![],
            reverted: vec!["edit-0".into()],
        },
        json!({
            "editId": "edit-1",
            "changed": true,
            "content": { "kind": "learning.section", "key": "lesson-1/0" },
            "scope": { "feature": "learning", "container": "program-1" },
            "revision": "rev-2",
            "affected": ["learning.program"],
            "consequences": [],
            "reverted": ["edit-0"]
        }),
    );

    round_trip(
        &RevertContentEditRequest {
            revert_id: "rev-req-1".into(),
            edit_id: "edit-1".into(),
            expected_revision: "rev-2".into(),
            and_later: false,
        },
        json!({
            "revertId": "rev-req-1",
            "editId": "edit-1",
            "expectedRevision": "rev-2",
            "andLater": false
        }),
    );

    round_trip(
        &ContentEditHistory {
            content: content.clone(),
            revision: "rev-2".into(),
            edits: vec![ContentEditRecord {
                id: "edit-1".into(),
                seq: 1,
                origin: EditOrigin::Learner,
                target: None,
                changes: vec![FieldChange {
                    path: "/title".into(),
                    before: Some(json!("Old")),
                    after: Some(json!("New")),
                }],
                created_at: 1_760_000_000_000,
                reverts_edit_id: None,
                reverted_by: None,
                standing: true,
                revertible: true,
                later: 0,
            }],
        },
        json!({
            "content": { "kind": "learning.section", "key": "lesson-1/0" },
            "revision": "rev-2",
            "edits": [{
                "id": "edit-1",
                "seq": 1,
                "origin": { "type": "learner" },
                "target": null,
                "changes": [{ "path": "/title", "before": "Old", "after": "New" }],
                "createdAt": 1_760_000_000_000_i64,
                "revertsEditId": null,
                "revertedBy": null,
                "standing": true,
                "revertible": true,
                "later": 0
            }]
        }),
    );

    round_trip(
        &ContentEditQuery {
            scope: Some(scope),
            contents: vec![content.clone()],
            since: Some(1_760_000_000_000),
        },
        json!({
            "scope": { "feature": "learning", "container": "program-1" },
            "contents": [{ "kind": "learning.section", "key": "lesson-1/0" }],
            "since": 1_760_000_000_000_i64
        }),
    );

    round_trip(
        &ContentEditSummary {
            id: "edit-1".into(),
            content,
            seq: 1,
            origin: EditOrigin::Learner,
            paths: vec!["/title".into()],
            created_at: 1_760_000_000_000,
            reverts_edit_id: None,
        },
        json!({
            "id": "edit-1",
            "content": { "kind": "learning.section", "key": "lesson-1/0" },
            "seq": 1,
            "origin": { "type": "learner" },
            "paths": ["/title"],
            "createdAt": 1_760_000_000_000_i64,
            "revertsEditId": null
        }),
    );
}
