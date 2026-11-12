use lattice_core::shared::persistence::Database;
use lattice_core::shared::Result;

pub(super) async fn program_ids(database: &Database) -> Result<Vec<String>> {
    let mut read = database.begin_read().await?;
    let ids = sqlx::query_scalar("SELECT id FROM learning_programs")
        .fetch_all(&mut *read)
        .await?;
    read.commit().await?;
    Ok(ids)
}

pub(super) async fn program_exists(database: &Database, program: &str) -> Result<bool> {
    let mut read = database.begin_read().await?;
    let count = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM learning_programs WHERE id=?")
        .bind(program)
        .fetch_one(&mut *read)
        .await?;
    read.commit().await?;
    Ok(count > 0)
}
