//! Preparing for: course-level practice profiles.
use super::*;

impl LearningPracticeRepository {
    /// Reads a course's practice profile, returning the default empty profile if none exists yet.
    pub async fn get_profile(&self, program_id: &str) -> Result<LearningPracticeProfileDto> {
        uuid(program_id, "program")?;
        let exists: Option<i64> = sqlx::query_scalar("SELECT 1 FROM learning_programs WHERE id=?")
            .bind(program_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(db)?;
        if exists.is_none() {
            return Err(AppError::NotFound("Learning program not found".into()));
        }

        let row = sqlx::query(
            "SELECT details, language, interview_at, company, role, revision FROM learning_practice_profiles WHERE program_id=?"
        )
        .bind(program_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(db)?;

        match row {
            Some(r) => Ok(LearningPracticeProfileDto {
                program_id: program_id.to_string(),
                details: r.get("details"),
                language: r.get("language"),
                interview_at: r.get("interview_at"),
                company: r.get("company"),
                role: r.get("role"),
                revision: r.get("revision"),
            }),
            None => Ok(LearningPracticeProfileDto {
                program_id: program_id.to_string(),
                details: String::new(),
                language: None,
                interview_at: None,
                company: None,
                role: None,
                revision: 0,
            }),
        }
    }

    /// Saves a course's practice profile, validating optimistic locking via expected_revision.
    pub async fn save_profile(
        &self,
        request: &SaveLearningPracticeProfileRequestDto,
    ) -> Result<LearningPracticeProfileDto> {
        uuid(&request.program_id, "program")?;
        let mut tx = self.pool.begin().await.map_err(db)?;
        Self::writer_lock(&mut tx, &request.program_id).await?;

        let exists: Option<i64> = sqlx::query_scalar("SELECT 1 FROM learning_programs WHERE id=?")
            .bind(&request.program_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(db)?;
        if exists.is_none() {
            return Err(AppError::NotFound("Learning program not found".into()));
        }

        let existing: Option<(i64, Option<i64>)> = sqlx::query_as(
            "SELECT revision, interview_at FROM learning_practice_profiles WHERE program_id=?"
        )
        .bind(&request.program_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(db)?;

        let (current_revision, _old_interview_at) = match existing {
            Some((rev, interview)) => (rev, interview),
            None => (0, None),
        };

        if let Some(expected) = request.expected_revision {
            if expected != current_revision {
                return Err(AppError::Conflict(
                    "Practice profile changed; reload and retry.".into(),
                ));
            }
        }

        let next_revision = current_revision + 1;
        let t = now();

        sqlx::query(
            "INSERT INTO learning_practice_profiles (program_id, details, language, interview_at, company, role, revision, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT(program_id) DO UPDATE SET
                details = excluded.details,
                language = excluded.language,
                interview_at = excluded.interview_at,
                company = excluded.company,
                role = excluded.role,
                revision = excluded.revision,
                updated_at = excluded.updated_at"
        )
        .bind(&request.program_id)
        .bind(&request.details)
        .bind(&request.language)
        .bind(request.interview_at)
        .bind(&request.company)
        .bind(&request.role)
        .bind(next_revision)
        .bind(t)
        .execute(&mut *tx)
        .await
        .map_err(db)?;

        Self::update_interview_target_in_tx(&mut tx, &request.program_id, request.interview_at).await?;

        tx.commit().await.map_err(db)?;

        Ok(LearningPracticeProfileDto {
            program_id: request.program_id.clone(),
            details: request.details.clone(),
            language: request.language.clone(),
            interview_at: request.interview_at,
            company: request.company.clone(),
            role: request.role.clone(),
            revision: next_revision,
        })
    }

    /// Internal helper to update target_at on associated interview study decks in a transaction.
    async fn update_interview_target_in_tx(
        tx: &mut Transaction<'_, Sqlite>,
        program_id: &str,
        target_at: Option<i64>,
    ) -> Result<()> {
        let activity_ids: Vec<String> = sqlx::query_scalar(
            "SELECT id FROM learning_practical_activities WHERE program_id=? AND kind='interview'"
        )
        .bind(program_id)
        .fetch_all(&mut **tx)
        .await
        .map_err(db)?;

        for activity_id in activity_ids {
            let deck_ids: Vec<String> = sqlx::query_scalar(
                "SELECT id FROM study_decks WHERE id LIKE ?"
            )
            .bind(format!("%{activity_id}%"))
            .fetch_all(&mut **tx)
            .await
            .map_err(db)?;

            for deck_id in deck_ids {
                match target_at {
                    Some(target) => {
                        sqlx::query(
                            "INSERT INTO study_deck_targets (deck_id, target_at) VALUES (?, ?)
                             ON CONFLICT(deck_id) DO UPDATE SET target_at=excluded.target_at"
                        )
                        .bind(&deck_id)
                        .bind(target)
                        .execute(&mut **tx)
                        .await
                        .map_err(db)?;
                    }
                    None => {
                        sqlx::query("DELETE FROM study_deck_targets WHERE deck_id=?")
                            .bind(&deck_id)
                            .execute(&mut **tx)
                            .await
                            .map_err(db)?;
                    }
                }
            }
        }
        Ok(())
    }

    /// Updates interview targets associated with a course's interview date.
    pub async fn update_interview_target(
        &self,
        program_id: &str,
        target_at: Option<i64>,
    ) -> Result<()> {
        uuid(program_id, "program")?;
        let mut tx = self.pool.begin().await.map_err(db)?;
        Self::update_interview_target_in_tx(&mut tx, program_id, target_at).await?;
        tx.commit().await.map_err(db)?;
        Ok(())
    }
}
