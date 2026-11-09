//! Stored practice-fit verdicts: one per course, on the outline basis it was decided on.
use super::*;
use crate::practice::practice_fit::{CourseOutline, OutlineLesson, OutlineModule};
use lattice_core::shared::persistence::Transactions;

/// A verdict as stored.
pub(crate) struct StoredFit {
    pub basis: String,
    pub teaches_code: bool,
    pub reason: String,
}

/// What a fit answer rests on, read in one transaction.
pub(crate) struct FitState {
    pub language: Option<String>,
    pub outline: CourseOutline,
    pub stored: Option<StoredFit>,
}

impl LearningPracticeRepository {
    pub(crate) async fn fit_state(&self, program_id: &str) -> Result<FitState> {
        uuid(program_id, "program")?;
        let mut tx = self.pool.begin_read().await.map_err(db)?;
        let title: String = sqlx::query_scalar("SELECT title FROM learning_programs WHERE id=?")
            .bind(program_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(db)?
            .ok_or_else(|| AppError::NotFound("Learning program not found".into()))?;
        let modules: Vec<(String, String)> = sqlx::query_as(
            "SELECT id,title FROM learning_modules WHERE program_id=? ORDER BY ordinal",
        )
        .bind(program_id)
        .fetch_all(&mut *tx)
        .await
        .map_err(db)?;
        let lessons: Vec<(String, String, String)> = sqlx::query_as(
            "SELECT l.module_id,l.title,l.objective FROM learning_lessons l JOIN learning_modules m ON m.id=l.module_id WHERE l.program_id=? ORDER BY m.ordinal,l.ordinal",
        )
        .bind(program_id)
        .fetch_all(&mut *tx)
        .await
        .map_err(db)?;
        let stored: Option<(String, i64, String)> = sqlx::query_as(
            "SELECT basis,teaches_code,reason FROM learning_practice_fits WHERE program_id=?",
        )
        .bind(program_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(db)?;
        let language = profile_in(&mut tx, program_id)
            .await?
            .and_then(|profile| profile.language);
        tx.commit().await.map_err(db)?;
        let modules = modules
            .into_iter()
            .map(|(module_id, module_title)| OutlineModule {
                title: module_title,
                lessons: lessons
                    .iter()
                    .filter(|(lesson_module, _, _)| *lesson_module == module_id)
                    .map(|(_, lesson_title, objective)| OutlineLesson {
                        title: lesson_title.clone(),
                        objective: objective.clone(),
                    })
                    .collect(),
            })
            .collect();
        Ok(FitState {
            language,
            outline: CourseOutline { title, modules },
            stored: stored.map(|(basis, teaches_code, reason)| StoredFit {
                basis,
                teaches_code: teaches_code != 0,
                reason,
            }),
        })
    }

    /// Stores the verdict on `basis`, replacing the course's earlier one.
    pub(crate) async fn record_fit(
        &self,
        program_id: &str,
        basis: &str,
        teaches_code: bool,
        reason: &str,
        model: &str,
        now: i64,
    ) -> Result<()> {
        uuid(program_id, "program")?;
        let mut tx = self.pool.begin_write().await.map_err(db)?;
        Self::writer_lock(&mut tx, program_id).await?;
        sqlx::query(
            "INSERT INTO learning_practice_fits(program_id,basis,teaches_code,reason,model,decided_at) SELECT ?1,?2,?3,?4,?5,?6 WHERE EXISTS(SELECT 1 FROM learning_programs WHERE id=?1) ON CONFLICT(program_id) DO UPDATE SET basis=excluded.basis,teaches_code=excluded.teaches_code,reason=excluded.reason,model=excluded.model,decided_at=excluded.decided_at",
        )
        .bind(program_id)
        .bind(basis)
        .bind(i64::from(teaches_code))
        .bind(reason)
        .bind(model)
        .bind(now)
        .execute(&mut *tx)
        .await
        .map_err(db)?;
        tx.commit().await.map_err(db)
    }
}
