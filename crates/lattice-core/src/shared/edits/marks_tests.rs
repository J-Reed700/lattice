use serde_json::json;

use super::fake::{note_ref, Harness, FAKE_REACTOR};
use super::types::{ContentEditRequest, EditOp, EditSource};

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

#[tokio::test]
async fn a_mark_reports_content_edited_or_replaced_since() {
    let h = Harness::new().await.unwrap();
    h.seed_note("n1", "Quillet", "Body 0").await.unwrap();

    let mut read = h.database.begin_read().await.unwrap();
    let opened_in = h.edits.open_in(&mut read.reading(), &note_ref("n1")).await;
    assert!(opened_in.is_ok(), "{opened_in:?}");
    let marks = h.edits.marks(&mut read.reading(), &[note_ref("n1")]).await;
    assert!(marks.is_ok(), "{marks:?}");
    let marks = marks.unwrap();
    assert_eq!(marks.len(), 1);
    let moved0 = h.edits.moved(&mut read.reading(), &marks).await.unwrap();
    assert!(moved0.is_empty());
    read.commit().await.unwrap();

    let rev0 = marks.first().unwrap().revision.clone();
    h.edits.apply(&req("e1", &rev0, "Body 1")).await.unwrap();

    let mut read = h.database.begin_read().await.unwrap();
    let moved1 = h.edits.moved(&mut read.reading(), &marks).await.unwrap();
    assert_eq!(moved1, vec![note_ref("n1")]);
    let edited_paths = h
        .edits
        .edited_paths(&mut read.reading(), &note_ref("n1"))
        .await
        .unwrap();
    assert_eq!(edited_paths, vec!["/body".to_string()]);
    let newest = h
        .edits
        .newest(&mut read.reading(), &[note_ref("n1")], FAKE_REACTOR)
        .await
        .unwrap();
    assert_eq!(newest.len(), 1);
    assert!(newest.first().unwrap().unbroken);
    read.commit().await.unwrap();

    let after_edit_marks = {
        let mut read = h.database.begin_read().await.unwrap();
        let m = h
            .edits
            .marks(&mut read.reading(), &[note_ref("n1")])
            .await
            .unwrap();
        read.commit().await.unwrap();
        m
    };

    sqlx::query("UPDATE fake_notes SET body = 'Body SQL' WHERE id = 'n1'")
        .execute(&h.pool)
        .await
        .unwrap();

    let mut read = h.database.begin_read().await.unwrap();
    assert_eq!(
        h.edits
            .moved(&mut read.reading(), &after_edit_marks)
            .await
            .unwrap(),
        vec![note_ref("n1")]
    );
    let newest_broken = h
        .edits
        .newest(&mut read.reading(), &[note_ref("n1")], FAKE_REACTOR)
        .await
        .unwrap();
    assert!(!newest_broken.first().unwrap().unbroken);
    read.commit().await.unwrap();

    sqlx::query("DELETE FROM fake_notes WHERE id = 'n1'")
        .execute(&h.pool)
        .await
        .unwrap();

    let mut read = h.database.begin_read().await.unwrap();
    assert_eq!(
        h.edits
            .moved(&mut read.reading(), &after_edit_marks)
            .await
            .unwrap(),
        vec![note_ref("n1")]
    );
    let newest_missing = h
        .edits
        .newest(&mut read.reading(), &[note_ref("n1")], FAKE_REACTOR)
        .await
        .unwrap();
    assert!(!newest_missing.first().unwrap().unbroken);
    assert!(newest_missing.first().unwrap().revision.is_empty());
    read.commit().await.unwrap();
}
