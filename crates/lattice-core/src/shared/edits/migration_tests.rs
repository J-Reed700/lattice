use crate::shared::error::Result;
use crate::shared::testing::sqlite::migrated_in_memory;

#[tokio::test]
async fn the_schema_has_the_edit_log() -> Result<()> {
    let pool = migrated_in_memory().await?;
    let tables: Vec<String> = sqlx::query_scalar(
        "SELECT name FROM sqlite_master WHERE type='table' AND name IN ('content_edits', 'content_edit_receipts') ORDER BY name",
    )
    .fetch_all(&pool)
    .await?;
    assert_eq!(tables, ["content_edit_receipts", "content_edits"]);
    Ok(())
}

#[tokio::test]
async fn a_courses_history_leaves_with_the_course() -> Result<()> {
    let pool = migrated_in_memory().await?;
    sqlx::query(
        "INSERT INTO learning_programs(id, title, goal, status, revision, prior_knowledge, minutes_per_session, model_name, created_at) \
         VALUES ('program-1', 'The Quillet Guide', 'Master Quillet', 'active', 1, '', 20, 'model', 1)",
    )
    .execute(&pool)
    .await?;

    sqlx::query(
        "INSERT INTO content_edits(id, kind, content_key, scope_feature, scope_container, seq, origin_json, target_json, ops_json, inverse_json, before_revision, after_revision, reverts_edit_id, request_sha256, outcome_json, app_version, created_at) \
         VALUES ('edit-1', 'learning.section', 'lesson-1/0', 'learning', 'program-1', 1, '{\"type\":\"learner\"}', NULL, '[]', '[]', 'r0', 'r1', NULL, 'sha', NULL, '1.0.0', 1)",
    )
    .execute(&pool)
    .await?;

    sqlx::query(
        "INSERT INTO content_edit_receipts(edit_id, reactor, receipt_json) VALUES ('edit-1', 'learning.claims', '{}')",
    )
    .execute(&pool)
    .await?;

    sqlx::query("DELETE FROM learning_programs WHERE id='program-1'")
        .execute(&pool)
        .await?;

    let remaining_edits: i64 =
        sqlx::query_scalar("SELECT count(*) FROM content_edits WHERE scope_container='program-1'")
            .fetch_one(&pool)
            .await?;
    let remaining_receipts: i64 =
        sqlx::query_scalar("SELECT count(*) FROM content_edit_receipts WHERE edit_id='edit-1'")
            .fetch_one(&pool)
            .await?;
    assert_eq!(remaining_edits, 0);
    assert_eq!(remaining_receipts, 0);
    Ok(())
}
