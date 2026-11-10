use lattice_core::domain::provenance::LabelSource;
use lattice_core::shared::error::{AppError, Result};
use lattice_core::shared::persistence::Transactions;
use sqlx::SqlitePool;

/// Deletes a claim's outcomes under each of `labels`, all or none.
pub(in crate::references::trust) async fn delete_for_labels(
    pool: &SqlitePool,
    claim_id: &str,
    labels: &[LabelSource],
) -> Result<()> {
    let database = |error: sqlx::Error| AppError::Database(error.to_string());
    let mut tx = pool.begin_write().await.map_err(database)?;
    for label in labels {
        sqlx::query("DELETE FROM source_trust_outcomes WHERE claim_id=? AND label_source=?")
            .bind(claim_id)
            .bind(label.as_str())
            .execute(&mut *tx)
            .await
            .map_err(database)?;
    }
    tx.commit().await.map_err(database)
}
