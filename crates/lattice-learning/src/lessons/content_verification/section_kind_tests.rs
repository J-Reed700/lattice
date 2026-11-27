#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic
)]
//! The `learning.section` content kind: opening, editing, check heading
//! preservation and repair, readiness gates, and form requirements.
use super::claim_ledger_tests::{figure, lesson, section, unreadable, Course};
use super::GOOD;
use crate::edits::{keys, BEING_PREPARED};
use crate::generation::CHECK_REVEAL;
use lattice_core::shared::edits::{
    ContentEditRequest, EditOp, EditSource, Editability, FieldSpec, TextFormat,
};
use lattice_core::shared::error::{AppError, Result};
use serde_json::json;
use uuid::Uuid;

async fn published() -> Result<Course> {
    let course = Course::new().await?;
    let prepared = course
        .prepare(&unreadable(), &lesson(&[GOOD, &figure(7)]))
        .await
        .expect("the lesson publishes");
    course.publish(prepared).await?;
    Ok(course)
}

fn replace(path: &str, value: &str) -> EditOp {
    EditOp::Replace {
        path: path.into(),
        value: json!(value),
    }
}

fn edit_request(
    content: lattice_core::shared::edits::ContentRef,
    expected_revision: String,
    ops: Vec<EditOp>,
) -> ContentEditRequest {
    ContentEditRequest {
        edit_id: Uuid::new_v4().to_string(),
        content,
        expected_revision,
        ops,
        source: EditSource::Learner,
        target: None,
    }
}

#[tokio::test]
async fn a_section_opens_with_its_title_and_body() -> Result<()> {
    let course = published().await?;
    let content = keys::section_ref(&course.lesson_id, 0);

    let opened = course.edits().open(&content).await;
    assert!(opened.is_ok(), "{opened:?}");
    let opened = opened?;

    assert_eq!(opened.content, content);
    assert_eq!(opened.scope, keys::learning_scope(&course.program_id));
    assert_eq!(opened.editable, Editability::Editable);
    assert_eq!(
        opened.form,
        vec![
            FieldSpec::Text {
                path: "/title".into(),
                label: "Title".into(),
                format: TextFormat::Line,
                required: true,
            },
            FieldSpec::Text {
                path: "/body".into(),
                label: "Body".into(),
                format: TextFormat::Markdown,
                required: true,
            },
        ]
    );
    assert_eq!(
        opened.document,
        json!({
            "title": "Section 0",
            "body": section(GOOD),
        })
    );
    Ok(())
}

#[tokio::test]
async fn a_section_edit_writes_the_block_and_nothing_else_of_it() -> Result<()> {
    let course = published().await?;
    let content = keys::section_ref(&course.lesson_id, 0);
    let opened = course.edits().open(&content).await;
    assert!(opened.is_ok(), "{opened:?}");
    let opened = opened?;

    let before_meta: (String, String) = sqlx::query_as(
        "SELECT kind, source_ids_json FROM learning_blocks WHERE lesson_id=? AND ordinal=0",
    )
    .bind(&course.lesson_id)
    .fetch_one(&course.pool)
    .await?;
    let other_before: (String, String) =
        sqlx::query_as("SELECT title, body FROM learning_blocks WHERE lesson_id=? AND ordinal=1")
            .bind(&course.lesson_id)
            .fetch_one(&course.pool)
            .await?;

    let outcome = course
        .edits()
        .apply(&edit_request(
            content.clone(),
            opened.revision,
            vec![
                replace("/title", "Edited Section Title"),
                replace("/body", "Updated paragraph for section 0."),
            ],
        ))
        .await;
    assert!(outcome.is_ok(), "{outcome:?}");
    let outcome = outcome?;
    assert!(outcome.changed);

    let after_row: (String, String, String, String) = sqlx::query_as(
        "SELECT kind, title, body, source_ids_json FROM learning_blocks WHERE lesson_id=? AND ordinal=0",
    )
    .bind(&course.lesson_id)
    .fetch_one(&course.pool)
    .await?;
    assert_eq!(
        after_row,
        (
            before_meta.0,
            "Edited Section Title".into(),
            "Updated paragraph for section 0.".into(),
            before_meta.1,
        )
    );
    let other_after: (String, String) =
        sqlx::query_as("SELECT title, body FROM learning_blocks WHERE lesson_id=? AND ordinal=1")
            .bind(&course.lesson_id)
            .fetch_one(&course.pool)
            .await?;
    assert_eq!(other_after, other_before);
    Ok(())
}

#[tokio::test]
async fn a_check_keeps_its_one_heading_line() -> Result<()> {
    let course = published().await?;
    let check_body =
        format!("Which row keeps headers?\n\n{CHECK_REVEAL}\n\nThe dictionary reader.");
    sqlx::query("UPDATE learning_blocks SET kind='check', body=? WHERE lesson_id=? AND ordinal=0")
        .bind(&check_body)
        .bind(&course.lesson_id)
        .execute(&course.pool)
        .await?;

    let content = keys::section_ref(&course.lesson_id, 0);
    let opened = course.edits().open(&content).await;
    assert!(opened.is_ok(), "{opened:?}");
    let opened = opened?;

    let req = edit_request(
        content,
        opened.revision,
        vec![replace(
            "/body",
            "Which row keeps headers? The dictionary reader.",
        )],
    );
    let preview = course.edits().preview(&req).await?;
    assert!(!preview.problems.is_empty(), "{preview:?}");
    assert_eq!(preview.problems[0].path.as_deref(), Some("/body"));

    let res = course.edits().apply(&req).await;
    assert!(matches!(res, Err(AppError::InvalidInput(_))), "{res:?}");
    let kept: String =
        sqlx::query_scalar("SELECT body FROM learning_blocks WHERE lesson_id=? AND ordinal=0")
            .bind(&course.lesson_id)
            .fetch_one(&course.pool)
            .await?;
    assert_eq!(kept, check_body);
    Ok(())
}

#[tokio::test]
async fn a_check_with_a_damaged_heading_can_be_repaired() -> Result<()> {
    let course = published().await?;
    let damaged = "Which row keeps headers? Missing the reveal heading.";
    sqlx::query("UPDATE learning_blocks SET kind='check', body=? WHERE lesson_id=? AND ordinal=0")
        .bind(damaged)
        .bind(&course.lesson_id)
        .execute(&course.pool)
        .await?;

    let content = keys::section_ref(&course.lesson_id, 0);
    let opened = course.edits().open(&content).await;
    assert!(opened.is_ok(), "{opened:?}");
    let opened = opened?;

    let repaired = format!("Which row keeps headers?\n\n{CHECK_REVEAL}\n\nThe dictionary reader.");
    let outcome = course
        .edits()
        .apply(&edit_request(
            content,
            opened.revision,
            vec![replace("/body", &repaired)],
        ))
        .await;
    assert!(outcome.is_ok(), "{outcome:?}");

    let stored: String =
        sqlx::query_scalar("SELECT body FROM learning_blocks WHERE lesson_id=? AND ordinal=0")
            .bind(&course.lesson_id)
            .fetch_one(&course.pool)
            .await?;
    assert_eq!(stored, repaired);
    Ok(())
}

#[tokio::test]
async fn a_lesson_still_being_prepared_is_not_editable() -> Result<()> {
    let course = published().await?;
    sqlx::query("UPDATE learning_lesson_reviews SET state='verifying' WHERE lesson_id=?")
        .bind(&course.lesson_id)
        .execute(&course.pool)
        .await?;

    let content = keys::section_ref(&course.lesson_id, 0);
    let opened = course.edits().open(&content).await;
    assert!(opened.is_ok(), "{opened:?}");
    let opened = opened?;
    assert_eq!(
        opened.editable,
        Editability::NotNow {
            reason: BEING_PREPARED.into(),
        }
    );

    let res = course
        .edits()
        .apply(&edit_request(
            content,
            opened.revision,
            vec![replace("/title", "Attempt while verifying")],
        ))
        .await;
    assert!(matches!(res, Err(AppError::InvalidState(_))), "{res:?}");
    Ok(())
}

#[tokio::test]
async fn a_lesson_without_a_review_is_editable() -> Result<()> {
    let course = published().await?;
    sqlx::query("DELETE FROM learning_lesson_reviews WHERE lesson_id=?")
        .bind(&course.lesson_id)
        .execute(&course.pool)
        .await?;

    let content = keys::section_ref(&course.lesson_id, 0);
    let opened = course.edits().open(&content).await;
    assert!(opened.is_ok(), "{opened:?}");
    let opened = opened?;
    assert_eq!(opened.editable, Editability::Editable);

    let outcome = course
        .edits()
        .apply(&edit_request(
            content,
            opened.revision,
            vec![replace("/title", "Edited without review")],
        ))
        .await;
    assert!(outcome.is_ok(), "{outcome:?}");

    let stored: String =
        sqlx::query_scalar("SELECT title FROM learning_blocks WHERE lesson_id=? AND ordinal=0")
            .bind(&course.lesson_id)
            .fetch_one(&course.pool)
            .await?;
    assert_eq!(stored, "Edited without review");
    Ok(())
}

#[tokio::test]
async fn a_blank_title_is_refused_by_the_form() -> Result<()> {
    let course = published().await?;
    let content = keys::section_ref(&course.lesson_id, 0);
    let opened = course.edits().open(&content).await;
    assert!(opened.is_ok(), "{opened:?}");
    let opened = opened?;

    let req = edit_request(content, opened.revision, vec![replace("/title", "   ")]);
    let preview = course.edits().preview(&req).await?;
    assert!(
        preview
            .problems
            .iter()
            .any(|p| p.path.as_deref() == Some("/title")),
        "{preview:?}"
    );
    let res = course.edits().apply(&req).await;
    assert!(matches!(res, Err(AppError::InvalidInput(_))), "{res:?}");
    Ok(())
}
