//! Fixtures of the practical tests, shared with lattice-desktop's live evals.

#![allow(clippy::indexing_slicing)]

use crate::dto::AcceptLearningProgramRequestDto;
use crate::persistence::db;
use crate::repository::LearningRepository;
use lattice_core::shared::error::Result;
use lattice_core::shared::persistence::Database;
use sqlx::SqlitePool;

pub fn id() -> String {
    uuid::Uuid::new_v4().to_string()
}

pub async fn fixture_on_pool(
    pool: SqlitePool,
    database: &Database,
) -> Result<(SqlitePool, String, String, String)> {
    let program = crate::test_support::fixture();
    let program_id = program.summary.id.clone();
    let module_id = program.modules[0].id.clone();
    let lesson_id = program.modules[0].lessons[0].id.clone();
    let programs = LearningRepository::new(database.clone());
    programs.create(&program).await?;
    programs
        .accept(&AcceptLearningProgramRequestDto {
            program_id: program_id.clone(),
            expected_revision: 0,
            title: program.summary.title,
        })
        .await?;
    let outcome_id = id();
    sqlx::query("INSERT INTO learning_outcome_definitions(id,program_id,module_id,lesson_id,title,description,ordinal,created_at) VALUES(?,?,?,NULL,?,?,0,?)")
        .bind(&outcome_id)
        .bind(&program_id)
        .bind(&module_id)
        .bind("Apply the central idea")
        .bind("Use the module idea in a practical setting.")
        .bind(chrono::Utc::now().timestamp_millis())
        .execute(&pool)
        .await
        .map_err(db)?;
    Ok((pool, program_id, lesson_id, outcome_id))
}

pub async fn fixture() -> Result<(SqlitePool, Database, String, String, String)> {
    let pool = crate::test_support::pool().await?;
    let database = Database::new(pool.clone());
    let (pool, program_id, lesson_id, outcome_id) = fixture_on_pool(pool, &database).await?;
    Ok((pool, database, program_id, lesson_id, outcome_id))
}
