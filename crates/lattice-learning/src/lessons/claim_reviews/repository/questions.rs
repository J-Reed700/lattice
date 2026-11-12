//! A lesson's own questions in the quiz bank.

use lattice_core::shared::error::Result;
use lattice_core::shared::persistence::WriteTx;
use sqlx::SqliteConnection;

use crate::persistence::db;

/// A question and its answer key as stored.
pub(in crate::lessons::claim_reviews) struct QuestionRow {
    pub id: String,
    pub format: String,
    pub prompt: String,
    pub options_json: String,
    pub key_json: String,
    pub explanation: String,
    pub rationales_json: String,
}

type Columns = (String, String, String, String, String, String, String);

/// The lesson's question at `index`, in the order it was banked.
pub(in crate::lessons::claim_reviews) async fn question_at(
    connection: &mut SqliteConnection,
    program_id: &str,
    lesson_id: &str,
    index: usize,
) -> Result<Option<QuestionRow>> {
    let row = sqlx::query_as::<_, Columns>(
        "SELECT c.id, c.format, c.prompt, c.options_json, k.key_json, k.explanation, \
                k.distractor_rationales_json \
         FROM learning_assessment_candidates c \
         JOIN learning_assessment_answer_keys k ON k.candidate_id=c.id \
         WHERE c.program_id=? AND c.lesson_id=? AND c.origin IN ('lesson','migrated') \
         ORDER BY c.rowid LIMIT 1 OFFSET ?",
    )
    .bind(program_id)
    .bind(lesson_id)
    .bind(i64::try_from(index).unwrap_or(i64::MAX))
    .fetch_optional(&mut *connection)
    .await
    .map_err(db)?;
    Ok(row.map(
        |(id, format, prompt, options_json, key_json, explanation, rationales_json)| QuestionRow {
            id,
            format,
            prompt,
            options_json,
            key_json,
            explanation,
            rationales_json,
        },
    ))
}

/// Whether another question of the program has this content token.
pub(in crate::lessons::claim_reviews) async fn token_taken_by_another(
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
    .await
    .map_err(db)?;
    Ok(other.is_some())
}

pub(in crate::lessons::claim_reviews) async fn write_question(
    tx: &mut WriteTx,
    candidate_id: &str,
    prompt: &str,
    options_json: Option<&str>,
    token: &str,
) -> Result<()> {
    sqlx::query(
        "UPDATE learning_assessment_candidates SET prompt=?, options_json=COALESCE(?, options_json), content_sha256=?, review_status=CASE WHEN review_status='dropped' THEN 'agreed' ELSE review_status END WHERE id=?",
    )
    .bind(prompt)
    .bind(options_json)
    .bind(token)
    .bind(candidate_id)
    .execute(&mut **tx)
    .await
    .map_err(db)?;
    Ok(())
}

pub(in crate::lessons::claim_reviews) async fn write_answer_key(
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
    .await
    .map_err(db)?;
    Ok(())
}
