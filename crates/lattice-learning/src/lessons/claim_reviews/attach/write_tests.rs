use super::*;
use lattice_core::shared::persistence::DatabaseHandle;

#[tokio::test]
async fn a_check_whose_example_cannot_be_saved_leaves_the_card_unchanged() {
    let world = world().await;
    sqlx::query("PRAGMA foreign_keys=OFF")
        .execute(world.pool.sqlx_pool())
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO learning_lesson_claims (claim_id, lesson_id, unit_index, unit_kind, field, quote, statement, state_json, evidence_json, updated_at) VALUES ('water', ?, 0, 'block', 'body', 'q', ?, '{}', '[]', 1)",
    )
    .bind(LESSON)
    .bind(WATER)
    .execute(world.pool.sqlx_pool())
    .await
    .unwrap();
    sqlx::query(
        "CREATE TRIGGER refuse_examples BEFORE INSERT ON learning_checker_examples BEGIN SELECT RAISE(ABORT, 'injected'); END",
    )
    .execute(world.pool.sqlx_pool())
    .await
    .unwrap();
    let web = ScriptedWeb::article();
    let snapshots = Snapshots::answering(ReferenceSnapshot::Pending);
    let judge = Judge::says("supported");

    let result = world
        .attach(&web, &snapshots, &judge, "water", World::pasted(NOTES))
        .await;

    assert!(result.is_err());
    let shown: String = sqlx::query_scalar(
        "SELECT evidence_json FROM learning_lesson_claims WHERE claim_id='water'",
    )
    .fetch_one(world.pool.sqlx_pool())
    .await
    .unwrap();
    assert_eq!(shown, "[]");
}
