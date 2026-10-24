//! Program-scoped source selectors, retrieval and versioned recall cards.
use super::{
    recall_content::validate_content,
    schedule::answered,
    study_dto::*,
    study_repository::{record_review, CardReview, ReviewOutcome},
};
use crate::features::learning::{
    operations::Operation,
    persistence::{db, encode as json, hash as payload_hash, now},
    portability_dto::*,
    references::course_index::CourseIndexes,
    source_library::LearningSourceLibraryRepository,
    source_selector::{create_quote_selector, match_quote_selector, LearningQuoteMatch},
};
use lattice_core::shared::error::{AppError, Result};
use sqlx::{Row, Sqlite, SqlitePool, Transaction};
use std::collections::HashSet;

mod cards;
#[cfg(test)]
mod cards_tests;
mod selectors;
mod workspace;

const MAX_CARDS: i64 = 2_000;
const MAX_QUERY_CHARS: usize = 240;
const SCHEDULER_DISCLOSURE: &str = "Cards are scheduled with FSRS 6 and its default parameters. A review interval is not a promise or measurement of retention.";

fn invalid(message: impl Into<String>) -> AppError {
    AppError::InvalidInput(message.into())
}
fn validate_uuid(value: &str, label: &str) -> Result<()> {
    uuid::Uuid::parse_str(value)
        .map(|_| ())
        .map_err(|_| invalid(format!("Invalid {label} ID.")))
}
fn validate_owned_id(value: &str, label: &str) -> Result<()> {
    if value.trim().is_empty() || value.chars().count() > 512 {
        return Err(invalid(format!("Invalid {label} ID.")));
    }
    Ok(())
}
fn format_name(value: LearningRecallCardFormat) -> &'static str {
    match value {
        LearningRecallCardFormat::MultipleChoice => "multiple_choice",
        LearningRecallCardFormat::QuestionAnswer => "question_answer",
        LearningRecallCardFormat::Cloze => "cloze",
        LearningRecallCardFormat::Reverse => "reverse",
        LearningRecallCardFormat::CodePrediction => "code_prediction",
        LearningRecallCardFormat::Reconstruction => "reconstruction",
    }
}
fn parse_format(value: &str) -> Result<LearningRecallCardFormat> {
    match value {
        "multiple_choice" => Ok(LearningRecallCardFormat::MultipleChoice),
        "question_answer" => Ok(LearningRecallCardFormat::QuestionAnswer),
        "cloze" => Ok(LearningRecallCardFormat::Cloze),
        "reverse" => Ok(LearningRecallCardFormat::Reverse),
        "code_prediction" => Ok(LearningRecallCardFormat::CodePrediction),
        "reconstruction" => Ok(LearningRecallCardFormat::Reconstruction),
        _ => Err(AppError::InvalidState("Unknown recall card format.".into())),
    }
}

#[derive(Clone)]
pub struct LearningRecallRepository {
    pool: SqlitePool,
}
impl LearningRecallRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// Give cards accepted into the program deck their recall profile and
    /// first content version.
    async fn bootstrap_deck_cards(&self, program_id: &str) -> Result<()> {
        let mut tx = self.pool.begin().await.map_err(db)?;
        let deck_id: Option<String> =
            sqlx::query_scalar("SELECT deck_id FROM learning_memory WHERE program_id=?")
                .bind(program_id)
                .fetch_optional(&mut *tx)
                .await
                .map_err(db)?
                .flatten();
        let Some(deck_id) = deck_id else {
            tx.commit().await.map_err(db)?;
            return Ok(());
        };
        let rows = sqlx::query("SELECT c.* FROM study_cards c LEFT JOIN learning_recall_card_profiles p ON p.card_id=c.id WHERE c.deck_id=? AND p.card_id IS NULL ORDER BY c.id")
            .bind(&deck_id)
            .fetch_all(&mut *tx)
            .await
            .map_err(db)?;
        for row in rows {
            let card_id: String = row.get("id");
            let study_format = super::study_repository::parse_card_format(row.get("format"))?;
            let format = match study_format {
                StudyCardFormat::MultipleChoice => LearningRecallCardFormat::MultipleChoice,
                StudyCardFormat::QuestionAnswer => LearningRecallCardFormat::QuestionAnswer,
            };
            let content = LearningRecallContentDto {
                prompt: row.get("question"),
                answer: row.get("answer"),
                explanation: row.get("explanation"),
                options: serde_json::from_str(row.get("options_json")).map_err(|error| {
                    AppError::InvalidState(format!("Invalid Study options: {error}"))
                })?,
                correct_option_index: (format == LearningRecallCardFormat::MultipleChoice)
                    .then(|| row.get::<i64, _>("correct_index").max(0) as usize),
                language: None,
                cloze_deletions: vec![],
            };
            validate_content(format, &content)?;
            let source_ids = self
                .card_source_versions(&mut tx, program_id, &card_id)
                .await?;
            let timestamp = now();
            let prompt_json = json(&content.prompt)?;
            let answer_json = json(&content)?;
            sqlx::query("INSERT INTO learning_recall_card_profiles(card_id,program_id,format,prompt_json,answer_json,source_version_ids_json,content_revision,created_at,updated_at) VALUES(?,?,?,?,?,?,1,?,?)")
                .bind(&card_id)
                .bind(program_id)
                .bind(format_name(format))
                .bind(prompt_json)
                .bind(answer_json)
                .bind(json(&source_ids)?)
                .bind(timestamp)
                .bind(timestamp)
                .execute(&mut *tx)
                .await
                .map_err(db)?;
            sqlx::query("INSERT INTO learning_recall_card_versions(card_id,revision,format,prompt_json,answer_json,source_version_ids_json,change_reason,created_at) VALUES(?,1,?,?,?,?,?,?)")
                .bind(&card_id)
                .bind(format_name(format))
                .bind(json(&content.prompt)?)
                .bind(json(&content)?)
                .bind(json(&source_ids)?)
                .bind("Accepted into the program's recall deck")
                .bind(timestamp)
                .execute(&mut *tx)
                .await
                .map_err(db)?;
        }
        tx.commit().await.map_err(db)
    }

    async fn card_source_versions(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        program_id: &str,
        card_id: &str,
    ) -> Result<Vec<String>> {
        let origin: Option<String> = sqlx::query_scalar(
            "SELECT source_ids_json FROM learning_card_origins WHERE program_id=? AND card_id=?",
        )
        .bind(program_id)
        .bind(card_id)
        .fetch_optional(&mut **tx)
        .await
        .map_err(db)?;
        let ids: Vec<String> = if let Some(value) = origin {
            serde_json::from_str(&value).map_err(|error| {
                AppError::InvalidState(format!("Invalid card source links: {error}"))
            })?
        } else {
            vec![]
        };
        let mut result = Vec::new();
        for id in ids {
            let version_exists: Option<i64> = sqlx::query_scalar(
                "SELECT 1 FROM learning_source_versions WHERE program_id=? AND id=?",
            )
            .bind(program_id)
            .bind(&id)
            .fetch_optional(&mut **tx)
            .await
            .map_err(db)?;
            if version_exists.is_some() {
                result.push(id);
            }
        }
        Ok(result)
    }
}

async fn lock_active_program(tx: &mut Transaction<'_, Sqlite>, program_id: &str) -> Result<()> {
    let result =
        sqlx::query("UPDATE learning_programs SET status=status WHERE id=? AND status='active'")
            .bind(program_id)
            .execute(&mut **tx)
            .await
            .map_err(db)?;
    if result.rows_affected() != 1 {
        return Err(AppError::NotFound(
            "Active learning program not found.".into(),
        ));
    }
    Ok(())
}
fn recall_operation<'a>(
    operation_id: &'a str,
    program_id: &'a str,
    kind: &'a str,
    payload_hash: &'a str,
) -> Operation<'a> {
    Operation {
        id: operation_id,
        scope: "recall",
        kind,
        program_id: Some(program_id),
        subject_id: None,
        payload_hash,
        conflict: "Operation ID was reused with different recall data.",
    }
}

async fn memory_deck(tx: &mut Transaction<'_, Sqlite>, program_id: &str) -> Result<String> {
    crate::features::learning::repository::ensure_memory_resources(tx, program_id)
        .await
        .map(|(_, deck_id)| deck_id)
}
