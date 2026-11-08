//! Cancel and apply accept only pending previews, replay by operation and
//! refuse a pack whose bytes changed after it was previewed.
use super::id;
use crate::features::learning::{
    pack_repository::LearningPackRepository,
    portability_dto::{
        ApplyLearningPackImportRequestDto, CancelLearningPackImportPreviewRequestDto,
        ExportLearningPackRequestDto, LearningPackConflictPolicy, LearningPackExportOptionsDto,
        LearningPackImportPreviewDto, LearningPackPreviewStatus,
        PreviewLearningPackImportRequestDto,
    },
    repository::LearningRepository,
    tests::{fixture, pool},
};
use lattice_core::shared::error::{AppError, Result};
use sqlx::SqlitePool;

#[track_caller]
fn assert_invalid<T: std::fmt::Debug>(result: Result<T>) {
    assert!(
        matches!(result, Err(AppError::InvalidInput(_))),
        "expected InvalidInput, got {result:?}"
    );
}

#[track_caller]
fn assert_not_found<T: std::fmt::Debug>(result: Result<T>) {
    assert!(
        matches!(result, Err(AppError::NotFound(_))),
        "expected NotFound, got {result:?}"
    );
}

async fn programs(pool: &SqlitePool) -> Result<i64> {
    Ok(sqlx::query_scalar("SELECT COUNT(*) FROM learning_programs")
        .fetch_one(pool)
        .await?)
}

/// Creates a program, exports it and returns the pack path.
async fn exported_pack(pool: &SqlitePool, packs: &LearningPackRepository) -> Result<String> {
    let program = fixture();
    LearningRepository::new(pool.clone())
        .create(&program)
        .await?;
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
    Ok(exported.exports[0].destination_path.clone())
}

async fn preview(
    packs: &LearningPackRepository,
    path: &str,
) -> Result<LearningPackImportPreviewDto> {
    let preview_id = id();
    let workspace = packs
        .preview(&PreviewLearningPackImportRequestDto {
            operation_id: id(),
            preview_id: preview_id.clone(),
            source_path: path.into(),
            conflict_policy: LearningPackConflictPolicy::CreateCopy,
        })
        .await?;
    workspace
        .import_previews
        .into_iter()
        .find(|p| p.id == preview_id)
        .ok_or_else(|| AppError::NotFound("preview row".into()))
}

fn apply_request(preview: &LearningPackImportPreviewDto) -> ApplyLearningPackImportRequestDto {
    ApplyLearningPackImportRequestDto {
        operation_id: id(),
        preview_id: preview.id.clone(),
        expected_root_sha256: preview.manifest.root_sha256.clone(),
    }
}

#[tokio::test]
async fn only_a_pending_preview_can_be_cancelled_or_applied() -> Result<()> {
    let pool = pool().await?;
    let dir = tempfile::tempdir().expect("pack workspace");
    let packs = LearningPackRepository::new(
        pool.clone(),
        dir.path().join("learning-packs"),
        dir.path().to_path_buf(),
    );
    let path = exported_pack(&pool, &packs).await?;
    let before = programs(&pool).await?;

    let cancelled = preview(&packs, &path).await?;
    assert_eq!(cancelled.status, LearningPackPreviewStatus::Pending);
    let cancel = CancelLearningPackImportPreviewRequestDto {
        operation_id: id(),
        preview_id: cancelled.id.clone(),
    };
    packs.cancel(&cancel).await?;
    let replayed = packs.cancel(&cancel).await?;
    assert_eq!(
        replayed
            .import_previews
            .iter()
            .find(|p| p.id == cancelled.id)
            .map(|p| p.status),
        Some(LearningPackPreviewStatus::Cancelled)
    );
    assert_invalid(
        packs
            .cancel(&CancelLearningPackImportPreviewRequestDto {
                operation_id: id(),
                preview_id: cancelled.id.clone(),
            })
            .await,
    );
    assert_not_found(
        packs
            .cancel(&CancelLearningPackImportPreviewRequestDto {
                operation_id: id(),
                preview_id: id(),
            })
            .await,
    );
    assert_invalid(
        packs
            .cancel(&CancelLearningPackImportPreviewRequestDto {
                operation_id: "not-a-uuid".into(),
                preview_id: cancelled.id.clone(),
            })
            .await,
    );
    assert_invalid(packs.apply(&apply_request(&cancelled)).await);
    let mut unknown = apply_request(&cancelled);
    unknown.preview_id = id();
    assert_not_found(packs.apply(&unknown).await);
    assert_eq!(programs(&pool).await?, before);

    let pending = preview(&packs, &path).await?;
    let mut wrong_root = apply_request(&pending);
    wrong_root.expected_root_sha256 = "0".repeat(64);
    assert_invalid(packs.apply(&wrong_root).await);
    assert_eq!(
        programs(&pool).await?,
        before,
        "a refused apply writes nothing"
    );

    let apply = apply_request(&pending);
    packs.apply(&apply).await?;
    packs.apply(&apply).await?;
    assert_eq!(
        programs(&pool).await?,
        before + 1,
        "a replayed apply imports once"
    );
    let status: String =
        sqlx::query_scalar("SELECT status FROM learning_pack_import_previews WHERE id=?")
            .bind(&pending.id)
            .fetch_one(&pool)
            .await?;
    assert_eq!(status, "applied");
    assert_invalid(packs.apply(&apply_request(&pending)).await);
    assert_eq!(programs(&pool).await?, before + 1);
    Ok(())
}

#[tokio::test]
async fn a_pack_rewritten_after_preview_is_not_applied() -> Result<()> {
    let pool = pool().await?;
    let dir = tempfile::tempdir().expect("pack workspace");
    let packs = LearningPackRepository::new(
        pool.clone(),
        dir.path().join("learning-packs"),
        dir.path().to_path_buf(),
    );
    let path = exported_pack(&pool, &packs).await?;
    let other_path = exported_pack(&pool, &packs).await?;
    let previewed = preview(&packs, &path).await?;
    let before = programs(&pool).await?;

    std::fs::copy(&other_path, &path).expect("replace the previewed pack");
    assert_invalid(packs.apply(&apply_request(&previewed)).await);
    assert_eq!(programs(&pool).await?, before);
    let status: String =
        sqlx::query_scalar("SELECT status FROM learning_pack_import_previews WHERE id=?")
            .bind(&previewed.id)
            .fetch_one(&pool)
            .await?;
    assert_eq!(
        status, "pending",
        "a refused apply leaves the preview pending"
    );
    Ok(())
}
