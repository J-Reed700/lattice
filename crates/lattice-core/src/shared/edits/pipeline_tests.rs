use serde_json::json;

use super::fake::{note_ref, Harness, FAKE_AFFECTED};
use super::types::{
    Consequence, ContentEditQuery, EditOp, EditScope, EditSource, Editability, FieldProblem,
    RelatedContent,
};
use crate::shared::error::AppError;

fn replace_body(
    edit_id: &str,
    id: &str,
    rev: &str,
    body: &str,
) -> super::types::ContentEditRequest {
    super::types::ContentEditRequest {
        edit_id: edit_id.into(),
        content: note_ref(id),
        expected_revision: rev.into(),
        ops: vec![EditOp::Replace {
            path: "/body".into(),
            value: json!(body),
        }],
        source: EditSource::Learner,
        target: None,
    }
}

#[tokio::test]
async fn a_stale_revision_writes_nothing_and_is_a_conflict() {
    let h = Harness::new().await.unwrap();
    h.seed_note("n1", "Quillet", "Initial text.").await.unwrap();

    let opened = h.edits.open(&note_ref("n1")).await;
    assert!(opened.is_ok(), "{opened:?}");
    let _opened = opened.unwrap();

    let err = h
        .edits
        .apply(&replace_body("e1", "n1", "wrong-revision", "Changed."))
        .await;
    assert!(matches!(err, Err(AppError::Conflict(_))), "{err:?}");

    let row = h.note_row("n1").await.unwrap().unwrap();
    assert_eq!(row.body, "Initial text.");
    assert_eq!(row.title, "Quillet");
    assert!(!row.tags_json.is_empty());
    assert_eq!(row.level, 0);
    assert_eq!(row.count, 2);
    assert_eq!(h.counts().await.unwrap(), (0, 0));
}

#[tokio::test]
async fn a_reactor_that_fails_leaves_no_content_no_edit_and_no_receipt() {
    let h = Harness::new().await.unwrap();
    h.seed_note("n1", "Quillet", "Initial text.").await.unwrap();

    let opened = h.edits.open(&note_ref("n1")).await;
    assert!(opened.is_ok(), "{opened:?}");
    let rev = opened.unwrap().revision;

    h.reactor.set_fail_apply(Some("reactor broke".into()));
    let res = h
        .edits
        .apply(&replace_body("e1", "n1", &rev, "Changed."))
        .await;
    assert!(
        matches!(res, Err(AppError::InvalidState(ref msg)) if msg == "reactor broke"),
        "{res:?}"
    );

    assert_eq!(
        h.note_row("n1").await.unwrap().unwrap().body,
        "Initial text."
    );
    assert_eq!(h.counts().await.unwrap(), (0, 0));
}

#[tokio::test]
async fn a_kind_that_stores_other_than_it_was_given_fails_the_change() {
    let h = Harness::new().await.unwrap();
    h.seed_note("n1", "Quillet", "Initial text.").await.unwrap();

    let opened = h.edits.open(&note_ref("n1")).await;
    assert!(opened.is_ok(), "{opened:?}");
    let rev = opened.unwrap().revision;

    h.kind.set_corrupt_store(true);
    let res = h
        .edits
        .apply(&replace_body("e1", "n1", &rev, "Changed."))
        .await;
    assert!(matches!(res, Err(AppError::InvalidState(_))), "{res:?}");
    assert_eq!(
        h.note_row("n1").await.unwrap().unwrap().body,
        "Initial text."
    );
    assert_eq!(h.counts().await.unwrap(), (0, 0));
}

#[tokio::test]
async fn a_repeated_edit_id_writes_once_and_returns_the_same_outcome() {
    let h = Harness::new().await.unwrap();
    h.seed_note("n1", "Quillet", "Initial text.").await.unwrap();

    let opened = h.edits.open(&note_ref("n1")).await;
    assert!(opened.is_ok(), "{opened:?}");
    let rev = opened.unwrap().revision;

    let req = replace_body("e1", "n1", &rev, "Saved once.");
    let first = h.edits.apply(&req).await;
    assert!(first.is_ok(), "{first:?}");
    let first = first.unwrap();
    assert!(first.changed);
    assert!(first.affected.contains(&FAKE_AFFECTED.to_string()));

    let second = h.edits.apply(&req).await;
    assert!(second.is_ok(), "{second:?}");
    assert_eq!(second.unwrap(), first);
    assert_eq!(h.counts().await.unwrap(), (1, 1));
    assert_eq!(h.reactor.recorded().len(), 1);
}

#[tokio::test]
async fn an_edit_id_reused_for_another_request_is_refused() {
    let h = Harness::new().await.unwrap();
    h.seed_note("n1", "Quillet", "Initial text.").await.unwrap();

    let opened = h.edits.open(&note_ref("n1")).await;
    assert!(opened.is_ok(), "{opened:?}");
    let rev = opened.unwrap().revision;

    let first = h
        .edits
        .apply(&replace_body("same-id", "n1", &rev, "First body."))
        .await;
    assert!(first.is_ok(), "{first:?}");

    let second = h
        .edits
        .apply(&replace_body("same-id", "n1", &rev, "Different body."))
        .await;
    assert!(
        matches!(second, Err(AppError::InvalidInput(_))),
        "{second:?}"
    );
    assert_eq!(h.counts().await.unwrap(), (1, 1));
}

#[tokio::test]
async fn a_preview_writes_nothing_and_reports_every_problem() {
    let h = Harness::new().await.unwrap();
    h.seed_note("n1", "Quillet", "Initial text.").await.unwrap();

    let opened = h.edits.open(&note_ref("n1")).await;
    assert!(opened.is_ok(), "{opened:?}");
    let opened = opened.unwrap();
    assert_eq!(opened.editable, Editability::Editable);

    h.kind.set_extra_check(vec![FieldProblem {
        path: None,
        message: "Kind rule failed.".into(),
    }]);
    let bad_req = super::types::ContentEditRequest {
        edit_id: "p1".into(),
        content: note_ref("n1"),
        expected_revision: opened.revision.clone(),
        ops: vec![
            EditOp::Replace {
                path: "/title".into(),
                value: json!("   "),
            },
            EditOp::Replace {
                path: "/body".into(),
                value: json!("First line\n```rust\nunclosed fence"),
            },
            EditOp::Replace {
                path: "/tags/1/main".into(),
                value: json!(true),
            },
        ],
        source: EditSource::Learner,
        target: None,
    };
    let preview = h.edits.preview(&bad_req).await;
    assert!(preview.is_ok(), "{preview:?}");
    let preview = preview.unwrap();
    assert!(preview.current);
    assert!(preview.changed);
    assert!(
        preview.problems.len() >= 3,
        "expected problems from form + kind, got {:?}",
        preview.problems
    );
    assert_eq!(preview.notices.len(), 1);
    assert_eq!(preview.notices.first().unwrap().code, "open_code_fence");
    assert!(preview.consequences.is_empty());
    assert_eq!(h.counts().await.unwrap(), (0, 0));

    h.kind.set_extra_check(Vec::new());
    h.reactor.set_preview_consequences(vec![Consequence {
        code: "test.c".into(),
        message: "Derived updates.".into(),
        related: vec![RelatedContent {
            content: note_ref("n1"),
            label: "Note: Quillet".into(),
        }],
    }]);
    let good_preview = h
        .edits
        .preview(&replace_body(
            "p2",
            "n1",
            &opened.revision,
            "Clean new body.",
        ))
        .await;
    assert!(good_preview.is_ok(), "{good_preview:?}");
    let good_preview = good_preview.unwrap();
    assert!(good_preview.problems.is_empty());
    assert_eq!(good_preview.consequences.len(), 1);

    let stale_preview = h
        .edits
        .preview(&replace_body("p3", "n1", "old-rev", "Clean new body."))
        .await;
    assert!(stale_preview.is_ok(), "{stale_preview:?}");
    assert!(!stale_preview.unwrap().current);
}

#[tokio::test]
async fn a_follow_up_runs_after_the_commit_and_its_failure_does_not_fail_the_save() {
    let h = Harness::new().await.unwrap();
    h.seed_note("n1", "Quillet", "Initial text.").await.unwrap();

    let opened = h.edits.open(&note_ref("n1")).await;
    assert!(opened.is_ok(), "{opened:?}");
    let rev = opened.unwrap().revision;

    h.reactor
        .set_follow_up(Some(json!({ "recompute": true })), true);
    let saved = h
        .edits
        .apply(&replace_body("e1", "n1", &rev, "New text."))
        .await;
    assert!(saved.is_ok(), "{saved:?}");
    assert_eq!(
        h.reactor.after_commits(),
        vec![json!({ "recompute": true })]
    );

    let history = h.edits.history(&note_ref("n1")).await;
    assert!(history.is_ok(), "{history:?}");
    assert_eq!(history.unwrap().edits.len(), 1);
}

#[tokio::test]
async fn not_now_content_opens_and_refuses_preview_and_apply_and_apply_in_passes_context() {
    let h = Harness::new().await.unwrap();
    h.seed_note("n1", "Quillet", "Initial text.").await.unwrap();

    let opened = h.edits.open(&note_ref("n1")).await;
    assert!(opened.is_ok(), "{opened:?}");
    let rev = opened.unwrap().revision;

    let noop = h
        .edits
        .apply(&replace_body("e0", "n1", &rev, "Initial text."))
        .await;
    assert!(noop.is_ok(), "{noop:?}");
    assert!(!noop.unwrap().changed);
    assert_eq!(h.counts().await.unwrap(), (0, 0));

    h.kind.set_not_now(Some("Busy right now.".into()));
    let locked = h.edits.open(&note_ref("n1")).await;
    assert!(locked.is_ok(), "{locked:?}");
    assert_eq!(
        locked.unwrap().editable,
        Editability::NotNow {
            reason: "Busy right now.".into()
        }
    );
    let p_err = h
        .edits
        .preview(&replace_body("p-lock", "n1", &rev, "Changed."))
        .await;
    assert!(
        matches!(p_err, Err(AppError::InvalidState(ref m)) if m == "Busy right now."),
        "{p_err:?}"
    );
    let a_err = h
        .edits
        .apply(&replace_body("a-lock", "n1", &rev, "Changed."))
        .await;
    assert!(
        matches!(a_err, Err(AppError::InvalidState(ref m)) if m == "Busy right now."),
        "{a_err:?}"
    );

    h.kind.set_not_now(None);
    let mut tx = h.database.urgent().begin_write().await.unwrap();
    let ctx = json!({ "action": "test" });
    let pending = h
        .edits
        .apply_in(
            &mut tx,
            &replace_body("e1", "n1", &rev, "With ctx."),
            Some(&ctx),
        )
        .await;
    assert!(pending.is_ok(), "{pending:?}");
    let pending = pending.unwrap();
    tx.commit().await.unwrap();
    let out = h.edits.committed(pending).await;
    assert!(out.changed);
    assert_eq!(
        h.reactor.recorded().first().and_then(|r| r.context.clone()),
        Some(ctx)
    );

    let listed = h
        .edits
        .list(&ContentEditQuery {
            scope: Some(EditScope {
                feature: "test".into(),
                container: "notebook-1".into(),
            }),
            contents: vec![note_ref("n1")],
            since: Some(0),
        })
        .await;
    assert!(listed.is_ok(), "{listed:?}");
    assert_eq!(listed.unwrap().len(), 1);
}
