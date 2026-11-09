use futures::FutureExt;
use sqlx::{SqliteConnection, SqlitePool};

use lattice_core::shared::error::Result;

pub async fn recorded_owner(pool: &SqlitePool) -> Result<Option<String>> {
    Ok(
        sqlx::query_scalar::<_, String>("SELECT owner_id FROM library_owner WHERE id = 1")
            .fetch_optional(pool)
            .boxed()
            .await?,
    )
}

pub async fn holds_documents(pool: &SqlitePool) -> Result<bool> {
    Ok(
        sqlx::query_scalar::<_, bool>("SELECT EXISTS(SELECT 1 FROM documents)")
            .fetch_one(pool)
            .boxed()
            .await?,
    )
}

pub async fn record_owner(connection: &mut SqliteConnection, id: &str) -> Result<()> {
    sqlx::query("INSERT INTO library_owner (id, owner_id) VALUES (1, ?)")
        .bind(id)
        .execute(connection)
        .await?;
    Ok(())
}
