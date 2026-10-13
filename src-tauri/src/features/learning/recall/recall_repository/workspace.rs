//! Reading the recall workspace: cards, their versions and schedules, and duplicate suggestions.
use super::*;

/// The card's FSRS schedule as the recall workspace reports it.
fn scheduler_dto(row: &sqlx::sqlite::SqliteRow) -> LearningRecallSchedulerStateDto {
    LearningRecallSchedulerStateDto {
        stability: row.get("stability"),
        difficulty: row.get("difficulty"),
        last_reviewed_at: row.get("last_reviewed_at"),
        due_at: row.get("due_at"),
        interval_days: row.get("interval_days"),
        review_count: row.get("review_count"),
    }
}

impl LearningRecallRepository {
    pub async fn workspace(&self, program_id: &str) -> Result<LearningRecallWorkspaceDto> {
        validate_uuid(program_id, "program")?;
        let program_exists: Option<i64> =
            sqlx::query_scalar("SELECT 1 FROM learning_programs WHERE id=?")
                .bind(program_id)
                .fetch_optional(&self.pool)
                .await
                .map_err(db)?;
        if program_exists.is_none() {
            return Err(AppError::NotFound("Learning program not found.".into()));
        }
        self.bootstrap_deck_cards(program_id).await?;
        let deck_id: Option<String> =
            sqlx::query_scalar("SELECT deck_id FROM learning_memory WHERE program_id=?")
                .bind(program_id)
                .fetch_optional(&self.pool)
                .await
                .map_err(db)?
                .flatten();
        let Some(deck_id) = deck_id else {
            return Ok(LearningRecallWorkspaceDto {
                program_id: program_id.into(),
                cards: vec![],
                duplicates: vec![],
                due_count: 0,
                scheduler_disclosure: SCHEDULER_DISCLOSURE.into(),
            });
        };
        let rows = sqlx::query("SELECT p.*,c.deck_id,c.due_at,c.interval_days,c.review_count,c.stability,c.difficulty,c.last_reviewed_at FROM learning_recall_card_profiles p JOIN study_cards c ON c.id=p.card_id WHERE p.program_id=? AND c.deck_id=? ORDER BY p.created_at,c.id")
            .bind(program_id)
            .bind(&deck_id)
            .fetch_all(&self.pool)
            .await
            .map_err(db)?;
        let mut cards = Vec::with_capacity(rows.len());
        for row in &rows {
            let card_id: String = row.get("card_id");
            let format = parse_format(row.get("format"))?;
            let content = content_from_row(row, format)?;
            let versions = self.card_versions(&card_id).await?;
            cards.push(LearningRecallCardDto {
                id: card_id,
                format,
                content,
                source_version_ids: serde_json::from_str(row.get("source_version_ids_json"))
                    .map_err(|error| {
                        AppError::InvalidState(format!("Invalid recall source references: {error}"))
                    })?,
                content_revision: row.get("content_revision"),
                scheduler: scheduler_dto(row),
                versions,
                created_at: row.get("created_at"),
                updated_at: row.get("updated_at"),
            });
        }
        let duplicates = self.duplicate_suggestions(program_id).await?;
        let due_count = cards
            .iter()
            .filter(|card| card.scheduler.due_at <= now())
            .count() as i64;
        Ok(LearningRecallWorkspaceDto {
            program_id: program_id.into(),
            cards,
            duplicates,
            due_count,
            scheduler_disclosure: SCHEDULER_DISCLOSURE.into(),
        })
    }

    async fn card_versions(&self, card_id: &str) -> Result<Vec<LearningRecallCardVersionDto>> {
        let rows = sqlx::query(
            "SELECT * FROM learning_recall_card_versions WHERE card_id=? ORDER BY revision",
        )
        .bind(card_id)
        .fetch_all(&self.pool)
        .await
        .map_err(db)?;
        rows.iter()
            .map(|row| {
                let format = parse_format(row.get("format"))?;
                let content: LearningRecallContentDto =
                    serde_json::from_str(row.get("answer_json")).map_err(|error| {
                        AppError::InvalidState(format!("Invalid saved recall content: {error}"))
                    })?;
                Ok(LearningRecallCardVersionDto {
                    revision: row.get("revision"),
                    format,
                    content,
                    source_version_ids: serde_json::from_str(row.get("source_version_ids_json"))
                        .map_err(|error| {
                            AppError::InvalidState(format!(
                                "Invalid recall version sources: {error}"
                            ))
                        })?,
                    change_reason: row.get("change_reason"),
                    created_at: row.get("created_at"),
                })
            })
            .collect()
    }

    async fn duplicate_suggestions(
        &self,
        program_id: &str,
    ) -> Result<Vec<LearningRecallDuplicateSuggestionDto>> {
        let rows = sqlx::query("SELECT * FROM learning_recall_duplicate_suggestions WHERE program_id=? ORDER BY created_at,id LIMIT 500")
            .bind(program_id)
            .fetch_all(&self.pool)
            .await
            .map_err(db)?;
        rows.iter()
            .map(|row| {
                Ok(LearningRecallDuplicateSuggestionDto {
                    id: row.get("id"),
                    card_id: row.get("card_id"),
                    possible_duplicate_card_id: row.get("possible_duplicate_card_id"),
                    reason: row.get("reason"),
                    similarity: row.get("similarity"),
                    status: match row.get::<String, _>("status").as_str() {
                        "pending" => LearningRecallDuplicateStatus::Pending,
                        "confirmed" => LearningRecallDuplicateStatus::Confirmed,
                        "dismissed" => LearningRecallDuplicateStatus::Dismissed,
                        _ => {
                            return Err(AppError::InvalidState(
                                "Unknown duplicate decision.".into(),
                            ))
                        }
                    },
                    created_at: row.get("created_at"),
                    decided_at: row.get("decided_at"),
                })
            })
            .collect()
    }
}

fn content_from_row(
    row: &sqlx::sqlite::SqliteRow,
    format: LearningRecallCardFormat,
) -> Result<LearningRecallContentDto> {
    let content: LearningRecallContentDto =
        serde_json::from_str(row.get("answer_json")).map_err(|error| {
            AppError::InvalidState(format!("Invalid saved recall content: {error}"))
        })?;
    validate_content(format, &content)?;
    Ok(content)
}
