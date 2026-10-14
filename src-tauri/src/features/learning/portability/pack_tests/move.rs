//! Moving a studied course to another computer and back. Everything the
//! learner resumes from must arrive exactly as it left: the current lesson,
//! each lesson's completion and curriculum state, every quiz and test
//! attempt, and the flashcard schedule with its review log.
use super::{move_request, packs, preview_request, repack};
use crate::features::learning::{
    dto::{
        CompleteLearningLessonRequestDto, LearningAnswerDto, LearningAssessmentKind,
        LearningProgramDto, SubmitLearningAttemptRequestDto,
    },
    pack::decode_learning_pack,
    pack_repository::LearningPackRepository,
    portability_dto::{
        ApplyLearningPackImportRequestDto, LearningPackConflictKind, LearningPackConflictPolicy,
        LearningPackImportPreviewDto, LearningPackStaysBehindKind, LearningPortabilityWorkspaceDto,
        PreviewLearningPackImportRequestDto,
    },
    repository::LearningRepository,
    tests::{pool, ready_program},
};
use crate::shared::error::Result;
use sqlx::SqlitePool;

type LessonRow = (String, i64, String, Option<String>);
type AttemptRow = (
    String,
    String,
    Option<String>,
    String,
    i64,
    i64,
    String,
    String,
    i64,
);
type CardRow = (
    String,
    Option<f64>,
    Option<f64>,
    i64,
    Option<i64>,
    i64,
    i64,
    i64,
);
type ReviewRow = (
    String,
    String,
    i64,
    String,
    String,
    Option<i64>,
    Option<i64>,
);

/// Everything the learner resumes from, read column by column.
#[derive(Debug, PartialEq)]
struct ResumeState {
    current_lesson_id: Option<String>,
    lessons: Vec<LessonRow>,
    attempts: Vec<AttemptRow>,
    cards: Vec<CardRow>,
    reviews: Vec<ReviewRow>,
}

async fn resume_state(pool: &SqlitePool, program_id: &str) -> Result<ResumeState> {
    let current_lesson_id =
        sqlx::query_scalar("SELECT current_lesson_id FROM learning_programs WHERE id=?")
            .bind(program_id)
            .fetch_one(pool)
            .await?;
    let lessons = sqlx::query_as("SELECT id,completed,curriculum_state,replacement_lesson_id FROM learning_lessons WHERE program_id=? ORDER BY id")
        .bind(program_id).fetch_all(pool).await?;
    let attempts = sqlx::query_as("SELECT id,module_id,lesson_id,kind,correct,total,answers_json,results_json,submitted_at FROM learning_attempts WHERE program_id=? ORDER BY id")
        .bind(program_id).fetch_all(pool).await?;
    let cards = sqlx::query_as("SELECT c.id,c.stability,c.difficulty,c.due_at,c.last_reviewed_at,c.interval_days,c.review_count,c.lapses FROM study_cards c JOIN learning_memory m ON m.deck_id=c.deck_id WHERE m.program_id=? ORDER BY c.id")
        .bind(program_id).fetch_all(pool).await?;
    let reviews = sqlx::query_as("SELECT r.id,r.card_id,r.reviewed_at,r.rating,r.mode,r.correct,r.selected_option FROM study_reviews r JOIN study_cards c ON c.id=r.card_id JOIN learning_memory m ON m.deck_id=c.deck_id WHERE m.program_id=? ORDER BY r.id")
        .bind(program_id).fetch_all(pool).await?;
    Ok(ResumeState {
        current_lesson_id,
        lessons,
        attempts,
        cards,
        reviews,
    })
}

async fn complete(pool: &SqlitePool, program_id: &str, lesson_id: &str) -> Result<()> {
    let repo = LearningRepository::new(pool.clone());
    let revision = repo.get(program_id).await?.summary.revision;
    repo.complete(&CompleteLearningLessonRequestDto {
        program_id: program_id.into(),
        lesson_id: lesson_id.into(),
        expected_revision: revision,
    })
    .await
}

/// Answer every question of `kind` in the module (a test) or one lesson.
async fn submit(
    pool: &SqlitePool,
    program_id: &str,
    module: usize,
    lesson: Option<usize>,
    kind: LearningAssessmentKind,
) -> Result<()> {
    let repo = LearningRepository::new(pool.clone());
    let program: LearningProgramDto = repo.get(program_id).await?;
    let module = &program.modules[module];
    let answers = module
        .lessons
        .iter()
        .enumerate()
        .filter(|(index, _)| lesson.is_none_or(|lesson| lesson == *index))
        .flat_map(|(_, lesson)| lesson.questions.iter())
        .filter(|question| question.kind == kind)
        .enumerate()
        .map(|(index, question)| LearningAnswerDto {
            question_id: question.id.clone(),
            // Alternate right and wrong answers so the results differ.
            selected_index: index % 2,
        })
        .collect();
    repo.submit(&SubmitLearningAttemptRequestDto {
        attempt_id: super::id(),
        program_id: program_id.into(),
        expected_revision: program.summary.revision,
        module_id: module.id.clone(),
        lesson_id: lesson.map(|index| module.lessons[index].id.clone()),
        kind,
        answers,
    })
    .await?;
    Ok(())
}

/// One FSRS review: the log row and the schedule it produced.
struct Review<'a> {
    card_id: &'a str,
    reviewed_at: i64,
    rating: &'a str,
    stability: f64,
    difficulty: f64,
    due_at: i64,
    interval_days: i64,
}

async fn review(pool: &SqlitePool, review: Review<'_>) -> Result<()> {
    sqlx::query("INSERT INTO study_reviews(id,card_id,reviewed_at,rating,mode,correct,selected_option) VALUES(?,?,?,?,'flashcard',1,NULL)")
        .bind(super::id()).bind(review.card_id).bind(review.reviewed_at).bind(review.rating)
        .execute(pool).await?;
    sqlx::query("UPDATE study_cards SET stability=?,difficulty=?,due_at=?,last_reviewed_at=?,interval_days=?,review_count=review_count+1 WHERE id=?")
        .bind(review.stability).bind(review.difficulty).bind(review.due_at).bind(review.reviewed_at)
        .bind(review.interval_days).bind(review.card_id)
        .execute(pool).await?;
    Ok(())
}

/// A course part-way through: two lessons done, one skipped, a quiz and a
/// module test taken, a lesson note written and two flashcards reviewed.
async fn studied_course(pool: &SqlitePool) -> Result<String> {
    let repo = LearningRepository::new(pool.clone());
    let program = ready_program(&repo).await?;
    let id = program.summary.id.clone();
    let lessons: Vec<String> = program
        .modules
        .iter()
        .flat_map(|module| module.lessons.iter().map(|lesson| lesson.id.clone()))
        .collect();
    complete(pool, &id, &lessons[0]).await?;
    complete(pool, &id, &lessons[1]).await?;
    sqlx::query(
        "UPDATE learning_lessons SET curriculum_state='skipped' WHERE program_id=? AND id=?",
    )
    .bind(&id)
    .bind(&lessons[3])
    .execute(pool)
    .await?;
    submit(pool, &id, 0, Some(0), LearningAssessmentKind::Quiz).await?;
    submit(pool, &id, 0, None, LearningAssessmentKind::Test).await?;
    // The note creates the course's notebook and flashcard deck.
    repo.ensure_lesson_note(&id, &lessons[0], "Lesson notes", "My own words")
        .await?;
    let deck: String = sqlx::query_scalar("SELECT deck_id FROM learning_memory WHERE program_id=?")
        .bind(&id)
        .fetch_one(pool)
        .await?;
    for card in ["card-a", "card-b"] {
        let card_id = format!("{id}-{card}");
        sqlx::query("INSERT INTO study_cards(id,deck_id,question,answer,options_json,correct_index,explanation,source_json,topic,due_at,interval_days,review_count,lapses,format) VALUES(?,?,'Why?','Because','[]',0,'Explained','{}','topic',0,0,0,1,'question_answer')")
            .bind(&card_id).bind(&deck).execute(pool).await?;
    }
    let card_a = format!("{id}-card-a");
    let card_b = format!("{id}-card-b");
    review(
        pool,
        Review {
            card_id: &card_a,
            reviewed_at: 1_791_000_000_123,
            rating: "good",
            stability: 3.173_092_418_6,
            difficulty: 5.282_434_119,
            due_at: 1_791_259_200_123,
            interval_days: 3,
        },
    )
    .await?;
    review(
        pool,
        Review {
            card_id: &card_a,
            reviewed_at: 1_791_259_300_456,
            rating: "hard",
            stability: 7.912_345_678_9,
            difficulty: 6.020_000_000_1,
            due_at: 1_791_864_100_456,
            interval_days: 7,
        },
    )
    .await?;
    review(
        pool,
        Review {
            card_id: &card_b,
            reviewed_at: 1_791_000_500_789,
            rating: "again",
            stability: 0.402_25,
            difficulty: 7.1,
            due_at: 1_791_000_800_789,
            interval_days: 0,
        },
    )
    .await?;
    Ok(id)
}

async fn export_pack(packs: &LearningPackRepository, program_id: &str) -> Result<String> {
    let workspace = packs.export(&move_request(program_id)).await?;
    Ok(workspace.exports[0].destination_path.clone())
}

/// The preview `request` created. Previews are listed newest first by
/// millisecond, so an earlier preview of the same course can tie with it.
fn preview_for<'a>(
    workspace: &'a LearningPortabilityWorkspaceDto,
    request: &PreviewLearningPackImportRequestDto,
) -> &'a LearningPackImportPreviewDto {
    workspace
        .import_previews
        .iter()
        .find(|preview| preview.id == request.preview_id)
        .expect("the requested preview")
}

async fn import(
    packs: &LearningPackRepository,
    path: &str,
    policy: LearningPackConflictPolicy,
) -> Result<LearningPortabilityWorkspaceDto> {
    let request = preview_request(path, policy);
    let previewed = packs.preview(&request).await?;
    let preview = preview_for(&previewed, &request);
    assert!(preview.can_apply, "{:?}", preview.conflicts);
    packs
        .apply(&ApplyLearningPackImportRequestDto {
            operation_id: super::id(),
            preview_id: preview.id.clone(),
            expected_root_sha256: preview.manifest.root_sha256.clone(),
        })
        .await
}

async fn note_link(pool: &SqlitePool, program_id: &str) -> Result<Vec<(String, String)>> {
    Ok(sqlx::query_as("SELECT lesson_id,note_id FROM learning_lesson_note_links WHERE program_id=? ORDER BY lesson_id")
        .bind(program_id).fetch_all(pool).await?)
}

#[tokio::test]
async fn merge_safe_moves_a_studied_course_to_a_fresh_computer_exactly() -> Result<()> {
    let home = pool().await?;
    let course = studied_course(&home).await?;
    let home_dir = tempfile::tempdir()?;
    let home_packs = packs(&home, home_dir.path());
    let notice = home_packs.workspace(&course).await?.export_notice;
    assert!(notice
        .stays_behind
        .iter()
        .any(|item| item.kind == LearningPackStaysBehindKind::NotebookNotes && item.count == 1));
    assert!(notice.blockers.is_empty());
    let pack = export_pack(&home_packs, &course).await?;

    let fresh = pool().await?;
    assert!(LearningRepository::new(fresh.clone())
        .list()
        .await?
        .is_empty());
    let fresh_dir = tempfile::tempdir()?;
    let fresh_packs = packs(&fresh, fresh_dir.path());
    let request = preview_request(&pack, LearningPackConflictPolicy::MergeSafe);
    let previewed = fresh_packs.preview(&request).await?;
    let preview = preview_for(&previewed, &request);
    assert_eq!(preview.incoming_program_id, course);
    assert!(preview.conflicts.is_empty(), "{:?}", preview.conflicts);
    let applied = import(&fresh_packs, &pack, LearningPackConflictPolicy::MergeSafe).await?;
    assert_eq!(applied.program_id, course);
    assert_eq!(applied.imports[0].imported_program_id, course);

    let left = resume_state(&home, &course).await?;
    // Guard against comparing two empty states.
    assert!(left.current_lesson_id.is_some());
    assert_eq!(
        left.lessons.iter().filter(|lesson| lesson.1 == 1).count(),
        2
    );
    assert!(left.lessons.iter().any(|lesson| lesson.2 == "skipped"));
    assert_eq!(left.attempts.len(), 2);
    assert!(left
        .attempts
        .iter()
        .any(|attempt| attempt.3 == "test" && attempt.5 == 2));
    assert_eq!(left.cards.len(), 2);
    assert_eq!(left.reviews.len(), 3);
    assert_eq!(resume_state(&fresh, &course).await?, left);
    // Notes stay in the first computer's notebook.
    assert!(note_link(&fresh, &course).await?.is_empty());
    Ok(())
}

#[tokio::test]
async fn replace_moves_newer_progress_back_and_keeps_the_lesson_note() -> Result<()> {
    let home = pool().await?;
    let course = studied_course(&home).await?;
    let home_dir = tempfile::tempdir()?;
    let home_packs = packs(&home, home_dir.path());
    let title = LearningRepository::new(home.clone())
        .get(&course)
        .await?
        .summary
        .title;
    let note = note_link(&home, &course).await?;
    assert_eq!(note.len(), 1);
    let journal: Option<String> =
        sqlx::query_scalar("SELECT journal_id FROM learning_memory WHERE program_id=?")
            .bind(&course)
            .fetch_one(&home)
            .await?;
    assert!(journal.is_some());
    let outbound = export_pack(&home_packs, &course).await?;

    // Keep studying on the second computer.
    let away = pool().await?;
    let away_dir = tempfile::tempdir()?;
    let away_packs = packs(&away, away_dir.path());
    import(
        &away_packs,
        &outbound,
        LearningPackConflictPolicy::MergeSafe,
    )
    .await?;
    let lessons: Vec<String> = sqlx::query_scalar("SELECT l.id FROM learning_lessons l JOIN learning_modules m ON m.id=l.module_id WHERE l.program_id=? ORDER BY m.ordinal,l.ordinal")
        .bind(&course).fetch_all(&away).await?;
    complete(&away, &course, &lessons[2]).await?;
    submit(&away, &course, 1, Some(0), LearningAssessmentKind::Quiz).await?;
    review(
        &away,
        Review {
            card_id: &format!("{course}-card-b"),
            reviewed_at: 1_791_100_000_001,
            rating: "good",
            stability: 2.5,
            difficulty: 6.75,
            due_at: 1_791_300_000_001,
            interval_days: 2,
        },
    )
    .await?;
    let newer = resume_state(&away, &course).await?;
    let older = resume_state(&home, &course).await?;
    assert_ne!(newer, older);
    assert_eq!(newer.attempts.len(), 3);
    assert_eq!(newer.reviews.len(), 4);
    let inbound = export_pack(&away_packs, &course).await?;

    // Back home the course already exists: moving it as-is is blocked.
    let request = preview_request(&inbound, LearningPackConflictPolicy::MergeSafe);
    let blocked = home_packs.preview(&request).await?;
    let blocked = preview_for(&blocked, &request);
    assert!(!blocked.can_apply);
    assert!(blocked.conflicts.iter().any(|conflict| conflict.kind
        == LearningPackConflictKind::Program
        && !conflict
            .clears_with
            .contains(&LearningPackConflictPolicy::MergeSafe)
        && conflict.existing_title.as_deref() == Some(&*title)));

    let applied = import(
        &home_packs,
        &inbound,
        LearningPackConflictPolicy::ReplaceAfterBackup,
    )
    .await?;
    assert_eq!(applied.program_id, course);
    assert_eq!(resume_state(&home, &course).await?, newer);
    // The note is still attached, in the same notebook.
    assert_eq!(note_link(&home, &course).await?, note);
    let restored_journal: Option<String> =
        sqlx::query_scalar("SELECT journal_id FROM learning_memory WHERE program_id=?")
            .bind(&course)
            .fetch_one(&home)
            .await?;
    assert_eq!(restored_journal, journal);
    let note_body: String =
        sqlx::query_scalar("SELECT content FROM daily_notes_workspace WHERE id=?")
            .bind(&note[0].1)
            .fetch_one(&home)
            .await?;
    assert_eq!(note_body, "My own words");

    // The replaced copy was saved first and still reads as a pack.
    let backup_id = applied.imports[0].backup_id.clone().expect("backup");
    let backup = applied
        .exports
        .iter()
        .find(|export| export.id == backup_id)
        .expect("backup is listed");
    let decoded = decode_learning_pack(&tokio::fs::read(&backup.destination_path).await?)?;
    let saved: LearningProgramDto =
        serde_json::from_slice(&decoded.entries["program/program.json"])?;
    assert_eq!(saved.summary.current_lesson_id, older.current_lesson_id);
    let completed: Vec<bool> = saved
        .modules
        .iter()
        .flat_map(|module| module.lessons.iter().map(|lesson| lesson.completed))
        .collect();
    assert_eq!(completed, [true, true, false, false]);
    Ok(())
}

async fn add_job(pool: &SqlitePool, program_id: &str) -> Result<String> {
    let job = super::id();
    sqlx::query("INSERT INTO jobs(id,kind,subject_id,operation_id,payload_hash,status,requested_json,progress_total,progress_message,created_at) VALUES(?,'learning_lesson',?,?,'job-hash','pending','{}',1,'Queued',1)")
        .bind(&job).bind(program_id).bind(super::id()).execute(pool).await?;
    sqlx::query("INSERT INTO learning_jobs(job_id,program_id) VALUES(?,?)")
        .bind(&job)
        .bind(program_id)
        .execute(pool)
        .await?;
    Ok(job)
}

#[tokio::test]
async fn replace_and_export_wait_for_a_pending_generation_job() -> Result<()> {
    let home = pool().await?;
    let course = studied_course(&home).await?;
    let home_dir = tempfile::tempdir()?;
    let home_packs = packs(&home, home_dir.path());
    let pack = export_pack(&home_packs, &course).await?;

    let job = add_job(&home, &course).await?;
    assert!(home_packs
        .workspace(&course)
        .await?
        .export_notice
        .blockers
        .contains(&LearningPackConflictKind::ProgramJobs));
    let refused = home_packs
        .export(&move_request(&course))
        .await
        .expect_err("export waits for the job");
    assert!(
        refused.to_string().contains("still being prepared"),
        "{refused}"
    );
    let request = preview_request(&pack, LearningPackConflictPolicy::ReplaceAfterBackup);
    let previewed = home_packs.preview(&request).await?;
    let preview = preview_for(&previewed, &request);
    assert!(!preview.can_apply);
    assert!(preview.conflicts.iter().any(|conflict| conflict.kind
        == LearningPackConflictKind::ProgramJobs
        && conflict.clears_with == [LearningPackConflictPolicy::CreateCopy]));

    // The job finishes; a fresh preview may proceed, but a job queued after
    // it is caught again under the replacement lock.
    sqlx::query("UPDATE jobs SET status='completed',finished_at=2,result_ref='done' WHERE id=?")
        .bind(&job)
        .execute(&home)
        .await?;
    let request = preview_request(&pack, LearningPackConflictPolicy::ReplaceAfterBackup);
    let previewed = home_packs.preview(&request).await?;
    let preview = preview_for(&previewed, &request);
    assert!(preview.can_apply, "{:?}", preview.conflicts);
    let late_job = add_job(&home, &course).await?;
    let before = resume_state(&home, &course).await?;
    let error = home_packs
        .apply(&ApplyLearningPackImportRequestDto {
            operation_id: super::id(),
            preview_id: preview.id.clone(),
            expected_root_sha256: preview.manifest.root_sha256.clone(),
        })
        .await
        .expect_err("apply re-checks the job under its lock");
    assert!(
        error.to_string().contains("still being prepared"),
        "{error}"
    );
    assert_eq!(resume_state(&home, &course).await?, before);
    let job_rows: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM learning_jobs WHERE job_id=?")
        .bind(&late_job)
        .fetch_one(&home)
        .await?;
    assert_eq!(job_rows, 1);
    let imports: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM learning_pack_imports")
        .fetch_one(&home)
        .await?;
    assert_eq!(imports, 0);
    Ok(())
}

#[tokio::test]
async fn a_pack_from_a_different_schema_is_blocked_at_preview() -> Result<()> {
    let home = pool().await?;
    let course = studied_course(&home).await?;
    let home_dir = tempfile::tempdir()?;
    let home_packs = packs(&home, home_dir.path());
    let pack = export_pack(&home_packs, &course).await?;
    let decoded = decode_learning_pack(&tokio::fs::read(&pack).await?)?;
    let other_build = repack(&decoded, |input| input.schema_version += 1)?;
    let other_path = home_dir.path().join("other-build.lattice-learning");
    tokio::fs::write(&other_path, other_build).await?;

    let fresh = pool().await?;
    let fresh_dir = tempfile::tempdir()?;
    let fresh_packs = packs(&fresh, fresh_dir.path());
    for policy in [
        LearningPackConflictPolicy::MergeSafe,
        LearningPackConflictPolicy::ReplaceAfterBackup,
        LearningPackConflictPolicy::CreateCopy,
    ] {
        let request = preview_request(&other_path.to_string_lossy(), policy);
        let previewed = fresh_packs.preview(&request).await?;
        let preview = preview_for(&previewed, &request);
        assert!(!preview.can_apply);
        assert_eq!(preview.conflicts.len(), 1);
        assert_eq!(
            preview.conflicts[0].kind,
            LearningPackConflictKind::PackSchema
        );
        assert!(preview.conflicts[0].clears_with.is_empty());
        // A copy would get a fresh identity; the others keep the pack's.
        if !policy.remaps_ids() {
            assert_eq!(preview.incoming_program_id, course);
        }
        assert!(preview.conflicts[0]
            .reason
            .as_deref()
            .is_some_and(|reason| reason.contains("different build of Lattice")));
        let error = fresh_packs
            .apply(&ApplyLearningPackImportRequestDto {
                operation_id: super::id(),
                preview_id: preview.id.clone(),
                expected_root_sha256: preview.manifest.root_sha256.clone(),
            })
            .await
            .expect_err("a blocked preview cannot be applied");
        assert!(
            error.to_string().contains("unresolved conflicts"),
            "{error}"
        );
    }
    assert!(LearningRepository::new(fresh.clone())
        .list()
        .await?
        .is_empty());
    Ok(())
}
