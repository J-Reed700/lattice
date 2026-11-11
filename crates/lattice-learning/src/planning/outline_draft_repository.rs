//! Atomic draft checkpoints, including raw citations and their review state.
use crate::{dto::*, outline_draft::OutlineDraft};
use lattice_core::shared::error::Result;

pub(crate) async fn insert(
    tx: &mut sqlx::SqliteConnection,
    program: &LearningProgramDto,
    draft: &OutlineDraft,
) -> Result<()> {
    let mut bound = draft.clone();
    bound.bind_program(program)?;
    let state = serde_json::to_string(&bound)?;
    sqlx::query("INSERT INTO learning_outline_drafts(program_id,state_json) VALUES(?,?)")
        .bind(&program.summary.id)
        .bind(&state)
        .execute(&mut *tx)
        .await?;
    sqlx::query("INSERT INTO learning_outline_checkpoints(program_id,revision,state_json,created_at) VALUES(?,?,?,?)").bind(&program.summary.id).bind(program.summary.revision).bind(&state).bind(draft.review.updated_at).execute(&mut *tx).await?;
    Ok(())
}
