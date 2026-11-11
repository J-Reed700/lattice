//! Stored practice-fit verdicts: one per course and kind of work, on the basis they were decided on.
use super::*;
use crate::practice::practice_fit::{CourseOutline, OutlineLesson, OutlineModule};
use lattice_core::shared::persistence::Transactions;

/// A verdict as stored.
pub(crate) struct StoredFit {
    pub kind: String,
    pub basis: String,
    pub fits: bool,
    pub reason: String,
}

/// A verdict on one kind to store.
pub(crate) struct KindVerdict {
    pub kind: String,
    pub fits: bool,
    pub reason: String,
}

/// What a fit answer rests on, read in one transaction.
pub(crate) struct FitState {
    pub profile: LearningPracticeProfileDto,
    pub outline: CourseOutline,
    pub stored: Vec<StoredFit>,
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
        let stored: Vec<(String, String, i64, String)> = sqlx::query_as(
            "SELECT kind,basis,fits,reason FROM learning_practice_fits WHERE program_id=? ORDER BY kind",
        )
        .bind(program_id)
        .fetch_all(&mut *tx)
        .await
        .map_err(db)?;
        let profile = course_profile(&mut tx, program_id).await?;
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
            profile,
            outline: CourseOutline { title, modules },
            stored: stored
                .into_iter()
                .map(|(kind, basis, fits, reason)| StoredFit {
                    kind,
                    basis,
                    fits: fits != 0,
                    reason,
                })
                .collect(),
        })
    }

    /// Stores the verdicts on `basis`, replacing the course's earlier ones. A course that is gone gets none.
    pub(crate) async fn record_fits(
        &self,
        program_id: &str,
        basis: &str,
        verdicts: &[KindVerdict],
        model: &str,
        now: i64,
    ) -> Result<()> {
        uuid(program_id, "program")?;
        let mut tx = self.pool.begin_write().await.map_err(db)?;
        Self::writer_lock(&mut tx, program_id).await?;
        sqlx::query("DELETE FROM learning_practice_fits WHERE program_id=?")
            .bind(program_id)
            .execute(&mut *tx)
            .await
            .map_err(db)?;
        for verdict in verdicts {
            sqlx::query(
                "INSERT INTO learning_practice_fits(program_id,kind,basis,fits,reason,model,decided_at) SELECT ?1,?2,?3,?4,?5,?6,?7 WHERE EXISTS(SELECT 1 FROM learning_programs WHERE id=?1)",
            )
            .bind(program_id)
            .bind(&verdict.kind)
            .bind(basis)
            .bind(i64::from(verdict.fits))
            .bind(&verdict.reason)
            .bind(model)
            .bind(now)
            .execute(&mut *tx)
            .await
            .map_err(db)?;
        }
        tx.commit().await.map_err(db)
    }
}
