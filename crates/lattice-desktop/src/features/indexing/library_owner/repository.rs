use sqlx::SqliteConnection;

use lattice_core::shared::error::Result;
use lattice_core::shared::persistence::driver::DriverPool;

pub async fn recorded_owner(pool: &impl DriverPool) -> Result<Option<String>> {
    let pool = pool.sqlx_pool();
    Ok(
        sqlx::query_scalar::<_, String>("SELECT owner_id FROM library_owner WHERE id = 1")
            .fetch_optional(pool)
            .await?,
    )
}

pub async fn holds_documents(pool: &impl DriverPool) -> Result<bool> {
    let pool = pool.sqlx_pool();
    Ok(
        sqlx::query_scalar::<_, bool>("SELECT EXISTS(SELECT 1 FROM documents)")
            .fetch_one(pool)
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
