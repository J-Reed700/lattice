use serde_json::{json, Value};

use super::fake::{note_ref, Harness, FAKE_REACTOR};
use super::types::{ContentEditRequest, EditOp, EditOrigin, EditSource, RevertContentEditRequest};
use crate::shared::error::AppError;

fn req(edit_id: &str, rev: &str, body: &str) -> ContentEditRequest {
    ContentEditRequest {
        edit_id: edit_id.into(),
        content: note_ref("n1"),
        expected_revision: rev.into(),
        ops: vec![EditOp::Replace {
            path: "/body".into(),
            value: json!(body),
        }],
        source: EditSource::Learner,
        target: None,
    }
}

fn revert_req(
    revert_id: &str,
    edit_id: &str,
    rev: &str,
    and_later: bool,
) -> RevertContentEditRequest {
    RevertContentEditRequest {
        revert_id: revert_id.into(),
        edit_id: edit_id.into(),
        expected_revision: rev.into(),
        and_later,
    }
}

#[tokio::test]
async fn a_revert_restores_the_bytes_and_hands_each_reactor_its_receipt() {
    let h = Harness::new().await.unwrap();
    h.seed_note("n1", "Quillet", "Initial text.").await.unwrap();

    let opened = h.edits.open(&note_ref("n1")).await;
    assert!(opened.is_ok(), "{opened:?}");
    let rev0 = opened.unwrap().revision;

    let e1 = h.edits.apply(&req("e1", &rev0, "First edit.")).await;
    assert!(e1.is_ok(), "{e1:?}");
    let rev1 = e1.unwrap().revision;

    let reverted = h.edits.revert(&revert_req("r1", "e1", &rev1, false)).await;
    assert!(reverted.is_ok(), "{reverted:?}");
    let reverted = reverted.unwrap();
    assert_eq!(reverted.revision, rev0);
    assert_eq!(reverted.reverted, vec!["e1".to_string()]);
    assert_eq!(
        h.note_row("n1").await.unwrap().unwrap().body,
        "Initial text."
    );

    let recorded = h.reactor.recorded();
    let revert_call = recorded.last().unwrap();
    assert_eq!(revert_call.edit_id, "r1");
    assert_eq!(
        revert_call.origin,
        EditOrigin::Revert {
            edit_id: "e1".into()
        }
    );
    assert_eq!(
        revert_call.reverted_receipt,
        Some(json!({ "editId": "e1", "marker": "r-e1" }))
    );
    assert_eq!(
        revert_call.before,
        json!({
            "title": "Quillet",
            "body": "First edit.",
            "tags": [{ "text": "guide", "main": true }, { "text": "reference", "main": false }],
            "level": 0,
            "count": 2
        })
    );
    assert_eq!(
        revert_call.after.get("body").and_then(Value::as_str),
        Some("Initial text.")
    );
    assert_eq!(revert_call.ops.len(), 1);
}

#[tokio::test]
async fn a_revert_with_later_edits_is_refused_unless_they_go_too() {
    let h = Harness::new().await.unwrap();
    h.seed_note("n1", "Quillet", "V0").await.unwrap();

    let opened = h.edits.open(&note_ref("n1")).await;
    assert!(opened.is_ok(), "{opened:?}");
    let rev0 = opened.unwrap().revision;

    let rev1 = h
        .edits
        .apply(&req("e1", &rev0, "V1"))
        .await
        .unwrap()
        .revision;
    let rev2 = h
        .edits
        .apply(&req("e2", &rev1, "V2"))
        .await
        .unwrap()
        .revision;

    let refused = h
        .edits
        .revert(&revert_req("r-early", "e1", &rev2, false))
        .await;
    assert!(
        matches!(
            refused,
            Err(AppError::InvalidState(ref msg))
                if msg == "Later changes were made to this. Undo them first, or undo them together."
        ),
        "{refused:?}"
    );

    let together = h
        .edits
        .revert(&revert_req("r-both", "e1", &rev2, true))
        .await;
    assert!(together.is_ok(), "{together:?}");
    let together = together.unwrap();
    assert_eq!(together.revision, rev0);
    assert_eq!(together.reverted, vec!["e2".to_string(), "e1".to_string()]);
    assert_eq!(h.note_row("n1").await.unwrap().unwrap().body, "V0");

    let again = h
        .edits
        .revert(&revert_req("r-again", "e1", &rev0, false))
        .await;
    assert!(
        matches!(
            again,
            Err(AppError::InvalidState(ref msg)) if msg == "That change was already undone."
        ),
        "{again:?}"
    );
}

#[tokio::test]
async fn content_replaced_since_is_not_revertible() {
    let h = Harness::new().await.unwrap();
    h.seed_note("n1", "Quillet", "V0").await.unwrap();

    let opened = h.edits.open(&note_ref("n1")).await;
    assert!(opened.is_ok(), "{opened:?}");
    let rev0 = opened.unwrap().revision;

    let _rev1 = h
        .edits
        .apply(&req("e1", &rev0, "V1"))
        .await
        .unwrap()
        .revision;

    sqlx::query("UPDATE fake_notes SET body = 'Replaced externally' WHERE id = 'n1'")
        .execute(&h.pool)
        .await
        .unwrap();

    let current_rev = h.edits.open(&note_ref("n1")).await.unwrap().revision;
    let res = h
        .edits
        .revert(&revert_req("r1", "e1", &current_rev, false))
        .await;
    assert!(
        matches!(
            res,
            Err(AppError::InvalidState(ref msg))
                if msg == "This was replaced after that change, so the change can't be undone."
        ),
        "{res:?}"
    );

    let hist = h.edits.history(&note_ref("n1")).await.unwrap();
    assert_eq!(hist.edits.len(), 1);
    assert!(!hist.edits.first().unwrap().revertible);
}

#[tokio::test]
async fn an_edit_already_undone_is_not_reverted_again_with_an_earlier_one() {
    let h = Harness::new().await.unwrap();
    h.seed_note("n1", "Quillet", "V0").await.unwrap();

    let opened = h.edits.open(&note_ref("n1")).await;
    assert!(opened.is_ok(), "{opened:?}");
    let rev0 = opened.unwrap().revision;

    let rev1 = h
        .edits
        .apply(&req("A", &rev0, "V1"))
        .await
        .unwrap()
        .revision;
    let rev2 = h
        .edits
        .apply(&req("B", &rev1, "V2"))
        .await
        .unwrap()
        .revision;
    let rev_rb = h
        .edits
        .revert(&revert_req("R_B", "B", &rev2, false))
        .await
        .unwrap()
        .revision;

    let mut read = h.database.begin_read().await.unwrap();
    let newest = h
        .edits
        .newest(&mut read.reading(), &[note_ref("n1")], FAKE_REACTOR)
        .await
        .unwrap();
    assert_eq!(newest.len(), 1);
    assert_eq!(newest.first().unwrap().edit_id, "A");
    read.commit().await.unwrap();

    let before_count = h.reactor.recorded().len();
    let res = h
        .edits
        .revert(&revert_req("R_A", "A", &rev_rb, false))
        .await;
    assert!(res.is_ok(), "{res:?}");
    let outcome = res.unwrap();
    assert_eq!(outcome.revision, rev0);
    assert_eq!(outcome.reverted, vec!["A".to_string()]);
    assert_eq!(h.note_row("n1").await.unwrap().unwrap().body, "V0");

    let new_calls = &h.reactor.recorded()[before_count..];
    assert_eq!(new_calls.len(), 1);
    assert_eq!(
        new_calls.first().unwrap().reverted_receipt,
        Some(json!({ "editId": "A", "marker": "r-A" }))
    );

    let rev_redo = h
        .edits
        .apply(&req("C", &outcome.revision, "V-redo"))
        .await
        .unwrap()
        .revision;
    let mut tx = h.database.urgent().begin_write().await.unwrap();
    let pending = h
        .edits
        .revert_in(&mut tx, &revert_req("R_C", "C", &rev_redo, false))
        .await;
    assert!(pending.is_ok(), "{pending:?}");
    let pending = pending.unwrap();
    tx.commit().await.unwrap();
    let reverted_in = h.edits.committed(pending).await;
    assert_eq!(reverted_in.revision, outcome.revision);
}

#[tokio::test]
async fn undoing_an_undo_applies_the_change_again() {
    let h = Harness::new().await.unwrap();
    h.seed_note("n1", "Quillet", "V0").await.unwrap();

    let opened = h.edits.open(&note_ref("n1")).await;
    assert!(opened.is_ok(), "{opened:?}");
    let rev0 = opened.unwrap().revision;

    let rev1 = h
        .edits
        .apply(&req("A", &rev0, "V1"))
        .await
        .unwrap()
        .revision;
    let rev2 = h
        .edits
        .apply(&req("B", &rev1, "V2"))
        .await
        .unwrap()
        .revision;
    let rev_rb = h
        .edits
        .revert(&revert_req("R_B", "B", &rev2, false))
        .await
        .unwrap()
        .revision;

    let redone = h
        .edits
        .revert(&revert_req("R_RB", "R_B", &rev_rb, false))
        .await;
    assert!(redone.is_ok(), "{redone:?}");
    let rev_after = redone.unwrap().revision;
    assert_eq!(rev_after, rev2);
    assert_eq!(h.note_row("n1").await.unwrap().unwrap().body, "V2");

    let hist = h.edits.history(&note_ref("n1")).await.unwrap();
    let row_b = hist.edits.iter().find(|r| r.id == "B").unwrap();
    let row_rb = hist.edits.iter().find(|r| r.id == "R_B").unwrap();
    let row_rrb = hist.edits.iter().find(|r| r.id == "R_RB").unwrap();

    assert!(!row_b.revertible);
    assert!(!row_rb.standing);
    assert!(!row_rb.revertible);
    assert!(row_rrb.standing);
    assert!(row_rrb.revertible);
}

#[tokio::test]
async fn a_redo_never_takes_a_later_edit_with_it() {
    let h = Harness::new().await.unwrap();
    h.seed_note("n1", "Quillet", "V0").await.unwrap();

    let opened = h.edits.open(&note_ref("n1")).await;
    assert!(opened.is_ok(), "{opened:?}");
    let rev0 = opened.unwrap().revision;

    let rev1 = h
        .edits
        .apply(&req("A", &rev0, "V1"))
        .await
        .unwrap()
        .revision;
    let rev2 = h
        .edits
        .apply(&req("B", &rev1, "V2"))
        .await
        .unwrap()
        .revision;
    let rev_rb = h
        .edits
        .revert(&revert_req("R_B", "B", &rev2, false))
        .await
        .unwrap()
        .revision;
    let rev_c = h
        .edits
        .apply(&req("C", &rev_rb, "V3"))
        .await
        .unwrap()
        .revision;

    let expected_msg =
        "This changed after that was undone. Undo the later changes first, then redo it.";
    for and_later in [true, false] {
        let err = h
            .edits
            .revert(&revert_req("try-redo", "R_B", &rev_c, and_later))
            .await;
        assert!(
            matches!(err, Err(AppError::InvalidState(ref msg)) if msg == expected_msg),
            "{err:?}"
        );
    }
    assert_eq!(h.note_row("n1").await.unwrap().unwrap().body, "V3");

    let hist = h.edits.history(&note_ref("n1")).await.unwrap();
    let row_rb = hist.edits.iter().find(|r| r.id == "R_B").unwrap();
    assert!(!row_rb.revertible);

    let rev_rc = h
        .edits
        .revert(&revert_req("R_C", "C", &rev_c, false))
        .await
        .unwrap()
        .revision;
    let hist_after_rc = h.edits.history(&note_ref("n1")).await.unwrap();
    let row_rb = hist_after_rc.edits.iter().find(|r| r.id == "R_B").unwrap();
    assert!(row_rb.revertible);

    let redone = h
        .edits
        .revert(&revert_req("redo-B", "R_B", &rev_rc, false))
        .await;
    assert!(redone.is_ok(), "{redone:?}");
    assert_eq!(h.note_row("n1").await.unwrap().unwrap().body, "V2");
}

#[tokio::test]
async fn several_undone_together_are_redone_one_at_a_time() {
    let h = Harness::new().await.unwrap();
    h.seed_note("n1", "Quillet", "V0").await.unwrap();

    let opened = h.edits.open(&note_ref("n1")).await;
    assert!(opened.is_ok(), "{opened:?}");
    let rev0 = opened.unwrap().revision;

    let rev1 = h
        .edits
        .apply(&req("A", &rev0, "V1"))
        .await
        .unwrap()
        .revision;
    let rev2 = h
        .edits
        .apply(&req("B", &rev1, "V2"))
        .await
        .unwrap()
        .revision;

    let rev_undone = h
        .edits
        .revert(&revert_req("R_group", "A", &rev2, true))
        .await
        .unwrap()
        .revision;
    assert_eq!(rev_undone, rev0);

    let hist = h.edits.history(&note_ref("n1")).await.unwrap();
    let revert_a_id = hist
        .edits
        .iter()
        .find(|r| r.reverts_edit_id.as_deref() == Some("A"))
        .unwrap()
        .id
        .clone();
    let revert_b_id = hist
        .edits
        .iter()
        .find(|r| r.reverts_edit_id.as_deref() == Some("B"))
        .unwrap()
        .id
        .clone();

    let row_ra = hist.edits.iter().find(|r| r.id == revert_a_id).unwrap();
    let row_rb = hist.edits.iter().find(|r| r.id == revert_b_id).unwrap();
    assert!(row_ra.revertible);
    assert_eq!(row_ra.later, 0);
    assert!(!row_rb.revertible);

    let rev_redo_a = h
        .edits
        .revert(&revert_req("redo-A", &revert_a_id, &rev_undone, false))
        .await
        .unwrap()
        .revision;
    assert_eq!(h.note_row("n1").await.unwrap().unwrap().body, "V1");

    let hist2 = h.edits.history(&note_ref("n1")).await.unwrap();
    let row_rb2 = hist2.edits.iter().find(|r| r.id == revert_b_id).unwrap();
    assert!(row_rb2.revertible);

    let rev_redo_b = h
        .edits
        .revert(&revert_req("redo-B", &revert_b_id, &rev_redo_a, false))
        .await
        .unwrap()
        .revision;
    assert_eq!(rev_redo_b, rev2);
    assert_eq!(h.note_row("n1").await.unwrap().unwrap().body, "V2");
}
