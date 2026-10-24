//! Quiz banks are course content: a course exported without learner
//! evidence still carries its banks, keys, blueprints and settings, and its
//! quizzes start after a copy or a replace without writing a question again.
use super::{id, packs, preview_request};
use crate::features::learning::{
    assessment::assessment_tests::fill_bank,
    assessment_repository::LearningAssessmentRepository,
    dto::{
        AcceptLearningProgramRequestDto, LearningQuizBankStatus, LearningQuizMode,
        LearningQuizScopeDto, LearningQuizScopeKind, StartLearningQuizRequestDto,
    },
    pack_repository::LearningPackRepository,
    portability_dto::{
        ApplyLearningPackImportRequestDto, ExportLearningPackRequestDto,
        LearningPackConflictPolicy, LearningPackExportOptionsDto,
    },
    repository::LearningRepository,
    tests::{fixture, pool, prepared},
};
use lattice_core::shared::error::Result;
use sqlx::SqlitePool;

/// Bank rows of one course: candidates, keys, blueprints, settings.
async fn bank_counts(pool: &SqlitePool, program_id: &str) -> Result<(i64, i64, i64, i64)> {
    let count = |sql: &'static str| async move {
        sqlx::query_scalar::<_, i64>(sql)
            .bind(program_id)
            .fetch_one(pool)
            .await
            .map_err(crate::features::learning::persistence::db)
    };
    Ok((
        count("SELECT count(*) FROM learning_assessment_candidates WHERE program_id=?").await?,
        count("SELECT count(*) FROM learning_assessment_answer_keys k JOIN learning_assessment_candidates c ON c.id=k.candidate_id WHERE c.program_id=?").await?,
        count("SELECT count(*) FROM learning_assessment_blueprints WHERE program_id=?").await?,
        count("SELECT count(*) FROM learning_quiz_settings WHERE program_id=?").await?,
    ))
}

async fn export_without_evidence(
    packs: &LearningPackRepository,
    program_id: &str,
) -> Result<String> {
    let exported = packs
        .export(&ExportLearningPackRequestDto {
            operation_id: id(),
            program_id: program_id.into(),
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

async fn import(
    packs: &LearningPackRepository,
    path: &str,
    policy: LearningPackConflictPolicy,
) -> Result<String> {
    let preview = packs.preview(&preview_request(path, policy)).await?;
    let row = preview.import_previews.first().expect("import preview");
    assert!(row.can_apply, "{:?}", row.conflicts);
    packs
        .apply(&ApplyLearningPackImportRequestDto {
            operation_id: id(),
            preview_id: row.id.clone(),
            expected_root_sha256: row.manifest.root_sha256.clone(),
        })
        .await?;
    Ok(row.incoming_program_id.clone())
}

/// Start the course's first lesson quiz: it must need no new questions.
async fn start_first_lesson_quiz(pool: &SqlitePool, program_id: &str) -> Result<usize> {
    let program = LearningRepository::new(pool.clone())
        .get(program_id)
        .await?;
    let lesson_id = program.modules[0].lessons[0].id.clone();
    let repo = LearningAssessmentRepository::new(pool.clone());
    let overview = repo.overview(program_id).await?;
    assert_eq!(
        overview.modules[0].lessons[0].quiz.bank_status,
        LearningQuizBankStatus::Ready
    );
    let form = repo
        .start_quiz(
            &StartLearningQuizRequestDto {
                operation_id: id(),
                form_id: id(),
                program_id: program_id.into(),
                scope: LearningQuizScopeDto {
                    kind: LearningQuizScopeKind::Lesson,
                    id: Some(lesson_id),
                },
                mode: LearningQuizMode::Graded,
                retake_of_form_id: None,
            },
            "start-imported-quiz",
        )
        .await?;
    Ok(form.items.len())
}

#[tokio::test]
async fn a_course_exported_without_evidence_keeps_its_quiz_banks() -> Result<()> {
    let pool = pool().await?;
    let program = fixture();
    let programs = LearningRepository::new(pool.clone());
    programs.create(&program).await?;
    programs
        .accept(&AcceptLearningProgramRequestDto {
            program_id: program.summary.id.clone(),
            expected_revision: 0,
            title: program.summary.title.clone(),
        })
        .await?;
    let program_id = program.summary.id.clone();
    let lesson_id = program.modules[0].lessons[0].id.clone();
    programs
        .prepare(&program_id, &lesson_id, 1, &prepared(&lesson_id))
        .await?;
    let asked = fill_bank(
        &pool,
        &program_id,
        &LearningQuizScopeDto {
            kind: LearningQuizScopeKind::Lesson,
            id: Some(lesson_id.clone()),
        },
    )
    .await?;
    sqlx::query("INSERT INTO learning_quiz_settings(program_id,quiz_pass,test_pass,exam_pass,written_answers,updated_at) VALUES(?,0.8,0.75,0.75,1,1)")
        .bind(&program_id)
        .execute(&pool)
        .await
        .map_err(crate::features::learning::persistence::db)?;
    let before = bank_counts(&pool, &program_id).await?;
    assert!(before.0 > 0 && before.0 == before.1 && before.2 > 0 && before.3 == 1);

    let workspace = tempfile::tempdir().expect("pack workspace");
    let packs = packs(&pool, workspace.path());
    let path = export_without_evidence(&packs, &program_id).await?;

    // A copy arrives with every bank row and starts its quiz at once.
    let copy = import(&packs, &path, LearningPackConflictPolicy::CreateCopy).await?;
    assert_ne!(copy, program_id);
    assert_eq!(bank_counts(&pool, &copy).await?, before);
    assert_eq!(start_first_lesson_quiz(&pool, &copy).await?, asked);

    // Replacing the course from the same pack keeps its banks too.
    let replaced = import(
        &packs,
        &path,
        LearningPackConflictPolicy::ReplaceAfterBackup,
    )
    .await?;
    assert_eq!(replaced, program_id);
    assert_eq!(bank_counts(&pool, &program_id).await?, before);
    assert_eq!(start_first_lesson_quiz(&pool, &program_id).await?, asked);
    Ok(())
}
