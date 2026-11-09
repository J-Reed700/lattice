//! Atomic draft checkpoints, including raw citations and their review state.
use crate::{
    dto::*,
    outline_draft::OutlineDraft,
    repository::{compare_sources, insert_source, reference_of, LearningRepository},
};
use lattice_core::shared::error::{AppError, Result};

impl LearningRepository {
    pub async fn outline_draft(&self, id: &str) -> Result<Option<OutlineDraft>> {
        let mut connection = self.pool.acquire().await?;
        Self::outline_draft_on(&mut connection, id).await
    }

    pub(crate) async fn outline_draft_on(
        connection: &mut sqlx::SqliteConnection,
        id: &str,
    ) -> Result<Option<OutlineDraft>> {
        let raw: Option<String> =
            sqlx::query_scalar("SELECT state_json FROM learning_outline_drafts WHERE program_id=?")
                .bind(id)
                .fetch_optional(connection)
                .await?;
        raw.map(|raw| serde_json::from_str(&raw).map_err(AppError::from))
            .transpose()
    }

    pub(crate) async fn checkpoint_outline(
        &self,
        program: &mut LearningProgramDto,
        draft: &mut OutlineDraft,
        sources: &[LearningSourceDto],
        references: &[crate::sources::InitialReference],
    ) -> Result<()> {
        draft.bind_program(program)?;
        draft.stamp();
        let state = serde_json::to_string(draft)?;
        let comparisons =
            compare_sources(&self.pool, &program.summary.id, sources, references).await?;
        let mut tx = self.pool.begin().await?;
        let changed = sqlx::query("UPDATE learning_programs SET revision=revision+1,current_lesson_id=? WHERE id=? AND revision=? AND status='draft'")
            .bind(program.modules.first().and_then(|m|m.lessons.first()).map(|l|&l.id)).bind(&program.summary.id).bind(program.summary.revision).execute(&mut *tx).await?;
        if changed.rows_affected() != 1 {
            return Err(AppError::InvalidInput(
                "The draft changed during repair; reload to see the saved version.".into(),
            ));
        }
        // Repairs keep every module and saved lesson, and may add lessons to a
        // module. Ready lessons are never rewritten here.
        for module in &program.modules {
            let changed = sqlx::query("UPDATE learning_modules SET title=?,summary=?,outcomes_json=?,prerequisite_ids_json=?,project_json=? WHERE id=? AND program_id=?")
                .bind(&module.title).bind(&module.summary).bind(serde_json::to_string(&module.outcomes)?).bind(serde_json::to_string(&module.prerequisite_module_ids)?).bind(serde_json::to_string(&module.project)?).bind(&module.id).bind(&program.summary.id).execute(&mut *tx).await?;
            if changed.rows_affected() != 1 {
                return Err(AppError::InvalidInput(
                    "Repair cannot replace the course structure.".into(),
                ));
            }
            let saved: Vec<String> = sqlx::query_scalar("SELECT id FROM learning_lessons WHERE module_id=? AND program_id=? ORDER BY ordinal")
                .bind(&module.id).bind(&program.summary.id).fetch_all(&mut *tx).await?;
            if saved
                .iter()
                .any(|id| !module.lessons.iter().any(|lesson| &lesson.id == id))
            {
                return Err(AppError::InvalidInput(
                    "Repair cannot remove a saved lesson.".into(),
                ));
            }
            let renumbered = saved.len() != module.lessons.len()
                || saved
                    .iter()
                    .zip(&module.lessons)
                    .any(|(id, lesson)| *id != lesson.id);
            if renumbered {
                // Stage ordinals out of the target range: SQLite checks the unique key per row.
                sqlx::query("UPDATE learning_lessons SET ordinal=ordinal+1000000 WHERE module_id=? AND program_id=?")
                    .bind(&module.id).bind(&program.summary.id).execute(&mut *tx).await?;
            }
            for (ordinal, lesson) in module.lessons.iter().enumerate() {
                if saved.contains(&lesson.id) {
                    let changed = sqlx::query("UPDATE learning_lessons SET title=?,objective=?,estimated_minutes=?,ordinal=COALESCE(?,ordinal) WHERE id=? AND program_id=? AND module_id=? AND preparation='outline'")
                        .bind(&lesson.title).bind(&lesson.objective).bind(lesson.estimated_minutes).bind(renumbered.then_some(ordinal as i64)).bind(&lesson.id).bind(&program.summary.id).bind(&module.id).execute(&mut *tx).await?;
                    if changed.rows_affected() != 1 {
                        return Err(AppError::InvalidInput(
                            "Repair cannot overwrite a prepared lesson.".into(),
                        ));
                    }
                } else {
                    sqlx::query("INSERT INTO learning_lessons(id,program_id,module_id,ordinal,title,objective,estimated_minutes,preparation,completed) VALUES(?,?,?,?,?,?,?,'outline',0)")
                        .bind(&lesson.id).bind(&program.summary.id).bind(&module.id).bind(ordinal as i64).bind(&lesson.title).bind(&lesson.objective).bind(lesson.estimated_minutes).execute(&mut *tx).await?;
                }
            }
        }
        for (source, comparison) in sources.iter().zip(&comparisons) {
            insert_source(
                &mut tx,
                &program.summary.id,
                source,
                reference_of(references, source),
                comparison,
            )
            .await?;
        }
        sqlx::query("UPDATE learning_outline_drafts SET state_json=? WHERE program_id=?")
            .bind(&state)
            .bind(&program.summary.id)
            .execute(&mut *tx)
            .await?;
        sqlx::query("INSERT INTO learning_outline_checkpoints(program_id,revision,state_json,created_at) VALUES(?,?,?,?)").bind(&program.summary.id).bind(program.summary.revision+1).bind(&state).bind(draft.review.updated_at).execute(&mut *tx).await?;
        tx.commit().await?;
        program.summary.revision += 1;
        program.summary.lesson_count = program
            .modules
            .iter()
            .map(|module| module.lessons.len() as i64)
            .sum();
        program.outline_review = Some(draft.review.clone());
        program.sources.extend_from_slice(sources);
        Ok(())
    }
}

pub(crate) async fn insert(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    program: &LearningProgramDto,
    draft: &OutlineDraft,
) -> Result<()> {
    let mut bound = draft.clone();
    bound.bind_program(program)?;
    let state = serde_json::to_string(&bound)?;
    sqlx::query("INSERT INTO learning_outline_drafts(program_id,state_json) VALUES(?,?)")
        .bind(&program.summary.id)
        .bind(&state)
        .execute(&mut **tx)
        .await?;
    sqlx::query("INSERT INTO learning_outline_checkpoints(program_id,revision,state_json,created_at) VALUES(?,?,?,?)").bind(&program.summary.id).bind(program.summary.revision).bind(&state).bind(draft.review.updated_at).execute(&mut **tx).await?;
    Ok(())
}
