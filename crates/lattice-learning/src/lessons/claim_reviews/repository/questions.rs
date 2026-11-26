//! A lesson's and course bank's questions in `learning_assessment_candidates`.

use lattice_core::shared::error::Result;
use lattice_core::shared::persistence::WriteTx;
use sqlx::{Row, SqliteConnection};

/// Where a candidate question lives and its current review / report state.
pub(crate) struct QuestionHome {
    pub program_id: String,
    pub lesson_id: Option<String>,
    pub review_state: Option<String>,
    pub review_status: String,
    pub has_reports: bool,
}

pub(crate) async fn question_home(
    connection: &mut SqliteConnection,
    candidate_id: &str,
) -> Result<Option<QuestionHome>> {
    let row = sqlx::query(
        "SELECT c.program_id, c.lesson_id, r.state AS review_state, c.review_status, \
                EXISTS(SELECT 1 FROM learning_assessment_item_reports rep WHERE rep.candidate_id=c.id) AS has_reports \
         FROM learning_assessment_candidates c \
         LEFT JOIN learning_lesson_reviews r ON r.lesson_id=c.lesson_id \
         WHERE c.id=?",
    )
    .bind(candidate_id)
    .fetch_optional(&mut *connection)
    .await?;
    Ok(row.map(|row| QuestionHome {
        program_id: row.get("program_id"),
        lesson_id: row.get("lesson_id"),
        review_state: row.get("review_state"),
        review_status: row.get("review_status"),
        has_reports: row.get::<i64, _>("has_reports") != 0,
    }))
}

/// Whether a candidate question already appears on submitted or open quizzes.
pub(crate) struct QuestionPlace {
    pub on_a_submitted_quiz: bool,
    pub in_open_quiz: bool,
}

pub(crate) async fn place_of(
    connection: &mut SqliteConnection,
    candidate_id: &str,
) -> Result<QuestionPlace> {
    let row = sqlx::query(
        "SELECT \
            EXISTS( \
                SELECT 1 FROM learning_assessment_form_items fi \
                JOIN learning_assessment_forms f ON f.id=fi.form_id \
                WHERE fi.candidate_id=? AND f.status='submitted' \
            ) AS on_submitted, \
            EXISTS( \
                SELECT 1 FROM learning_assessment_form_items fi \
                JOIN learning_assessment_forms f ON f.id=fi.form_id \
                WHERE fi.candidate_id=? AND f.status IN ('active','interrupted') \
            ) AS in_open",
    )
    .bind(candidate_id)
    .bind(candidate_id)
    .fetch_one(&mut *connection)
    .await?;
    Ok(QuestionPlace {
        on_a_submitted_quiz: row.get::<i64, _>("on_submitted") != 0,
        in_open_quiz: row.get::<i64, _>("in_open") != 0,
    })
}

/// A lesson's own question in banked order.
pub(crate) struct LessonQuestion {
    pub id: String,
}

pub(crate) async fn lesson_questions(
    connection: &mut SqliteConnection,
    lesson_id: &str,
) -> Result<Vec<LessonQuestion>> {
    let ids: Vec<String> = sqlx::query_scalar(
        "SELECT id FROM learning_assessment_candidates \
         WHERE lesson_id=? AND origin IN ('lesson','migrated') \
         ORDER BY rowid",
    )
    .bind(lesson_id)
    .fetch_all(&mut *connection)
    .await?;
    Ok(ids.into_iter().map(|id| LessonQuestion { id }).collect())
}

pub(crate) async fn token_taken_by_another(
    connection: &mut SqliteConnection,
    program_id: &str,
    candidate_id: &str,
    token: &str,
) -> Result<bool> {
    let other: Option<i64> = sqlx::query_scalar(
        "SELECT 1 FROM learning_assessment_candidates WHERE program_id=? AND content_sha256=? AND id<>?",
    )
    .bind(program_id)
    .bind(token)
    .bind(candidate_id)
    .fetch_optional(&mut *connection)
    .await?;
    Ok(other.is_some())
}

pub(crate) async fn write_question(
    tx: &mut WriteTx,
    candidate_id: &str,
    prompt: &str,
    options_json: Option<&str>,
    token: &str,
) -> Result<()> {
    sqlx::query(
        "UPDATE learning_assessment_candidates \
         SET prompt=?, options_json=COALESCE(?, options_json), content_sha256=? \
         WHERE id=?",
    )
    .bind(prompt)
    .bind(options_json)
    .bind(token)
    .bind(candidate_id)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

pub(crate) async fn write_answer_key(
    tx: &mut WriteTx,
    candidate_id: &str,
    explanation: &str,
    key_json: Option<&str>,
    rationales_json: &str,
) -> Result<()> {
    sqlx::query(
        "UPDATE learning_assessment_answer_keys SET explanation=?, key_json=COALESCE(?, key_json), \
         distractor_rationales_json=? WHERE candidate_id=?",
    )
    .bind(explanation)
    .bind(key_json)
    .bind(rationales_json)
    .bind(candidate_id)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

pub(crate) async fn review_status(
    connection: &mut SqliteConnection,
    candidate_id: &str,
) -> Result<Option<String>> {
    Ok(
        sqlx::query_scalar("SELECT review_status FROM learning_assessment_candidates WHERE id=?")
            .bind(candidate_id)
            .fetch_optional(&mut *connection)
            .await?,
    )
}

pub(crate) async fn set_review_status(
    tx: &mut WriteTx,
    candidate_id: &str,
    status: &str,
) -> Result<()> {
    sqlx::query("UPDATE learning_assessment_candidates SET review_status=? WHERE id=?")
        .bind(status)
        .bind(candidate_id)
        .execute(&mut **tx)
        .await?;
    Ok(())
}
