//! Real PostgreSQL contract tests for pulling, conflicts, status and device
//! registration. Each test gets an isolated sqlx test database.
//! Run with DATABASE_URL set and
//! `cargo test --test sync_repository_reads -- --ignored`.
use lattice_sync::{
    error::AppError,
    persistence::sync_repository::PgSyncRepository,
    sync::{
        repository::SyncRepository,
        types::{ConflictResolution, ConflictStatus, PushChange, SyncAction},
    },
};
use sqlx::PgPool;
use uuid::Uuid;

fn create(path: &str) -> PushChange {
    PushChange {
        client_op_id: Uuid::new_v4(),
        action: SyncAction::Create,
        path: path.into(),
        title: Some("Field notes".into()),
        content: Some("Observed three herons at dawn.".into()),
        content_hash: None,
        base_version: Some(0),
    }
}

async fn device(repo: &PgSyncRepository, user: i64, name: &str) -> Uuid {
    let id = Uuid::new_v4();
    repo.upsert_device(user, id, name.into()).await.unwrap();
    id
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires a disposable PostgreSQL DATABASE_URL"]
async fn pull_pages_by_sequence_and_skips_the_readers_own_writes(pool: PgPool) {
    let repo = PgSyncRepository::new(pool.clone());
    let writer = device(&repo, 7, "laptop").await;
    let reader = device(&repo, 7, "phone").await;
    let pushed = repo
        .push_changes(
            7,
            writer,
            vec![create("a.md"), create("b.md"), create("c.md")],
        )
        .await
        .unwrap();
    assert_eq!(pushed.accepted_paths.len(), 3);

    let first = repo.pull_changes(7, reader, 0, 2).await.unwrap();
    let paths: Vec<_> = first.changes.iter().map(|c| c.path.as_str()).collect();
    assert_eq!(paths, vec!["a.md", "b.md"]);
    assert!(first.changes[0].seq < first.changes[1].seq);
    assert_eq!(first.next_seq, first.changes[1].seq);

    let second = repo
        .pull_changes(7, reader, first.next_seq, 2)
        .await
        .unwrap();
    assert_eq!(second.changes.len(), 1);
    assert_eq!(second.changes[0].path, "c.md");
    assert_eq!(second.next_seq, pushed.high_watermark_seq);

    let drained = repo
        .pull_changes(7, reader, second.next_seq, 2)
        .await
        .unwrap();
    assert!(drained.changes.is_empty());
    assert_eq!(
        drained.next_seq, second.next_seq,
        "an empty page keeps the cursor"
    );

    let own = repo.pull_changes(7, writer, 0, 100).await.unwrap();
    assert!(
        own.changes.is_empty(),
        "a device never pulls its own writes"
    );

    let stranger = Uuid::new_v4();
    assert!(matches!(
        repo.pull_changes(7, stranger, 0, 100).await,
        Err(AppError::NotFound(_))
    ));
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires a disposable PostgreSQL DATABASE_URL"]
async fn conflicts_show_in_pull_and_status_until_resolved(pool: PgPool) {
    let repo = PgSyncRepository::new(pool.clone());
    let writer = device(&repo, 9, "desktop").await;
    let reader = device(&repo, 9, "tablet").await;
    let first = repo
        .push_changes(9, writer, vec![create("herons.md")])
        .await
        .unwrap();
    let stale = repo
        .push_changes(9, writer, vec![create("herons.md")])
        .await
        .unwrap();
    assert_eq!(stale.conflicts.len(), 1);
    let conflict_id = stale.conflicts[0].id;

    let pulled = repo.pull_changes(9, reader, 0, 100).await.unwrap();
    assert_eq!(pulled.conflicts.len(), 1);
    assert!(matches!(
        pulled.conflicts[0].status,
        ConflictStatus::Pending
    ));

    let status = repo.get_status(9, reader).await.unwrap();
    assert_eq!(status.device_id, reader);
    assert_eq!(status.last_acked_seq, 0);
    assert_eq!(status.pending_conflicts, 1);
    assert_eq!(status.synced_documents, 1);
    assert_eq!(status.server_high_watermark_seq, stale.high_watermark_seq);

    assert!(matches!(
        repo.resolve_conflict(10, conflict_id, ConflictResolution::ResolvedLocal, None)
            .await,
        Err(AppError::NotFound(_))
    ));
    assert!(matches!(
        repo.resolve_conflict(
            9,
            conflict_id + 1_000,
            ConflictResolution::ResolvedLocal,
            None
        )
        .await,
        Err(AppError::NotFound(_))
    ));
    repo.resolve_conflict(
        9,
        conflict_id,
        ConflictResolution::ResolvedMerge,
        Some("Observed three herons and an egret.".into()),
    )
    .await
    .unwrap();
    let (stored_status, merged): (String, Option<String>) =
        sqlx::query_as("SELECT status::text, resolution_content FROM conflicts WHERE id = $1")
            .bind(conflict_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(stored_status, "resolved_merge");
    assert_eq!(
        merged.as_deref(),
        Some("Observed three herons and an egret.")
    );
    assert!(
        repo.pull_changes(9, reader, 0, 100)
            .await
            .unwrap()
            .conflicts
            .is_empty()
    );

    repo.ack_checkpoint(9, reader, first.high_watermark_seq)
        .await
        .unwrap();
    let status = repo.get_status(9, reader).await.unwrap();
    assert_eq!(status.pending_conflicts, 0);
    assert_eq!(status.last_acked_seq, first.high_watermark_seq);
    assert!(matches!(
        repo.get_status(9, Uuid::new_v4()).await,
        Err(AppError::NotFound(_))
    ));
    assert!(matches!(
        repo.ack_checkpoint(9, Uuid::new_v4(), 1).await,
        Err(AppError::NotFound(_))
    ));
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires a disposable PostgreSQL DATABASE_URL"]
async fn registering_a_device_again_renames_it_in_place(pool: PgPool) {
    let repo = PgSyncRepository::new(pool.clone());
    let uuid = Uuid::new_v4();
    let first = repo
        .upsert_device(5, uuid, "kitchen tablet".into())
        .await
        .unwrap();
    let again = repo
        .upsert_device(5, uuid, "garden tablet".into())
        .await
        .unwrap();
    assert_eq!(again.id, first.id);
    assert_eq!(again.device_name, "garden tablet");
    assert_eq!(again.created_at, first.created_at);
    assert_eq!(
        repo.get_device(5, uuid).await.unwrap().device_name,
        "garden tablet"
    );

    let other_user = repo
        .upsert_device(6, uuid, "kitchen tablet".into())
        .await
        .unwrap();
    assert_ne!(other_user.id, first.id, "devices are scoped to their user");
}
