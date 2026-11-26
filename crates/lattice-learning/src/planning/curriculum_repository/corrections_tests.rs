//! Tests for `learning.lesson`, `learning.module`, and `learning.outline` through `ContentEdits`.
use super::*;
use crate::{
    curriculum::{
        LearningCurriculumLesson, LearningCurriculumLessonState, LearningCurriculumOperation,
    },
    dto::{AcceptLearningProgramRequestDto, LearningProgramDto},
    edits::{
        content_edits,
        keys::{lesson_ref, module_ref, section_ref},
    },
    repository::LearningRepository,
    tests::{fixture, pool},
};
use lattice_core::shared::edits::{
    ContentEditRequest, EditOp, EditSource, Editability, FieldSpec, RevertContentEditRequest,
};
use lattice_core::shared::error::{AppError, Result};
use lattice_core::shared::persistence::Database;
use serde_json::json;

const OUTLINE_DRAFT_REASON: &str =
    "This course's outline is still a draft. You can edit it once the course has started.";

fn op_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

async fn active_program(database: &Database) -> Result<LearningProgramDto> {
    let repo = LearningRepository::new(database.clone());
    let program = fixture();
    repo.create(&program).await?;
    repo.accept(&AcceptLearningProgramRequestDto {
        program_id: program.summary.id.clone(),
        expected_revision: 0,
        title: program.summary.title.clone(),
    })
    .await?;
    repo.get(&program.summary.id).await
}

fn edit_request(
    content: lattice_core::shared::edits::ContentRef,
    expected_revision: String,
    ops: Vec<EditOp>,
) -> ContentEditRequest {
    ContentEditRequest {
        edit_id: op_id(),
        content,
        expected_revision,
        ops,
        source: EditSource::Learner,
        target: None,
    }
}

#[tokio::test]
async fn a_corrected_title_reaches_the_row_and_a_new_accepted_snapshot() -> Result<()> {
    let pool = pool().await?;
    let db = Database::new(pool.clone());
    let program = active_program(&db).await?;
    let curriculum = LearningCurriculumRepository::new(db.clone());
    let initial_plan = curriculum.plan(&program.summary.id).await?;
    let earlier_id = initial_plan.accepted_revision.as_ref().unwrap().id.clone();
    let earlier_required: i64 = sqlx::query_scalar(
        "SELECT required_lesson_count FROM learning_curriculum_revisions WHERE id=?",
    )
    .bind(&earlier_id)
    .fetch_one(&pool)
    .await?;
    let earlier_resume: Option<String> =
        sqlx::query_scalar("SELECT resume_lesson_id FROM learning_curriculum_revisions WHERE id=?")
            .bind(&earlier_id)
            .fetch_one(&pool)
            .await?;

    let lesson_id = program.modules[0].lessons[0].id.clone();
    let edits = content_edits(&db)?;
    let opened = edits.open(&lesson_ref(&lesson_id)).await;
    assert!(opened.is_ok(), "{opened:?}");
    let opened = opened.unwrap();

    let outcome = edits
        .apply(&edit_request(
            lesson_ref(&lesson_id),
            opened.revision,
            vec![EditOp::Replace {
                path: "/title".into(),
                value: json!("Clarified lesson title"),
            }],
        ))
        .await;
    assert!(outcome.is_ok(), "{outcome:?}");

    let stored_title: String = sqlx::query_scalar("SELECT title FROM learning_lessons WHERE id=?")
        .bind(&lesson_id)
        .fetch_one(&pool)
        .await?;
    assert_eq!(stored_title, "Clarified lesson title");

    let accepted_count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM learning_curriculum_revisions WHERE program_id=? AND status='accepted'",
    )
    .bind(&program.summary.id)
    .fetch_one(&pool)
    .await?;
    assert_eq!(accepted_count, 1);

    let total_count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM learning_curriculum_revisions WHERE program_id=?")
            .bind(&program.summary.id)
            .fetch_one(&pool)
            .await?;
    assert_eq!(total_count, 2);

    let earlier_status: String =
        sqlx::query_scalar("SELECT status FROM learning_curriculum_revisions WHERE id=?")
            .bind(&earlier_id)
            .fetch_one(&pool)
            .await?;
    assert_eq!(earlier_status, "superseded");

    let accepted_row = sqlx::query(
        "SELECT predecessor_id, required_lesson_count, resume_lesson_id, snapshot_json FROM learning_curriculum_revisions WHERE program_id=? AND status='accepted'",
    )
    .bind(&program.summary.id)
    .fetch_one(&pool)
    .await?;
    assert_eq!(
        accepted_row
            .get::<Option<String>, _>("predecessor_id")
            .as_deref(),
        Some(earlier_id.as_str())
    );
    assert_eq!(
        accepted_row.get::<i64, _>("required_lesson_count"),
        earlier_required
    );
    assert_eq!(
        accepted_row.get::<Option<String>, _>("resume_lesson_id"),
        earlier_resume
    );
    let snapshot: LearningCurriculumRevision =
        serde_json::from_str(accepted_row.get("snapshot_json")).unwrap();
    assert_eq!(
        snapshot.modules[0].lessons[0].title,
        "Clarified lesson title"
    );

    let program_revision: i64 =
        sqlx::query_scalar("SELECT revision FROM learning_programs WHERE id=?")
            .bind(&program.summary.id)
            .fetch_one(&pool)
            .await?;
    assert_eq!(program_revision, program.summary.revision + 1);
    Ok(())
}

#[tokio::test]
async fn a_corrected_title_survives_the_next_accepted_plan_revision() -> Result<()> {
    let pool = pool().await?;
    let db = Database::new(pool.clone());
    let program = active_program(&db).await?;
    let curriculum = LearningCurriculumRepository::new(db.clone());
    let lesson_id = program.modules[0].lessons[0].id.clone();

    let edits = content_edits(&db)?;
    let opened = edits.open(&lesson_ref(&lesson_id)).await;
    assert!(opened.is_ok(), "{opened:?}");
    let opened = opened.unwrap();
    let applied = edits
        .apply(&edit_request(
            lesson_ref(&lesson_id),
            opened.revision,
            vec![EditOp::Replace {
                path: "/title".into(),
                value: json!("Surviving lesson title"),
            }],
        ))
        .await;
    assert!(applied.is_ok(), "{applied:?}");

    let current = LearningRepository::new(db.clone())
        .get(&program.summary.id)
        .await?;
    let preview = curriculum
        .preview(&PreviewLearningCurriculumRevisionRequestDto {
            operation_id: op_id(),
            program_id: program.summary.id.clone(),
            expected_revision: current.summary.revision,
            reason: "Add a follow-up lesson".into(),
            operations: vec![LearningCurriculumOperation::AddLesson {
                module_id: program.modules[0].id.clone(),
                after_lesson_id: None,
                lesson: LearningCurriculumLesson {
                    id: op_id(),
                    title: "Added lesson".into(),
                    objective: "Apply the concept.".into(),
                    estimated_minutes: 15,
                    state: LearningCurriculumLessonState::Outline,
                    assessment_started: false,
                    replacement_lesson_id: None,
                },
            }],
        })
        .await?;
    curriculum
        .accept_revision(&LearningCurriculumRevisionActionRequestDto {
            operation_id: op_id(),
            program_id: program.summary.id.clone(),
            revision_id: preview.draft_revision.unwrap().id,
            expected_revision: current.summary.revision,
        })
        .await?;

    let stored_title: String = sqlx::query_scalar("SELECT title FROM learning_lessons WHERE id=?")
        .bind(&lesson_id)
        .fetch_one(&pool)
        .await?;
    assert_eq!(stored_title, "Surviving lesson title");
    Ok(())
}

#[tokio::test]
async fn a_corrected_title_survives_a_draft_made_before_it() -> Result<()> {
    let pool = pool().await?;
    let db = Database::new(pool.clone());
    let program = active_program(&db).await?;
    let curriculum = LearningCurriculumRepository::new(db.clone());
    let lesson_id = program.modules[0].lessons[0].id.clone();

    curriculum.plan(&program.summary.id).await?;
    let draft_plan = curriculum
        .preview(&PreviewLearningCurriculumRevisionRequestDto {
            operation_id: op_id(),
            program_id: program.summary.id.clone(),
            expected_revision: program.summary.revision,
            reason: "Add an extra lesson in draft".into(),
            operations: vec![LearningCurriculumOperation::AddLesson {
                module_id: program.modules[1].id.clone(),
                after_lesson_id: None,
                lesson: LearningCurriculumLesson {
                    id: op_id(),
                    title: "Draft-added lesson".into(),
                    objective: "Check the idea.".into(),
                    estimated_minutes: 20,
                    state: LearningCurriculumLessonState::Outline,
                    assessment_started: false,
                    replacement_lesson_id: None,
                },
            }],
        })
        .await?;
    let draft_id = draft_plan.draft_revision.unwrap().id;

    let edits = content_edits(&db)?;
    let opened = edits.open(&lesson_ref(&lesson_id)).await;
    assert!(opened.is_ok(), "{opened:?}");
    let opened = opened.unwrap();
    let applied = edits
        .apply(&edit_request(
            lesson_ref(&lesson_id),
            opened.revision,
            vec![EditOp::Replace {
                path: "/title".into(),
                value: json!("Corrected while draft stood"),
            }],
        ))
        .await;
    assert!(applied.is_ok(), "{applied:?}");

    let current = LearningRepository::new(db.clone())
        .get(&program.summary.id)
        .await?;
    curriculum
        .accept_revision(&LearningCurriculumRevisionActionRequestDto {
            operation_id: op_id(),
            program_id: program.summary.id.clone(),
            revision_id: draft_id,
            expected_revision: current.summary.revision,
        })
        .await?;

    let stored_title: String = sqlx::query_scalar("SELECT title FROM learning_lessons WHERE id=?")
        .bind(&lesson_id)
        .fetch_one(&pool)
        .await?;
    assert_eq!(stored_title, "Corrected while draft stood");
    Ok(())
}

#[tokio::test]
async fn a_course_without_a_snapshot_gets_one_before_the_correction() -> Result<()> {
    let pool = pool().await?;
    let db = Database::new(pool.clone());
    let program = active_program(&db).await?;
    let initial_rows: i64 =
        sqlx::query_scalar("SELECT count(*) FROM learning_curriculum_revisions WHERE program_id=?")
            .bind(&program.summary.id)
            .fetch_one(&pool)
            .await?;
    assert_eq!(initial_rows, 0);

    let lesson_id = program.modules[0].lessons[0].id.clone();
    let edits = content_edits(&db)?;
    let opened = edits.open(&lesson_ref(&lesson_id)).await;
    assert!(opened.is_ok(), "{opened:?}");
    let opened = opened.unwrap();
    let applied = edits
        .apply(&edit_request(
            lesson_ref(&lesson_id),
            opened.revision,
            vec![EditOp::Replace {
                path: "/title".into(),
                value: json!("Seeded then corrected"),
            }],
        ))
        .await;
    assert!(applied.is_ok(), "{applied:?}");

    let rows: Vec<(i64, String)> = sqlx::query_as(
        "SELECT revision, status FROM learning_curriculum_revisions WHERE program_id=? ORDER BY revision",
    )
    .bind(&program.summary.id)
    .fetch_all(&pool)
    .await?;
    assert_eq!(
        rows,
        vec![(1, "superseded".to_owned()), (2, "accepted".to_owned())]
    );
    Ok(())
}

#[tokio::test]
async fn a_title_edit_moves_the_outline_revision_and_a_section_edit_does_not() -> Result<()> {
    let pool = pool().await?;
    let db = Database::new(pool.clone());
    let program = active_program(&db).await?;
    let lesson_id = program.modules[0].lessons[0].id.clone();

    sqlx::query(
        "INSERT INTO learning_blocks(lesson_id, ordinal, kind, title, body, source_ids_json) VALUES(?, 0, 'explanation', 'Core idea', 'Original explanation text.', '[]')",
    )
    .bind(&lesson_id)
    .execute(&pool)
    .await?;
    sqlx::query("UPDATE learning_lessons SET preparation='ready' WHERE id=?")
        .bind(&lesson_id)
        .execute(&pool)
        .await?;

    let initial_outline: i64 =
        sqlx::query_scalar("SELECT content_revision FROM learning_programs WHERE id=?")
            .bind(&program.summary.id)
            .fetch_one(&pool)
            .await?;

    let edits = content_edits(&db)?;
    let opened_section = edits.open(&section_ref(&lesson_id, 0)).await;
    assert!(opened_section.is_ok(), "{opened_section:?}");
    let opened_section = opened_section.unwrap();
    let edited_section = edits
        .apply(&edit_request(
            section_ref(&lesson_id, 0),
            opened_section.revision,
            vec![EditOp::Replace {
                path: "/body".into(),
                value: json!("Updated explanation text."),
            }],
        ))
        .await;
    assert!(edited_section.is_ok(), "{edited_section:?}");

    let after_section: i64 =
        sqlx::query_scalar("SELECT content_revision FROM learning_programs WHERE id=?")
            .bind(&program.summary.id)
            .fetch_one(&pool)
            .await?;
    assert_eq!(after_section, initial_outline);

    let opened_lesson = edits.open(&lesson_ref(&lesson_id)).await;
    assert!(opened_lesson.is_ok(), "{opened_lesson:?}");
    let opened_lesson = opened_lesson.unwrap();
    let edited_lesson = edits
        .apply(&edit_request(
            lesson_ref(&lesson_id),
            opened_lesson.revision,
            vec![EditOp::Replace {
                path: "/title".into(),
                value: json!("Moved outline title"),
            }],
        ))
        .await;
    assert!(edited_lesson.is_ok(), "{edited_lesson:?}");

    let after_lesson: i64 =
        sqlx::query_scalar("SELECT content_revision FROM learning_programs WHERE id=?")
            .bind(&program.summary.id)
            .fetch_one(&pool)
            .await?;
    assert!(after_lesson > after_section);
    Ok(())
}

#[tokio::test]
async fn minutes_are_not_offered_for_a_completed_lesson() -> Result<()> {
    let pool = pool().await?;
    let db = Database::new(pool.clone());
    let program = active_program(&db).await?;
    let lesson_id = program.modules[0].lessons[0].id.clone();

    sqlx::query("UPDATE learning_lessons SET completed=1 WHERE id=?")
        .bind(&lesson_id)
        .execute(&pool)
        .await?;

    let edits = content_edits(&db)?;
    let opened = edits.open(&lesson_ref(&lesson_id)).await;
    assert!(opened.is_ok(), "{opened:?}");
    let opened = opened.unwrap();
    assert!(opened.document.get("estimatedMinutes").is_none());
    assert!(!opened.form.iter().any(|field| matches!(
        field,
        FieldSpec::Number { path, .. } if path == "/estimatedMinutes"
    )));
    assert!(opened.notes.contains(
        &"The time of a completed lesson, or of one with a started quiz or practice, can't change."
            .to_owned()
    ));

    let applied = edits
        .apply(&edit_request(
            lesson_ref(&lesson_id),
            opened.revision,
            vec![EditOp::Replace {
                path: "/title".into(),
                value: json!("Completed lesson corrected title"),
            }],
        ))
        .await;
    assert!(applied.is_ok(), "{applied:?}");

    let plan = LearningCurriculumRepository::new(db)
        .plan(&program.summary.id)
        .await?;
    assert_eq!(
        plan.accepted_revision.unwrap().modules[0].lessons[0].estimated_minutes,
        20
    );
    Ok(())
}

#[tokio::test]
async fn a_module_summary_is_the_snapshots_purpose() -> Result<()> {
    let pool = pool().await?;
    let db = Database::new(pool.clone());
    let program = active_program(&db).await?;
    let module_id = program.modules[0].id.clone();

    let edits = content_edits(&db)?;
    let opened = edits.open(&module_ref(&module_id)).await;
    assert!(opened.is_ok(), "{opened:?}");
    let opened = opened.unwrap();
    let applied = edits
        .apply(&edit_request(
            module_ref(&module_id),
            opened.revision,
            vec![EditOp::Replace {
                path: "/summary".into(),
                value: json!("Clarified module purpose."),
            }],
        ))
        .await;
    assert!(applied.is_ok(), "{applied:?}");

    let stored_summary: String =
        sqlx::query_scalar("SELECT summary FROM learning_modules WHERE id=?")
            .bind(&module_id)
            .fetch_one(&pool)
            .await?;
    assert_eq!(stored_summary, "Clarified module purpose.");

    let plan = LearningCurriculumRepository::new(db)
        .plan(&program.summary.id)
        .await?;
    assert_eq!(
        plan.accepted_revision.unwrap().modules[0].purpose,
        "Clarified module purpose."
    );
    Ok(())
}

#[tokio::test]
async fn reverting_a_title_writes_another_snapshot() -> Result<()> {
    let pool = pool().await?;
    let db = Database::new(pool.clone());
    let program = active_program(&db).await?;
    let curriculum = LearningCurriculumRepository::new(db.clone());
    curriculum.plan(&program.summary.id).await?;
    let lesson_id = program.modules[0].lessons[0].id.clone();
    let original_title = program.modules[0].lessons[0].title.clone();

    let edits = content_edits(&db)?;
    let opened = edits.open(&lesson_ref(&lesson_id)).await;
    assert!(opened.is_ok(), "{opened:?}");
    let opened = opened.unwrap();
    let applied = edits
        .apply(&edit_request(
            lesson_ref(&lesson_id),
            opened.revision,
            vec![EditOp::Replace {
                path: "/title".into(),
                value: json!("Temporarily renamed lesson"),
            }],
        ))
        .await;
    assert!(applied.is_ok(), "{applied:?}");
    let applied = applied.unwrap();

    let reverted = edits
        .revert(&RevertContentEditRequest {
            revert_id: op_id(),
            edit_id: applied.edit_id,
            expected_revision: applied.revision,
            and_later: false,
        })
        .await;
    assert!(reverted.is_ok(), "{reverted:?}");

    let stored_title: String = sqlx::query_scalar("SELECT title FROM learning_lessons WHERE id=?")
        .bind(&lesson_id)
        .fetch_one(&pool)
        .await?;
    assert_eq!(stored_title, original_title);

    let total_count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM learning_curriculum_revisions WHERE program_id=?")
            .bind(&program.summary.id)
            .fetch_one(&pool)
            .await?;
    assert_eq!(total_count, 3);
    let accepted_count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM learning_curriculum_revisions WHERE program_id=? AND status='accepted'",
    )
    .bind(&program.summary.id)
    .fetch_one(&pool)
    .await?;
    assert_eq!(accepted_count, 1);
    Ok(())
}

#[tokio::test]
async fn a_draft_courses_outline_is_not_editable() -> Result<()> {
    let pool = pool().await?;
    let db = Database::new(pool.clone());
    let program = fixture();
    LearningRepository::new(db.clone()).create(&program).await?;
    let lesson_id = program.modules[0].lessons[0].id.clone();
    let module_id = program.modules[0].id.clone();

    let edits = content_edits(&db)?;
    let opened_lesson = edits.open(&lesson_ref(&lesson_id)).await;
    assert!(opened_lesson.is_ok(), "{opened_lesson:?}");
    let opened_lesson = opened_lesson.unwrap();
    assert_eq!(
        opened_lesson.editable,
        Editability::NotNow {
            reason: OUTLINE_DRAFT_REASON.into(),
        }
    );

    let opened_module = edits.open(&module_ref(&module_id)).await;
    assert!(opened_module.is_ok(), "{opened_module:?}");
    assert_eq!(
        opened_module.unwrap().editable,
        Editability::NotNow {
            reason: OUTLINE_DRAFT_REASON.into(),
        }
    );

    let refused = edits
        .apply(&edit_request(
            lesson_ref(&lesson_id),
            opened_lesson.revision,
            vec![EditOp::Replace {
                path: "/title".into(),
                value: json!("Should not save on a draft"),
            }],
        ))
        .await;
    assert!(
        matches!(refused, Err(AppError::InvalidState(ref message)) if message == OUTLINE_DRAFT_REASON),
        "{refused:?}"
    );
    let revision_rows: i64 =
        sqlx::query_scalar("SELECT count(*) FROM learning_curriculum_revisions WHERE program_id=?")
            .bind(&program.summary.id)
            .fetch_one(&pool)
            .await?;
    assert_eq!(revision_rows, 0);
    Ok(())
}

#[tokio::test]
async fn saving_a_title_says_which_preparations_stop() -> Result<()> {
    let pool = pool().await?;
    let db = Database::new(pool.clone());
    let program = active_program(&db).await?;
    let curriculum = LearningCurriculumRepository::new(db.clone());
    let lesson_a = program.modules[0].lessons[0].id.clone();
    let lesson_b = program.modules[0].lessons[1].id.clone();

    let edits = content_edits(&db)?;
    let opened = edits.open(&lesson_ref(&lesson_a)).await;
    assert!(opened.is_ok(), "{opened:?}");
    let opened = opened.unwrap();

    let quiet = edits
        .preview(&edit_request(
            lesson_ref(&lesson_a),
            opened.revision.clone(),
            vec![EditOp::Replace {
                path: "/title".into(),
                value: json!("Edited without preparations"),
            }],
        ))
        .await;
    assert!(quiet.is_ok(), "{quiet:?}");
    assert!(quiet.unwrap().consequences.is_empty());

    for lesson_id in [&lesson_a, &lesson_b] {
        curriculum
            .start_lesson_job(&StartLearningGenerationJobRequestDto {
                operation_id: op_id(),
                program_id: program.summary.id.clone(),
                expected_revision: program.summary.revision,
                kind: LearningGenerationJobKind::LessonPreparation,
                request_json: json!({ "lessonIds": [lesson_id] }).to_string(),
                progress_total: 1,
            })
            .await?;
    }

    let warned = edits
        .preview(&edit_request(
            lesson_ref(&lesson_a),
            opened.revision,
            vec![EditOp::Replace {
                path: "/title".into(),
                value: json!("Edited while two preparations run"),
            }],
        ))
        .await;
    assert!(warned.is_ok(), "{warned:?}");
    let consequences = warned.unwrap().consequences;
    assert_eq!(consequences.len(), 1);
    assert_eq!(consequences[0].code, "outline.preparations_stop");
    assert_eq!(
        consequences[0].message,
        "2 lessons are being prepared from the outline as it is now. Saving stops them, and they will need to be started again."
    );
    Ok(())
}
