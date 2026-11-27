#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic
)]
//! The `learning.question` content kind and `learning.quiz_bank` reactor:
//! candidate-ID identity, choice editing, key retention, validation,
//! open-quiz gate, bank status revert, and section-quote consequences.
use super::claim_ledger_tests::{figure, lesson, unreadable, Course};
use super::GOOD;
use crate::assessment::assessment_tests::fill_bank;
use crate::assessment_engine::{
    LearningAssessmentCandidate, LearningAssessmentKey, LearningItemFormat, LearningTextKey,
};
use crate::assessment_repository::{insert_bank_items_in, BankItem, LearningAssessmentRepository};
use crate::dto::{
    AcceptLearningProgramRequestDto, LearningQuizMode, LearningQuizScopeDto, LearningQuizScopeKind,
    StartLearningQuizRequestDto,
};
use crate::edits::keys;
use crate::repository::LearningRepository;
use lattice_core::shared::edits::{
    ContentEditRequest, EditOp, EditSource, Editability, FieldSpec, RevertContentEditRequest,
};
use lattice_core::shared::error::{AppError, Result};
use lattice_core::shared::persistence::Database;
use serde_json::json;
use uuid::Uuid;

fn single_choice(outcome_id: &str, prompt: &str, options: &[&str], right: usize) -> BankItem {
    let candidate = LearningAssessmentCandidate {
        id: Uuid::new_v4().to_string(),
        outcome_ids: vec![outcome_id.into()],
        format: LearningItemFormat::SingleChoice,
        difficulty: 3,
        prompt: prompt.into(),
        options: options.iter().map(|s| (*s).to_owned()).collect(),
        left: Vec::new(),
        source_version_ids: Vec::new(),
        rubric: Vec::new(),
        key: LearningAssessmentKey::Choice(right),
        explanation: "Headers preserve whitespace unless trimmed explicitly.".into(),
        distractor_rationales: (0..options.len())
            .map(|index| {
                if index == right {
                    String::new()
                } else {
                    match index {
                        0 => "Leading whitespace is not trimmed automatically.",
                        1 => "Whitespace is not stripped automatically.",
                        2 => "Columns are kept intact.",
                        _ => "Spaces in headers are valid.",
                    }
                    .into()
                }
            })
            .collect(),
    };
    BankItem {
        item: candidate.into(),
        source_quote: String::new(),
        lesson_id: None,
        origin: "lesson",
        review_status: crate::assessment_repository::CandidateReviewStatus::Agreed,
    }
}

async fn course_with_questions() -> Result<(Course, String, Vec<String>)> {
    let course = Course::new().await?;
    let prepared = course
        .prepare(&unreadable(), &lesson(&[GOOD, &figure(7)]))
        .await
        .expect("the lesson publishes");
    course.publish(prepared).await?;
    let outcome = Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO learning_outcome_definitions(id,program_id,module_id,lesson_id,title,description,ordinal,created_at) VALUES(?,?,NULL,?,?,?,0,0)",
    )
    .bind(&outcome)
    .bind(&course.program_id)
    .bind(&course.lesson_id)
    .bind("CSV headers")
    .bind("Read CSV headers cleanly")
    .execute(&course.pool)
    .await?;

    let mut q0 = single_choice(
        &outcome,
        "How does DictReader handle spaces in column names?",
        &[
            "It keeps them.",
            "It trims them.",
            "It drops the column.",
            "It raises an error.",
        ],
        0,
    );
    q0.lesson_id = Some(course.lesson_id.clone());

    let mut q1 = single_choice(
        &outcome,
        "Which header behavior holds when a row is too short?",
        &[
            "Missing fields become None.",
            "The row is skipped.",
            "An exception is raised.",
            "Missing fields become empty strings.",
        ],
        0,
    );
    q1.lesson_id = Some(course.lesson_id.clone());

    let tf = BankItem {
        item: LearningAssessmentCandidate {
            id: Uuid::new_v4().to_string(),
            outcome_ids: vec![outcome.clone()],
            format: LearningItemFormat::TrueFalse,
            difficulty: 2,
            prompt: "DictReader returns every row as a list.".into(),
            options: vec!["True".into(), "False".into()],
            left: Vec::new(),
            source_version_ids: Vec::new(),
            rubric: Vec::new(),
            key: LearningAssessmentKey::Choice(1),
            explanation: "It yields each row mapped by header names.".into(),
            distractor_rationales: Vec::new(),
        }
        .into(),
        source_quote: String::new(),
        lesson_id: Some(course.lesson_id.clone()),
        origin: "lesson",
        review_status: crate::assessment_repository::CandidateReviewStatus::Agreed,
    };

    let short = BankItem {
        item: LearningAssessmentCandidate {
            id: Uuid::new_v4().to_string(),
            outcome_ids: vec![outcome.clone()],
            format: LearningItemFormat::ShortAnswer,
            difficulty: 2,
            prompt: "Which attribute lists header column names?".into(),
            options: Vec::new(),
            left: Vec::new(),
            source_version_ids: Vec::new(),
            rubric: Vec::new(),
            key: LearningAssessmentKey::Text(LearningTextKey {
                variants: vec!["fieldnames".into()],
                numeric: None,
            }),
            explanation: "The fieldnames attribute stores the parsed column headers.".into(),
            distractor_rationales: Vec::new(),
        }
        .into(),
        source_quote: String::new(),
        lesson_id: Some(course.lesson_id.clone()),
        origin: "lesson",
        review_status: crate::assessment_repository::CandidateReviewStatus::Agreed,
    };

    let ids: Vec<String> = [&q0, &q1, &tf, &short]
        .into_iter()
        .map(|b| b.item.id.clone())
        .collect();
    let mut tx = course.db.begin_write().await?;
    insert_bank_items_in(&mut tx, &course.program_id, &[q0, q1, tf, short], None).await?;
    tx.commit().await?;
    Ok((course, outcome, ids))
}

fn edit_req(
    content: lattice_core::shared::edits::ContentRef,
    expected_revision: String,
    ops: Vec<EditOp>,
) -> ContentEditRequest {
    ContentEditRequest {
        edit_id: Uuid::new_v4().to_string(),
        content,
        expected_revision,
        ops,
        source: EditSource::Learner,
        target: None,
    }
}

fn field_paths(form: &[FieldSpec]) -> Vec<&str> {
    form.iter()
        .map(|spec| match spec {
            FieldSpec::Text { path, .. }
            | FieldSpec::Flag { path, .. }
            | FieldSpec::Choice { path, .. }
            | FieldSpec::Number { path, .. }
            | FieldSpec::List { path, .. } => path.as_str(),
        })
        .collect()
}

#[tokio::test]
async fn a_question_is_edited_by_the_id_quizzes_use() -> Result<()> {
    let (course, _, ids) = course_with_questions().await?;
    let content = keys::question_ref(&ids[0]);
    let opened = course.edits().open(&content).await;
    assert!(opened.is_ok(), "{opened:?}");
    let opened = opened?;
    assert_eq!(opened.content, content);
    assert_eq!(opened.editable, Editability::Editable);

    let outcome = course
        .edits()
        .apply(&edit_req(
            content,
            opened.revision,
            vec![EditOp::Replace {
                path: "/prompt".into(),
                value: json!("What happens to spaces inside header names in DictReader?"),
            }],
        ))
        .await;
    assert!(outcome.is_ok(), "{outcome:?}");

    let stored: String =
        sqlx::query_scalar("SELECT prompt FROM learning_assessment_candidates WHERE id=?")
            .bind(&ids[0])
            .fetch_one(&course.pool)
            .await?;
    assert_eq!(
        stored,
        "What happens to spaces inside header names in DictReader?"
    );
    Ok(())
}

#[tokio::test]
async fn removing_a_choice_keeps_the_key_on_the_right_answer() -> Result<()> {
    let (course, outcome, _) = course_with_questions().await?;
    let item = single_choice(
        &outcome,
        "Which choice marks the true header behavior?",
        &[
            "Leading distractor.",
            "It preserves spaces.",
            "Trailing distractor.",
            "Another distractor.",
        ],
        1,
    );
    let qid = item.item.id.clone();
    let mut tx = course.db.begin_write().await?;
    insert_bank_items_in(&mut tx, &course.program_id, &[item], None).await?;
    tx.commit().await?;

    let content = keys::question_ref(&qid);
    let opened = course.edits().open(&content).await;
    assert!(opened.is_ok(), "{opened:?}");
    let opened = opened?;

    let applied = course
        .edits()
        .apply(&edit_req(
            content,
            opened.revision,
            vec![EditOp::Remove {
                path: "/choices/0".into(),
            }],
        ))
        .await;
    assert!(applied.is_ok(), "{applied:?}");

    let (options_json, key_json): (String, String) = sqlx::query_as(
        "SELECT c.options_json, k.key_json FROM learning_assessment_candidates c JOIN learning_assessment_answer_keys k ON k.candidate_id=c.id WHERE c.id=?",
    )
    .bind(&qid)
    .fetch_one(&course.pool)
    .await?;
    let options: Vec<String> = serde_json::from_str(&options_json)?;
    let key: LearningAssessmentKey = serde_json::from_str(&key_json)?;
    assert_eq!(options[0], "It preserves spaces.");
    assert_eq!(key, LearningAssessmentKey::Choice(0));
    Ok(())
}

#[tokio::test]
async fn a_true_false_answer_and_its_reason_are_stored_together() -> Result<()> {
    let (course, _, ids) = course_with_questions().await?;
    let content = keys::question_ref(&ids[2]);
    let opened = course.edits().open(&content).await;
    assert!(opened.is_ok(), "{opened:?}");
    let opened = opened?;

    let applied = course
        .edits()
        .apply(&edit_req(
            content,
            opened.revision,
            vec![
                EditOp::Replace {
                    path: "/answer".into(),
                    value: json!(0),
                },
                EditOp::Replace {
                    path: "/explanation".into(),
                    value: json!("Changed explanation matching True."),
                },
            ],
        ))
        .await;
    assert!(applied.is_ok(), "{applied:?}");

    let (key_json, explanation): (String, String) = sqlx::query_as(
        "SELECT key_json, explanation FROM learning_assessment_answer_keys WHERE candidate_id=?",
    )
    .bind(&ids[2])
    .fetch_one(&course.pool)
    .await?;
    let key: LearningAssessmentKey = serde_json::from_str(&key_json)?;
    assert_eq!(key, LearningAssessmentKey::Choice(0));
    assert_eq!(explanation, "Changed explanation matching True.");
    Ok(())
}

#[tokio::test]
async fn a_question_without_choices_offers_its_prompt_and_explanation() -> Result<()> {
    let (course, _, ids) = course_with_questions().await?;
    let content = keys::question_ref(&ids[3]);
    let opened = course.edits().open(&content).await;
    assert!(opened.is_ok(), "{opened:?}");
    let opened = opened?;
    assert_eq!(field_paths(&opened.form), vec!["/prompt", "/explanation"]);

    let applied = course
        .edits()
        .apply(&edit_req(
            content,
            opened.revision,
            vec![
                EditOp::Replace {
                    path: "/prompt".into(),
                    value: json!("Which property stores parsed column headers?"),
                },
                EditOp::Replace {
                    path: "/explanation".into(),
                    value: json!("The fieldnames property exposes the parsed headers."),
                },
            ],
        ))
        .await;
    assert!(applied.is_ok(), "{applied:?}");

    let (prompt, key_json, explanation): (String, String, String) = sqlx::query_as(
        "SELECT c.prompt, k.key_json, k.explanation FROM learning_assessment_candidates c JOIN learning_assessment_answer_keys k ON k.candidate_id=c.id WHERE c.id=?",
    )
    .bind(&ids[3])
    .fetch_one(&course.pool)
    .await?;
    let key: LearningAssessmentKey = serde_json::from_str(&key_json)?;
    assert_eq!(prompt, "Which property stores parsed column headers?");
    assert_eq!(
        explanation,
        "The fieldnames property exposes the parsed headers."
    );
    assert_eq!(
        key,
        LearningAssessmentKey::Text(LearningTextKey {
            variants: vec!["fieldnames".into()],
            numeric: None,
        })
    );
    Ok(())
}

#[tokio::test]
async fn two_equal_choices_are_refused() -> Result<()> {
    let (course, _, ids) = course_with_questions().await?;
    let content = keys::question_ref(&ids[0]);
    let opened = course.edits().open(&content).await;
    assert!(opened.is_ok(), "{opened:?}");
    let opened = opened?;

    let req = edit_req(
        content,
        opened.revision,
        vec![EditOp::Replace {
            path: "/choices/1/text".into(),
            value: json!("It keeps them."),
        }],
    );
    let preview = course.edits().preview(&req).await?;
    assert!(!preview.problems.is_empty(), "{preview:?}");
    let res = course.edits().apply(&req).await;
    assert!(matches!(res, Err(AppError::InvalidInput(_))), "{res:?}");
    Ok(())
}

#[tokio::test]
async fn a_question_equal_to_another_is_refused() -> Result<()> {
    let (course, _, ids) = course_with_questions().await?;
    let content = keys::question_ref(&ids[1]);
    let opened = course.edits().open(&content).await;
    assert!(opened.is_ok(), "{opened:?}");
    let opened = opened?;

    let req = edit_req(
        content,
        opened.revision,
        vec![
            EditOp::Replace {
                path: "/prompt".into(),
                value: json!("How does DictReader handle spaces in column names?"),
            },
            EditOp::Replace {
                path: "/choices/0/text".into(),
                value: json!("It keeps them."),
            },
            EditOp::Replace {
                path: "/choices/1/text".into(),
                value: json!("It trims them."),
            },
            EditOp::Replace {
                path: "/choices/2/text".into(),
                value: json!("It drops the column."),
            },
            EditOp::Replace {
                path: "/choices/3/text".into(),
                value: json!("It raises an error."),
            },
        ],
    );
    let preview = course.edits().preview(&req).await?;
    assert!(
        preview
            .problems
            .iter()
            .any(|p| p.message.contains("Another question in this course")),
        "{preview:?}"
    );
    let res = course.edits().apply(&req).await;
    assert!(matches!(res, Err(AppError::InvalidInput(_))), "{res:?}");
    Ok(())
}

#[tokio::test]
async fn a_question_needs_a_right_answer() -> Result<()> {
    let (course, _, ids) = course_with_questions().await?;
    let content = keys::question_ref(&ids[0]);
    let opened = course.edits().open(&content).await;
    assert!(opened.is_ok(), "{opened:?}");
    let opened = opened?;

    let req = edit_req(
        content,
        opened.revision,
        vec![EditOp::Replace {
            path: "/choices/0/correct".into(),
            value: json!(false),
        }],
    );
    let preview = course.edits().preview(&req).await?;
    assert!(!preview.problems.is_empty(), "{preview:?}");
    let res = course.edits().apply(&req).await;
    assert!(matches!(res, Err(AppError::InvalidInput(_))), "{res:?}");
    Ok(())
}

#[tokio::test]
async fn a_question_in_an_open_quiz_is_not_editable() -> Result<()> {
    let pool = crate::tests::pool().await?;
    let database = Database::new(pool);
    let program = crate::tests::fixture();
    let programs = LearningRepository::new(database.clone());
    programs.create(&program).await?;
    programs
        .accept(&AcceptLearningProgramRequestDto {
            program_id: program.summary.id.clone(),
            expected_revision: 0,
            title: program.summary.title.clone(),
        })
        .await?;
    let lesson_id = program.modules[0].lessons[0].id.clone();
    programs
        .prepare(
            &program.summary.id,
            &lesson_id,
            1,
            &crate::tests::prepared(&lesson_id),
        )
        .await?;
    let scope = LearningQuizScopeDto {
        kind: LearningQuizScopeKind::Lesson,
        id: Some(lesson_id),
    };
    fill_bank(&database, &program.summary.id, &scope).await?;
    let form = LearningAssessmentRepository::new(database.clone())
        .start_quiz(
            &StartLearningQuizRequestDto {
                operation_id: Uuid::new_v4().to_string(),
                form_id: Uuid::new_v4().to_string(),
                program_id: program.summary.id.clone(),
                scope,
                mode: LearningQuizMode::Graded,
                retake_of_form_id: None,
            },
            "start",
        )
        .await?;

    let edits = crate::edits::content_edits(&database)?;
    let content = keys::question_ref(&form.items[0].id);
    let opened = edits.open(&content).await;
    assert!(opened.is_ok(), "{opened:?}");
    let opened = opened?;
    assert!(
        matches!(opened.editable, Editability::NotNow { .. }),
        "{:?}",
        opened.editable
    );

    let res = edits
        .apply(&edit_req(
            content,
            opened.revision,
            vec![EditOp::Replace {
                path: "/prompt".into(),
                value: json!("Edited while quiz is open."),
            }],
        ))
        .await;
    assert!(matches!(res, Err(AppError::InvalidState(_))), "{res:?}");
    Ok(())
}

#[tokio::test]
async fn a_dropped_question_returns_to_the_bank_and_a_revert_drops_it_again() -> Result<()> {
    let (course, _, ids) = course_with_questions().await?;
    sqlx::query("UPDATE learning_assessment_candidates SET review_status='dropped' WHERE id=?")
        .bind(&ids[0])
        .execute(&course.pool)
        .await?;

    let content = keys::question_ref(&ids[0]);
    let opened = course.edits().open(&content).await;
    assert!(opened.is_ok(), "{opened:?}");
    let opened = opened?;

    let applied = course
        .edits()
        .apply(&edit_req(
            content,
            opened.revision,
            vec![EditOp::Replace {
                path: "/prompt".into(),
                value: json!("Updated prompt restoring a dropped question."),
            }],
        ))
        .await;
    assert!(applied.is_ok(), "{applied:?}");
    let applied = applied?;

    let status_after: String =
        sqlx::query_scalar("SELECT review_status FROM learning_assessment_candidates WHERE id=?")
            .bind(&ids[0])
            .fetch_one(&course.pool)
            .await?;
    assert_eq!(status_after, "agreed");

    let reverted = course
        .edits()
        .revert(&RevertContentEditRequest {
            revert_id: Uuid::new_v4().to_string(),
            edit_id: applied.edit_id,
            expected_revision: applied.revision,
            and_later: false,
        })
        .await;
    assert!(reverted.is_ok(), "{reverted:?}");

    let status_reverted: String =
        sqlx::query_scalar("SELECT review_status FROM learning_assessment_candidates WHERE id=?")
            .bind(&ids[0])
            .fetch_one(&course.pool)
            .await?;
    assert_eq!(status_reverted, "dropped");
    Ok(())
}

#[tokio::test]
async fn a_section_edit_names_the_bank_questions_written_from_removed_text() -> Result<()> {
    let (course, outcome, _) = course_with_questions().await?;
    let body = "First lesson sentence stays intact. Second lesson sentence explains whitespace preservation.";
    sqlx::query("UPDATE learning_blocks SET body=? WHERE lesson_id=? AND ordinal=0")
        .bind(body)
        .bind(&course.lesson_id)
        .execute(&course.pool)
        .await?;

    let mut bank_hit = single_choice(
        &outcome,
        "Why are spaces preserved?",
        &[
            "The parser leaves them intact.",
            "The parser collapses spaces.",
            "The parser strips tabs.",
            "The parser renames headers.",
        ],
        0,
    );
    bank_hit.origin = "bank";
    bank_hit.lesson_id = Some(course.lesson_id.clone());
    bank_hit.source_quote = "Second lesson sentence explains whitespace preservation.".into();
    let hit_id = bank_hit.item.id.clone();

    let mut bank_untouched = single_choice(
        &outcome,
        "Which part stays intact?",
        &[
            "The first sentence.",
            "The last paragraph.",
            "The title only.",
            "Nothing stays.",
        ],
        0,
    );
    bank_untouched.origin = "bank";
    bank_untouched.lesson_id = Some(course.lesson_id.clone());
    bank_untouched.source_quote = "First lesson sentence stays intact.".into();
    let untouched_id = bank_untouched.item.id.clone();

    let mut tx = course.db.begin_write().await?;
    insert_bank_items_in(
        &mut tx,
        &course.program_id,
        &[bank_hit, bank_untouched],
        None,
    )
    .await?;
    tx.commit().await?;

    let section_content = keys::section_ref(&course.lesson_id, 0);
    let opened = course.edits().open(&section_content).await;
    assert!(opened.is_ok(), "{opened:?}");
    let opened = opened?;

    let req = edit_req(
        section_content,
        opened.revision,
        vec![EditOp::Replace {
            path: "/body".into(),
            value: json!("First lesson sentence stays intact."),
        }],
    );
    let preview = course.edits().preview(&req).await?;
    let hit_ref = keys::question_ref(&hit_id);
    let untouched_ref = keys::question_ref(&untouched_id);
    let consequence = preview
        .consequences
        .iter()
        .find(|c| c.code == "quiz_bank.quote_removed")
        .expect("expected quiz_bank.quote_removed consequence");
    assert!(
        consequence.related.iter().any(|r| r.content == hit_ref),
        "{consequence:?}"
    );
    assert!(
        consequence
            .related
            .iter()
            .all(|r| r.content != untouched_ref),
        "{consequence:?}"
    );

    let applied = course.edits().apply(&req).await?;
    let applied_consequence = applied
        .consequences
        .iter()
        .find(|c| c.code == "quiz_bank.quote_removed")
        .expect("expected applied quiz_bank.quote_removed consequence");
    assert!(
        applied_consequence
            .related
            .iter()
            .any(|r| r.content == hit_ref),
        "{applied_consequence:?}"
    );
    Ok(())
}
