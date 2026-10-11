use super::*;
use crate::features::learning::{
    dto::LearningProgramDto,
    pack_repository::LearningPackRepository,
    portability_dto::{LearningPackConflictPolicy, PreviewLearningPackImportRequestDto},
    repository::LearningRepository,
    tests::fixture,
};
use std::time::Duration;

async fn setup() -> Result<(
    tempfile::TempDir,
    SqlitePool,
    Container,
    LearningProgramDto,
    String,
)> {
    let directory = tempfile::tempdir()?;
    let options = sqlx::sqlite::SqliteConnectOptions::new()
        .filename(directory.path().join("replacement.sqlite"))
        .create_if_missing(true)
        .foreign_keys(true)
        .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal)
        .busy_timeout(Duration::from_secs(5));
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(2)
        .connect_with(options)
        .await
        .map_err(db)?;
    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .map_err(|error| crate::shared::error::AppError::Database(error.to_string()))?;
    let program = fixture();
    LearningRepository::new(pool.clone())
        .create(&program)
        .await?;
    let container = crate::tests::common::setup_test_container().await?;
    let packs = LearningPackRepository::new(pool.clone());
    let exported = packs
        .export(
            &container,
            &ExportLearningPackRequestDto {
                operation_id: uuid::Uuid::new_v4().to_string(),
                program_id: program.summary.id.clone(),
                file_name: format!("replacement-test-{}.lattice-learning", uuid::Uuid::new_v4()),
                include_evidence: true,
                include_practical_artifacts: true,
                include_source_bodies: true,
                source_body_redistribution_confirmed: true,
            },
        )
        .await?;
    let preview_id = uuid::Uuid::new_v4().to_string();
    let preview = packs
        .preview(&PreviewLearningPackImportRequestDto {
            operation_id: uuid::Uuid::new_v4().to_string(),
            preview_id: preview_id.clone(),
            source_path: exported.exports[0].destination_path.clone(),
            conflict_policy: LearningPackConflictPolicy::ReplaceAfterBackup,
        })
        .await?;
    assert!(preview.import_previews[0].can_apply);
    Ok((directory, pool, container, program, preview_id))
}

async fn backup_program(backup: &BackupExportRecord) -> Result<LearningProgramDto> {
    let bytes = tokio::fs::read(&backup.destination_path).await?;
    let decoded = decode_learning_pack(&bytes)?;
    Ok(serde_json::from_slice(
        &decoded.entries[super::super::PROGRAM_ENTRY],
    )?)
}

#[tokio::test]
async fn replacement_keeps_other_writers_out_until_after_delete_or_rollback() -> Result<()> {
    let (_directory, pool, container, program, preview_id) = setup().await?;
    let (mut tx, backup) =
        begin_apply(&pool, &container, &preview_id, &program.summary.id, true).await?;
    let saved = backup_program(&backup.expect("durable backup")).await?;
    assert_eq!(saved.summary.title, program.summary.title);

    let write = sqlx::query("UPDATE learning_programs SET title='Later save' WHERE id=?")
        .bind(&program.summary.id)
        .execute(&pool);
    tokio::pin!(write);
    tokio::select! {
        result = &mut write => panic!("A writer slipped between backup and replacement: {result:?}"),
        _ = tokio::time::sleep(Duration::from_millis(100)) => {},
    }
    sqlx::query("DELETE FROM learning_programs WHERE id=?")
        .bind(&program.summary.id)
        .execute(&mut *tx)
        .await
        .map_err(db)?;
    tokio::select! {
        result = &mut write => panic!("A writer escaped the replacement transaction: {result:?}"),
        _ = tokio::time::sleep(Duration::from_millis(100)) => {},
    }
    // A failed restoration leaves the old program intact, then releases writers.
    tx.rollback().await.map_err(db)?;
    assert_eq!(write.await.map_err(db)?.rows_affected(), 1);
    assert_eq!(
        LearningRepository::new(pool.clone())
            .get(&program.summary.id)
            .await?
            .summary
            .title,
        "Later save"
    );
    pool.close().await;
    Ok(())
}

#[tokio::test]
async fn replacement_captures_prior_writer_even_without_a_program_revision_bump() -> Result<()> {
    let (_directory, pool, container, program, preview_id) = setup().await?;
    let mut writer = pool.begin_with("BEGIN IMMEDIATE").await.map_err(db)?;
    sqlx::query("UPDATE learning_programs SET title='Saved while import was starting' WHERE id=?")
        .bind(&program.summary.id)
        .execute(&mut *writer)
        .await
        .map_err(db)?;
    let begin = begin_apply(&pool, &container, &preview_id, &program.summary.id, true);
    tokio::pin!(begin);
    tokio::select! {
        _ = &mut begin => panic!("Backup started before the prior writer committed"),
        _ = tokio::time::sleep(Duration::from_millis(100)) => {},
    }
    writer.commit().await.map_err(db)?;
    let (tx, backup) = begin.await?;
    let saved = backup_program(&backup.expect("durable backup")).await?;
    assert_eq!(saved.summary.title, "Saved while import was starting");
    assert_eq!(saved.summary.revision, program.summary.revision);
    tx.rollback().await.map_err(db)?;
    pool.close().await;
    Ok(())
}

#[tokio::test]
async fn replacement_rechecks_unportable_state_after_preview() -> Result<()> {
    let (_directory, pool, container, program, preview_id) = setup().await?;
    let activity_id = uuid::Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO learning_practical_activities(id,program_id,lesson_id,kind,title,brief,status,practice_mode,allowed_aids_json,outcome_ids_json,source_version_ids_json,rubric_json,runtime_kind,generator_model,revision,created_at,updated_at) VALUES(?,?,?,'project','Protected project','A project with a private evaluator','ready','practice','[]','[]','[]','[]','none','test-model',0,1,1)")
        .bind(&activity_id).bind(&program.summary.id).bind(&program.modules[0].lessons[0].id)
        .execute(&pool).await.map_err(db)?;
    sqlx::query("INSERT INTO learning_practical_files(activity_id,ordinal,path,role,content,content_sha256,editable) VALUES(?,0,'checks/test.sh','check','assertion','private-check-hash',0)")
        .bind(&activity_id).execute(&pool).await.map_err(db)?;
    let error = match begin_apply(&pool, &container, &preview_id, &program.summary.id, true).await {
        Ok(_) => panic!("Replacement must refuse newly added unportable data"),
        Err(error) => error,
    };
    assert!(error.to_string().contains("hidden practical evaluators"));
    assert_eq!(
        LearningRepository::new(pool.clone())
            .get(&program.summary.id)
            .await?
            .summary
            .title,
        program.summary.title
    );
    pool.close().await;
    Ok(())
}
