//! Replacement previews and backups on one computer.
use super::id;
use crate::features::learning::portability_dto::LearningPackExportOptionsDto;
use crate::features::learning::{
    pack_repository::LearningPackRepository,
    portability_dto::{
        ApplyLearningPackImportRequestDto, CancelLearningPackImportPreviewRequestDto,
        ExportLearningPackRequestDto, LearningPackConflictKind, LearningPackConflictPolicy,
        PreviewLearningPackImportRequestDto,
    },
    repository::LearningRepository,
    tests::{fixture, pool},
};
use crate::shared::error::Result;

#[tokio::test]
async fn cancelled_preview_and_blocked_replace_do_not_write_program_rows() -> Result<()> {
    let pool = pool().await?;
    let program = fixture();
    LearningRepository::new(pool.clone())
        .create(&program)
        .await?;
    let lesson_id = &program.modules[0].lessons[0].id;
    let activity_id = id();
    sqlx::query("INSERT INTO learning_practical_activities(id,program_id,lesson_id,predecessor_id,kind,title,brief,status,practice_mode,allowed_aids_json,outcome_ids_json,source_version_ids_json,rubric_json,runtime_kind,runtime_profile_id,runtime_engine,runtime_image_id,runtime_command_json,runtime_limits_json,generator_model,revision,created_at,updated_at) VALUES(?,?,?,NULL,'project','Protected project','A project with a private evaluator','ready','practice','[]','[]','[]','[]','none',NULL,NULL,NULL,NULL,NULL,'test-model',0,1,1)")
        .bind(&activity_id)
        .bind(&program.summary.id)
        .bind(lesson_id)
        .execute(&pool)
        .await
        .expect("practical activity inserts");
    sqlx::query("INSERT INTO learning_practical_files(activity_id,ordinal,path,role,content,content_sha256,editable) VALUES(?,0,'checks/test.sh','check','assertion','private-check-hash',0)")
        .bind(&activity_id)
        .execute(&pool)
        .await
        .expect("hidden practical evaluator inserts");

    let workspace = tempfile::tempdir().expect("pack workspace");
    let packs = LearningPackRepository::new(
        pool.clone(),
        workspace.path().join("learning-packs"),
        workspace.path().to_path_buf(),
    );
    let exported = packs
        .export(&ExportLearningPackRequestDto {
            operation_id: id(),
            program_id: program.summary.id.clone(),
            options: LearningPackExportOptionsDto {
                include_evidence: false,
                include_practical_artifacts: false,
                include_source_bodies: false,
                source_body_redistribution_confirmed: false,
            },
        })
        .await?;
    let source_path = exported.exports[0].destination_path.clone();
    let source_program_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM learning_programs")
        .fetch_one(&pool)
        .await?;
    let source_hidden_file_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM learning_practical_files f JOIN learning_practical_activities a ON a.id=f.activity_id WHERE a.program_id=? AND f.role IN ('check','solution')")
        .bind(&program.summary.id)
        .fetch_one(&pool)
        .await?;

    let blocked = packs
        .preview(&PreviewLearningPackImportRequestDto {
            operation_id: id(),
            preview_id: id(),
            source_path: source_path.clone(),
            conflict_policy: LearningPackConflictPolicy::ReplaceAfterBackup,
        })
        .await?;
    let blocked_preview = blocked.import_previews.first().expect("replace preview");
    assert!(!blocked_preview.can_apply);
    // The preview shows the reason itself, not an error's display wrapper.
    let history = blocked_preview
        .conflicts
        .iter()
        .find(|conflict| conflict.kind == LearningPackConflictKind::ProgramHistory)
        .expect("the program history conflict");
    assert_eq!(
        history.clears_with,
        [LearningPackConflictPolicy::CreateCopy]
    );
    assert_eq!(
        history.reason.as_deref(),
        Some("Replace was blocked because the target has hidden practical evaluators that this pack version cannot restore. The existing program was left unchanged.")
    );
    let title_after_block: String =
        sqlx::query_scalar("SELECT title FROM learning_programs WHERE id=?")
            .bind(&program.summary.id)
            .fetch_one(&pool)
            .await?;
    assert_eq!(title_after_block, program.summary.title);
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM learning_programs")
            .fetch_one(&pool)
            .await?,
        source_program_count
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM learning_practical_files f JOIN learning_practical_activities a ON a.id=f.activity_id WHERE a.program_id=? AND f.role IN ('check','solution')")
        .bind(&program.summary.id)
        .fetch_one(&pool)
        .await?,
        source_hidden_file_count
    );

    let cancelled = packs
        .preview(&PreviewLearningPackImportRequestDto {
            operation_id: id(),
            preview_id: id(),
            source_path,
            conflict_policy: LearningPackConflictPolicy::CreateCopy,
        })
        .await?;
    let pending = cancelled.import_previews.first().expect("copy preview");
    let pending_id = pending.id.clone();
    let cancelled = packs
        .cancel(&CancelLearningPackImportPreviewRequestDto {
            operation_id: id(),
            preview_id: pending_id.clone(),
        })
        .await?;
    assert_eq!(
        cancelled
            .import_previews
            .iter()
            .find(|preview| preview.id == pending_id)
            .map(|preview| preview.status),
        Some(crate::features::learning::portability_dto::LearningPackPreviewStatus::Cancelled)
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM learning_programs")
            .fetch_one(&pool)
            .await?,
        source_program_count
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM learning_practical_files f JOIN learning_practical_activities a ON a.id=f.activity_id WHERE a.program_id=? AND f.role IN ('check','solution')")
        .bind(&program.summary.id)
        .fetch_one(&pool)
        .await?,
        source_hidden_file_count
    );
    Ok(())
}

#[tokio::test]
async fn successful_replace_returns_a_readable_recovery_backup() -> Result<()> {
    let pool = pool().await?;
    let program = fixture();
    LearningRepository::new(pool.clone())
        .create(&program)
        .await?;
    let workspace = tempfile::tempdir().expect("pack workspace");
    let packs = LearningPackRepository::new(
        pool.clone(),
        workspace.path().join("learning-packs"),
        workspace.path().to_path_buf(),
    );
    let exported = packs
        .export(&ExportLearningPackRequestDto {
            operation_id: id(),
            program_id: program.summary.id.clone(),
            options: LearningPackExportOptionsDto {
                include_evidence: false,
                include_practical_artifacts: false,
                include_source_bodies: false,
                source_body_redistribution_confirmed: false,
            },
        })
        .await?;
    let preview = packs
        .preview(&PreviewLearningPackImportRequestDto {
            operation_id: id(),
            preview_id: id(),
            source_path: exported.exports[0].destination_path.clone(),
            conflict_policy: LearningPackConflictPolicy::ReplaceAfterBackup,
        })
        .await?;
    let preview = preview.import_previews.first().expect("replace preview");
    assert!(preview.can_apply);
    let preview_id = preview.id.clone();
    let applied = packs
        .apply(&ApplyLearningPackImportRequestDto {
            operation_id: id(),
            preview_id: preview_id.clone(),
            expected_root_sha256: preview.manifest.root_sha256.clone(),
        })
        .await?;
    let import = applied
        .imports
        .iter()
        .find(|item| item.preview_id == preview_id)
        .expect("replacement import result");
    let backup_id = import
        .backup_id
        .as_ref()
        .expect("replacement returns a recovery backup ID");
    assert!(applied.exports.iter().any(|export| {
        export.id.as_str() == backup_id.as_str()
            && export
                .destination_path
                .ends_with(&format!("backup-{backup_id}.lattice-learning"))
    }));
    let backup_path = workspace
        .path()
        .join("learning-packs")
        .join(format!("backup-{backup_id}.lattice-learning"));
    let backup_bytes = tokio::fs::read(&backup_path)
        .await
        .expect("reported recovery backup is present");
    let decoded = crate::features::learning::pack::decode_learning_pack(&backup_bytes)
        .expect("reported recovery backup is a valid learning pack");
    assert_eq!(decoded.manifest.title, program.summary.title);
    let backed_up_program: serde_json::Value = serde_json::from_slice(
        decoded
            .entries
            .get("program/program.json")
            .expect("backup contains its program snapshot"),
    )
    .expect("backup program snapshot parses");
    assert_eq!(
        backed_up_program["summary"]["id"],
        program.summary.id.as_str()
    );
    let restored_title: String =
        sqlx::query_scalar("SELECT title FROM learning_programs WHERE id=?")
            .bind(&program.summary.id)
            .fetch_one(&pool)
            .await
            .expect("replacement program remains available");
    assert_eq!(restored_title, program.summary.title);
    Ok(())
}
