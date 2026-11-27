//! Single writer for study and recall card text across `study_cards` and recall profiles.
use super::{json, parse_format};
use crate::portability_dto::{LearningRecallCardFormat, LearningRecallContentDto};
use crate::recall::study_dto::StudyCardFormat;
use lattice_core::shared::{
    error::{AppError, Result},
    persistence::WriteTx,
};
use sqlx::{Row, SqliteConnection};

pub(crate) const CORRECTED: &str = "Corrected in place.";
pub(crate) const UNDID: &str = "Undid a correction.";

pub(crate) struct CardRow {
    pub deck_id: String,
    pub multiple_choice: bool,
    pub question: String,
    pub answer: String,
    pub options: Vec<String>,
    pub correct_index: usize,
    pub explanation: String,
    pub profile: Option<CardProfile>,
}

pub(crate) struct CardProfile {
    pub program_id: String,
    pub format: LearningRecallCardFormat,
    pub content: LearningRecallContentDto,
}

pub(crate) async fn card_row(
    connection: &mut SqliteConnection,
    card_id: &str,
) -> Result<Option<CardRow>> {
    let Some(row) = sqlx::query(
        "SELECT c.deck_id, c.format AS study_format, c.question, c.answer, c.options_json, \
         c.correct_index, c.explanation, p.program_id, p.format AS profile_format, p.answer_json \
         FROM study_cards c \
         LEFT JOIN learning_recall_card_profiles p ON p.card_id = c.id \
         WHERE c.id = ?",
    )
    .bind(card_id)
    .fetch_optional(&mut *connection)
    .await?
    else {
        return Ok(None);
    };
    let options: Vec<String> = serde_json::from_str(row.get("options_json"))
        .map_err(|error| AppError::InvalidState(format!("Invalid stored study data: {error}")))?;
    let correct_index = usize::try_from(row.get::<i64, _>("correct_index").max(0)).unwrap_or(0);
    let multiple_choice =
        StudyCardFormat::parse(&row.get::<String, _>("study_format"))? == StudyCardFormat::MultipleChoice;
    let profile = match row.get::<Option<String>, _>("program_id") {
        Some(program_id) => {
            let format = parse_format(row.get("profile_format"))?;
            let content: LearningRecallContentDto = serde_json::from_str(row.get("answer_json"))
                .map_err(|error| {
                    AppError::InvalidState(format!("Invalid saved recall content: {error}"))
                })?;
            Some(CardProfile {
                program_id,
                format,
                content,
            })
        }
        None => None,
    };
    Ok(Some(CardRow {
        deck_id: row.get("deck_id"),
        multiple_choice,
        question: row.get("question"),
        answer: row.get("answer"),
        options,
        correct_index,
        explanation: row.get("explanation"),
        profile,
    }))
}

pub(crate) struct CardText<'a> {
    pub question: &'a str,
    pub answer: &'a str,
    pub options: &'a [String],
    pub correct_index: usize,
    pub explanation: &'a str,
    pub profile: Option<(&'a LearningRecallContentDto, &'a str)>,
}

/// The one function that updates a card's text.
pub(crate) async fn write_card_text(
    tx: &mut WriteTx,
    card_id: &str,
    text: &CardText<'_>,
    at: i64,
) -> Result<()> {
    let options_json = json(text.options)?;
    sqlx::query(
        "UPDATE study_cards \
         SET question=?, answer=?, options_json=?, correct_index=?, explanation=? \
         WHERE id=?",
    )
    .bind(text.question)
    .bind(text.answer)
    .bind(options_json)
    .bind(i64::try_from(text.correct_index).unwrap_or(0))
    .bind(text.explanation)
    .bind(card_id)
    .execute(&mut **tx)
    .await?;

    if let Some((content, reason)) = text.profile {
        let profile_row = sqlx::query(
            "SELECT format, source_version_ids_json, content_revision \
             FROM learning_recall_card_profiles WHERE card_id=?",
        )
        .bind(card_id)
        .fetch_optional(&mut **tx)
        .await?
        .ok_or_else(|| AppError::NotFound("Recall card profile not found.".into()))?;
        let format: String = profile_row.get("format");
        let source_version_ids_json: String = profile_row.get("source_version_ids_json");
        let next_revision = profile_row.get::<i64, _>("content_revision") + 1;
        let prompt_json = json(&content.prompt)?;
        let answer_json = json(content)?;
        sqlx::query(
            "UPDATE learning_recall_card_profiles \
             SET prompt_json=?, answer_json=?, content_revision=content_revision+1, updated_at=? \
             WHERE card_id=?",
        )
        .bind(&prompt_json)
        .bind(&answer_json)
        .bind(at)
        .bind(card_id)
        .execute(&mut **tx)
        .await?;
        insert_card_version(
            tx,
            card_id,
            next_revision,
            &format,
            &prompt_json,
            &answer_json,
            &source_version_ids_json,
            reason,
            at,
        )
        .await?;
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn insert_card_version(
    tx: &mut WriteTx,
    card_id: &str,
    revision: i64,
    format: &str,
    prompt_json: &str,
    answer_json: &str,
    source_version_ids_json: &str,
    change_reason: &str,
    created_at: i64,
) -> Result<()> {
    sqlx::query(
        "INSERT INTO learning_recall_card_versions(\
         card_id,revision,format,prompt_json,answer_json,source_version_ids_json,change_reason,created_at\
         ) VALUES(?,?,?,?,?,?,?,?)",
    )
    .bind(card_id)
    .bind(revision)
    .bind(format)
    .bind(prompt_json)
    .bind(answer_json)
    .bind(source_version_ids_json)
    .bind(change_reason)
    .bind(created_at)
    .execute(&mut **tx)
    .await?;
    Ok(())
}
