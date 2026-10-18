//! Exporting every course into one folder.
use super::{id, packs, picks, Picks};
use crate::features::learning::{
    dto::{AcceptLearningProgramRequestDto, LearningProgramDto},
    operations::Operation,
    pack::decode_learning_pack,
    persistence::hash,
    portability_dto::{
        ExportAllLearningPacksRequestDto, ExportAllLearningPacksResultDto, ExportedLearningPackDto,
        FailedLearningPackExportDto, LearningPackExportOptionsDto,
    },
    repository::LearningRepository,
    tests::{fixture, pool},
};
use lattice_core::shared::error::{AppError, Result};
use sqlx::SqlitePool;
use std::path::Path;

/// What a batch into a chosen folder did.
struct Batch {
    destination_dir: String,
    exported: Vec<ExportedLearningPackDto>,
    failed: Vec<FailedLearningPackExportDto>,
}

/// The batch a chosen folder produced; a dismissed dialog fails the test.
fn done(result: ExportAllLearningPacksResultDto) -> Result<Batch> {
    match result {
        ExportAllLearningPacksResultDto::Exported {
            destination_dir,
            exported,
            failed,
        } => Ok(Batch {
            destination_dir,
            exported,
            failed,
        }),
        ExportAllLearningPacksResultDto::Cancelled => Err(AppError::InvalidState(
            "the folder dialog was dismissed".into(),
        )),
    }
}

fn request() -> ExportAllLearningPacksRequestDto {
    ExportAllLearningPacksRequestDto {
        operation_id: id(),
        options: LearningPackExportOptionsDto {
            include_evidence: true,
            include_practical_artifacts: true,
            include_source_bodies: false,
            source_body_redistribution_confirmed: false,
        },
    }
}

async fn create(pool: &SqlitePool, title: &str) -> Result<LearningProgramDto> {
    let mut program = fixture();
    program.summary.title = title.into();
    LearningRepository::new(pool.clone())
        .create(&program)
        .await?;
    Ok(program)
}

fn files(dir: &Path) -> Result<Vec<String>> {
    let mut names: Vec<String> = std::fs::read_dir(dir)?
        .map(|entry| entry.map(|entry| entry.file_name().to_string_lossy().into_owned()))
        .collect::<std::io::Result<_>>()?;
    names.sort();
    Ok(names)
}

#[tokio::test]
async fn every_course_gets_one_file_and_failures_do_not_stop_the_rest() -> Result<()> {
    let pool = pool().await?;
    let repo = LearningRepository::new(pool.clone());
    let draft = create(&pool, "Draft: chemistry").await?;
    let active = create(&pool, "Statistics").await?;
    repo.accept(&AcceptLearningProgramRequestDto {
        program_id: active.summary.id.clone(),
        expected_revision: 0,
        title: active.summary.title.clone(),
    })
    .await?;
    let twin = create(&pool, "Statistics").await?;
    let busy = create(&pool, "Busy course").await?;
    let job = id();
    sqlx::query("INSERT INTO jobs(id,kind,subject_id,operation_id,payload_hash,status,requested_json,progress_total,progress_message,created_at) VALUES(?,'learning_lesson',?,?,'job-hash','running','{}',1,'Writing',1)")
        .bind(&job).bind(&busy.summary.id).bind(id()).execute(&pool).await?;
    sqlx::query("INSERT INTO learning_jobs(job_id,program_id) VALUES(?,?)")
        .bind(&job)
        .bind(&busy.summary.id)
        .execute(&pool)
        .await?;
    let mut empty = fixture();
    empty.summary.title = "Not outlined yet".into();
    empty.summary.current_lesson_id = None;
    empty.modules.clear();
    repo.create(&empty).await?;

    let app_data = tempfile::tempdir()?;
    let folder = tempfile::tempdir()?;
    let packs = packs(&pool, app_data.path());
    let result = done(packs.export_all(&request(), &picks(folder.path())).await?)?;
    assert_eq!(
        Some(Path::new(&result.destination_dir)),
        Some(std::fs::canonicalize(folder.path())?.as_path())
    );
    let mut exported: Vec<&str> = result
        .exported
        .iter()
        .map(|row| row.program_id.as_str())
        .collect();
    exported.sort();
    let mut expected = vec![
        draft.summary.id.as_str(),
        active.summary.id.as_str(),
        twin.summary.id.as_str(),
    ];
    expected.sort();
    assert_eq!(exported, expected);
    assert_eq!(
        files(folder.path())?,
        [
            "Draft chemistry.lattice-learning",
            "Statistics (2).lattice-learning",
            "Statistics.lattice-learning",
        ]
    );
    for row in &result.exported {
        let decoded = decode_learning_pack(&tokio::fs::read(&row.path).await?)?;
        assert_eq!(decoded.manifest.title, row.title);
    }
    let mut failed: Vec<(&str, &str)> = result
        .failed
        .iter()
        .map(|row| (row.program_id.as_str(), row.error.as_str()))
        .collect();
    failed.sort();
    let mut expected_failures = vec![
        (busy.summary.id.as_str(), "still being prepared"),
        (empty.summary.id.as_str(), "no modules yet"),
    ];
    expected_failures.sort();
    assert_eq!(failed.len(), 2);
    for ((program, error), (expected_program, reason)) in failed.iter().zip(&expected_failures) {
        assert_eq!(program, expected_program);
        assert!(error.contains(reason), "{error}");
        assert!(!error.starts_with("Invalid input"), "{error}");
    }
    Ok(())
}

#[tokio::test]
async fn a_retried_batch_replays_instead_of_writing_duplicates() -> Result<()> {
    let pool = pool().await?;
    create(&pool, "Geometry").await?;
    create(&pool, "Poetry").await?;
    let app_data = tempfile::tempdir()?;
    let folder = tempfile::tempdir()?;
    let packs = packs(&pool, app_data.path());

    // A completed batch answers from the ledger, before any dialog opens.
    let batch = request();
    // A dismissed dialog records nothing, so the same batch can run later.
    assert_eq!(
        packs.export_all(&batch, &Picks(None)).await?,
        ExportAllLearningPacksResultDto::Cancelled
    );
    let first = packs.export_all(&batch, &picks(folder.path())).await?;
    assert_eq!(packs.export_all(&batch, &Picks(None)).await?, first);
    let written = files(folder.path())?;
    assert_eq!(
        written,
        ["Geometry.lattice-learning", "Poetry.lattice-learning"]
    );

    // A batch interrupted before it was recorded replays course by course.
    let interrupted = request();
    let other = tempfile::tempdir()?;
    let partial = packs.export_all(&interrupted, &picks(other.path())).await?;
    forget_batch(&pool, &interrupted).await?;
    let retried = packs.export_all(&interrupted, &picks(other.path())).await?;
    assert_eq!(retried, partial);
    assert_eq!(files(other.path())?, written);
    Ok(())
}

/// The app quit before the batch was recorded, so a retry with the same
/// operation ID opens the folder dialog again.
async fn forget_batch(pool: &SqlitePool, batch: &ExportAllLearningPacksRequestDto) -> Result<()> {
    let payload_hash = hash(batch)?;
    Operation {
        id: &batch.operation_id,
        scope: "portability",
        kind: "export_all",
        program_id: None,
        subject_id: None,
        payload_hash: &payload_hash,
        conflict: "",
    }
    .forget(pool)
    .await
}

#[tokio::test]
async fn a_retry_into_another_folder_writes_fresh_packs_there() -> Result<()> {
    let pool = pool().await?;
    create(&pool, "Geometry").await?;
    create(&pool, "Poetry").await?;
    let app_data = tempfile::tempdir()?;
    let first_folder = tempfile::tempdir()?;
    let second_folder = tempfile::tempdir()?;
    let packs = packs(&pool, app_data.path());
    let batch = request();
    packs
        .export_all(&batch, &picks(first_folder.path()))
        .await?;
    let first_files = files(first_folder.path())?;
    forget_batch(&pool, &batch).await?;

    let retried = done(
        packs
            .export_all(&batch, &picks(second_folder.path()))
            .await?,
    )?;
    let second = std::fs::canonicalize(second_folder.path())?;
    assert_eq!(Path::new(&retried.destination_dir), second.as_path());
    assert!(retried.failed.is_empty(), "{:?}", retried.failed);
    assert_eq!(retried.exported.len(), 2);
    for row in &retried.exported {
        assert_eq!(Path::new(&row.path).parent(), Some(second.as_path()));
        decode_learning_pack(&tokio::fs::read(&row.path).await?)?;
    }
    assert_eq!(files(second_folder.path())?, first_files);
    assert_eq!(files(first_folder.path())?, first_files);
    Ok(())
}

#[tokio::test]
async fn a_retry_rewrites_a_pack_deleted_since_the_first_run() -> Result<()> {
    let pool = pool().await?;
    let geometry = create(&pool, "Geometry").await?;
    create(&pool, "Poetry").await?;
    let app_data = tempfile::tempdir()?;
    let folder = tempfile::tempdir()?;
    let packs = packs(&pool, app_data.path());
    let batch = request();
    let first = done(packs.export_all(&batch, &picks(folder.path())).await?)?;
    forget_batch(&pool, &batch).await?;
    let deleted = first
        .exported
        .iter()
        .find(|row| row.program_id == geometry.summary.id)
        .map(|row| row.path.clone())
        .expect("geometry was exported");
    tokio::fs::remove_file(&deleted).await?;

    let retried = done(packs.export_all(&batch, &picks(folder.path())).await?)?;
    assert!(retried.failed.is_empty(), "{:?}", retried.failed);
    assert_eq!(retried.exported, first.exported);
    decode_learning_pack(&tokio::fs::read(&deleted).await?)?;
    assert_eq!(
        files(folder.path())?,
        ["Geometry.lattice-learning", "Poetry.lattice-learning"]
    );
    // The rewritten pack replaces the stale record rather than adding one.
    let recorded: Vec<String> =
        sqlx::query_scalar("SELECT destination_path FROM learning_pack_exports WHERE program_id=?")
            .bind(&geometry.summary.id)
            .fetch_all(&pool)
            .await?;
    assert_eq!(recorded, [deleted]);
    Ok(())
}

#[tokio::test]
async fn no_courses_and_bad_folders() -> Result<()> {
    let pool = pool().await?;
    let app_data = tempfile::tempdir()?;
    let folder = tempfile::tempdir()?;
    let packs = packs(&pool, app_data.path());
    let result = done(packs.export_all(&request(), &picks(folder.path())).await?)?;
    assert!(result.exported.is_empty() && result.failed.is_empty());
    assert!(!result.destination_dir.is_empty());

    create(&pool, "Anything").await?;
    let inside = app_data.path().join("exports");
    tokio::fs::create_dir_all(&inside).await?;
    let error = packs
        .export_all(&request(), &picks(&inside))
        .await
        .expect_err("inside the app data folder");
    assert!(
        error
            .to_string()
            .contains("outside Lattice's own data folder"),
        "{error}"
    );
    let missing = folder.path().join("missing");
    assert!(packs
        .export_all(&request(), &picks(&missing))
        .await
        .is_err());
    assert!(files(&inside)?.is_empty());
    Ok(())
}
