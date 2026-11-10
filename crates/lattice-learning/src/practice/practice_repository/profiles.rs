//! Preparing for: course-level practice profiles.
use super::*;
use crate::recall::{study_repository::set_target_in, study_service};
use lattice_core::shared::persistence::Transactions;
use sqlx::SqliteConnection;

async fn require_program(connection: &mut SqliteConnection, program_id: &str) -> Result<()> {
    let exists: Option<i64> = sqlx::query_scalar("SELECT 1 FROM learning_programs WHERE id=?")
        .bind(program_id)
        .fetch_optional(&mut *connection)
        .await
        .map_err(db)?;
    match exists {
        Some(_) => Ok(()),
        None => Err(AppError::NotFound("Learning program not found".into())),
    }
}

impl LearningPracticeRepository {
    /// A course's practice profile; the empty profile at revision 0 when none was saved.
    pub async fn get_profile(&self, program_id: &str) -> Result<LearningPracticeProfileDto> {
        uuid(program_id, "program")?;
        let mut tx = self.pool.begin_read().await.map_err(db)?;
        require_program(&mut tx, program_id).await?;
        let saved = profile_in(&mut tx, program_id).await?;
        tx.commit().await.map_err(db)?;
        Ok(saved.unwrap_or_else(|| LearningPracticeProfileDto {
            program_id: program_id.to_owned(),
            details: String::new(),
            language: None,
            interview_at: None,
            company: None,
            role: None,
            revision: 0,
        }))
    }

    /// Saves a course's practice profile when `expected_revision` is its current revision. A changed interview date
    /// becomes the target of the decks of the course's interviews, in the same transaction.
    pub async fn save_profile(
        &self,
        request: &SaveLearningPracticeProfileRequestDto,
    ) -> Result<LearningPracticeProfileDto> {
        let program_id = request.program_id.as_str();
        uuid(program_id, "program")?;
        let decks = study_service::interview_deck_ids(&self.pool, program_id).await?;
        let now = now();
        let mut tx = self.pool.begin_write().await.map_err(db)?;
        Self::writer_lock(&mut tx, program_id).await?;
        require_program(&mut tx, program_id).await?;
        let current = profile_in(&mut tx, program_id).await?;
        let current_revision = current.as_ref().map_or(0, |profile| profile.revision);
        if request.expected_revision != current_revision {
            return Err(AppError::Conflict(
                "The practice profile changed since it was read.".into(),
            ));
        }
        let saved_date = current.as_ref().and_then(|profile| profile.interview_at);
        if saved_date != request.interview_at {
            study_service::validate_target(request.interview_at, now)?;
            for deck_id in &decks {
                set_target_in(&mut tx, deck_id, request.interview_at, now).await?;
            }
        }
        let revision = current_revision + 1;
        sqlx::query(
            "INSERT INTO learning_practice_profiles(program_id,details,language,interview_at,company,role,revision,updated_at) VALUES(?,?,?,?,?,?,?,?) ON CONFLICT(program_id) DO UPDATE SET details=excluded.details,language=excluded.language,interview_at=excluded.interview_at,company=excluded.company,role=excluded.role,revision=excluded.revision,updated_at=excluded.updated_at",
        )
        .bind(program_id)
        .bind(&request.details)
        .bind(&request.language)
        .bind(request.interview_at)
        .bind(&request.company)
        .bind(&request.role)
        .bind(revision)
        .bind(now)
        .execute(&mut *tx)
        .await
        .map_err(db)?;
        tx.commit().await.map_err(db)?;
        Ok(LearningPracticeProfileDto {
            program_id: program_id.to_owned(),
            details: request.details.clone(),
            language: request.language.clone(),
            interview_at: request.interview_at,
            company: request.company.clone(),
            role: request.role.clone(),
            revision,
        })
    }
}

/// The saved profile of a course, if any.
pub(crate) async fn profile_in(
    connection: &mut SqliteConnection,
    program_id: &str,
) -> Result<Option<LearningPracticeProfileDto>> {
    let row = sqlx::query(
        "SELECT details,language,interview_at,company,role,revision FROM learning_practice_profiles WHERE program_id=?",
    )
    .bind(program_id)
    .fetch_optional(&mut *connection)
    .await
    .map_err(db)?;
    row.map(|row| {
        Ok::<_, AppError>(LearningPracticeProfileDto {
            program_id: program_id.to_owned(),
            details: row.try_get("details").map_err(db)?,
            language: row.try_get("language").map_err(db)?,
            interview_at: row.try_get("interview_at").map_err(db)?,
            company: row.try_get("company").map_err(db)?,
            role: row.try_get("role").map_err(db)?,
            revision: row.try_get("revision").map_err(db)?,
        })
    })
    .transpose()
}
