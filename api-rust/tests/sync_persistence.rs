//! Real PostgreSQL contract tests. Each test gets an isolated sqlx test database.
//! Run with DATABASE_URL set and `cargo test --test sync_persistence -- --ignored`.
use api_rust::{
    error::AppError,
    persistence::sync_repository::PgSyncRepository,
    sync::{
        repository::SyncRepository,
        types::{PushChange, SyncAction},
    },
};
use sqlx::PgPool;
use uuid::Uuid;

fn change(path: &str) -> PushChange {
    PushChange {
        client_op_id: Uuid::new_v4(),
        action: SyncAction::Create,
        path: path.into(),
        title: Some("Title".into()),
        content: Some("content".into()),
        content_hash: None,
        base_version: Some(0),
    }
}
async fn device(repo: &PgSyncRepository, user: i64) -> Uuid {
    let id = Uuid::new_v4();
    repo.upsert_device(user, id, "test".into()).await.unwrap();
    id
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires a disposable PostgreSQL DATABASE_URL"]
async fn retry_is_idempotent_and_pull_is_tenant_scoped(pool: PgPool) {
    let repo = PgSyncRepository::new(pool.clone());
    let writer = device(&repo, 42).await;
    let reader = device(&repo, 42).await;
    let outsider = device(&repo, 43).await;
    let op = change("note.md");
    let first = repo
        .push_changes(42, writer, vec![op.clone()])
        .await
        .unwrap();
    let retry = repo.push_changes(42, writer, vec![op]).await.unwrap();
    assert_eq!(retry.high_watermark_seq, first.high_watermark_seq);
    assert_eq!(
        repo.pull_changes(42, reader, 0, 100)
            .await
            .unwrap()
            .changes
            .len(),
        1
    );
    assert!(
        repo.pull_changes(43, outsider, 0, 100)
            .await
            .unwrap()
            .changes
            .is_empty()
    );
    assert!(matches!(
        repo.get_device(43, writer).await,
        Err(AppError::NotFound(_))
    ));
    let version: i64 = sqlx::query_scalar(
        "SELECT version FROM document_heads WHERE user_id=42 AND path='note.md'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(version, 1);
    assert_eq!(
        repo.ack_checkpoint(42, reader, first.high_watermark_seq)
            .await
            .unwrap()
            .last_acked_seq,
        first.high_watermark_seq
    );
    assert_eq!(
        repo.ack_checkpoint(42, reader, 0)
            .await
            .unwrap()
            .last_acked_seq,
        first.high_watermark_seq
    );
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires a disposable PostgreSQL DATABASE_URL"]
async fn failed_batch_rolls_back_operations_and_heads(pool: PgPool) {
    let repo = PgSyncRepository::new(pool.clone());
    let writer = device(&repo, 42).await;
    let valid = change("valid.md");
    let mut invalid = change("invalid.md");
    invalid.content_hash = Some("x".repeat(65));
    assert!(
        repo.push_changes(42, writer, vec![valid.clone(), invalid])
            .await
            .is_err()
    );
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM document_ops")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 0);
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM document_heads")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 0);
    assert_eq!(
        repo.push_changes(42, writer, vec![valid])
            .await
            .unwrap()
            .accepted_paths,
        vec!["valid.md"]
    );
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires a disposable PostgreSQL DATABASE_URL"]
async fn concurrent_first_writes_preserve_optimistic_conflict_detection(pool: PgPool) {
    let repo = PgSyncRepository::new(pool.clone());
    let first_device = device(&repo, 42).await;
    let second_device = device(&repo, 42).await;
    // Widen the empty-head race; row locks alone cannot lock a row that is absent.
    sqlx::raw_sql("CREATE FUNCTION slow_head_insert() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN PERFORM pg_sleep(0.1); RETURN NEW; END $$;
        CREATE TRIGGER slow_head BEFORE INSERT ON document_heads FOR EACH ROW EXECUTE FUNCTION slow_head_insert();")
        .execute(&pool).await.unwrap();
    let (a, b) = tokio::join!(
        repo.push_changes(42, first_device, vec![change("same.md")]),
        repo.push_changes(42, second_device, vec![change("same.md")])
    );
    let (a, b) = (a.unwrap(), b.unwrap());
    assert_eq!(a.accepted_paths.len() + b.accepted_paths.len(), 1);
    assert_eq!(a.conflicts.len() + b.conflicts.len(), 1);
    let version: i64 =
        sqlx::query_scalar("SELECT version FROM document_heads WHERE path='same.md'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(version, 1);
}
