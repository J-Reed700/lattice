use lattice_core::shared::error::Result;
use lattice_core::shared::persistence::{Database, WriteTx};

pub async fn recorded_owner(db: &Database) -> Result<Option<String>> {
    let mut read = db.begin_read().await?;
    let owner = sqlx::query_scalar::<_, String>("SELECT owner_id FROM library_owner WHERE id = 1")
        .fetch_optional(&mut *read)
        .await?;
    read.commit().await?;
    Ok(owner)
}

pub async fn holds_documents(db: &Database) -> Result<bool> {
    let mut read = db.begin_read().await?;
    let holds = sqlx::query_scalar::<_, bool>("SELECT EXISTS(SELECT 1 FROM documents)")
        .fetch_one(&mut *read)
        .await?;
    read.commit().await?;
    Ok(holds)
}

pub async fn record_owner(tx: &mut WriteTx, id: &str) -> Result<()> {
    sqlx::query("INSERT INTO library_owner (id, owner_id) VALUES (1, ?)")
        .bind(id)
        .execute(&mut **tx)
        .await?;
    Ok(())
}
