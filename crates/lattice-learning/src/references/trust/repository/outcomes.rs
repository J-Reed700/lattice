use lattice_core::domain::provenance::{LabelSource, OutcomeRecord, ProvenanceRecord};
use lattice_core::shared::error::Result;
use sqlx::SqliteConnection;

use super::rows::{provenance_from_row, PROVENANCE_COLUMNS};
use super::{insert_outcome, outcome_from_row};
use crate::persistence::db;

/// Deletes a claim's outcomes under each of `labels`, on the caller's unit of work.
pub(in crate::references::trust) async fn delete_for_labels_in(
    conn: &mut SqliteConnection,
    claim_id: &str,
    labels: &[LabelSource],
) -> Result<()> {
    for label in labels {
        sqlx::query("DELETE FROM source_trust_outcomes WHERE claim_id=? AND label_source=?")
            .bind(claim_id)
            .bind(label.as_str())
            .execute(&mut *conn)
            .await
            .map_err(db)?;
    }
    Ok(())
}

/// Every outcome, oldest first.
pub(in crate::references::trust) async fn list_in(
    conn: &mut SqliteConnection,
) -> Result<Vec<OutcomeRecord>> {
    sqlx::query("SELECT id,program_id,lesson_id,claim_id,source_version_id,origin_key,level,claim_kind,decision_sufficient,decision_right,label_source,policy,at FROM source_trust_outcomes ORDER BY at,rowid")
        .fetch_all(&mut *conn)
        .await
        .map_err(db)?
        .iter()
        .map(outcome_from_row)
        .collect()
}

/// The provenance recorded for a source version, when there is one.
pub(in crate::references::trust) async fn provenance_in(
    conn: &mut SqliteConnection,
    source_version_id: &str,
) -> Result<Option<ProvenanceRecord>> {
    let sql =
        format!("SELECT {PROVENANCE_COLUMNS} FROM source_provenance WHERE source_version_id=?");
    sqlx::query(&sql)
        .bind(source_version_id)
        .fetch_optional(&mut *conn)
        .await
        .map_err(db)?
        .as_ref()
        .map(provenance_from_row)
        .transpose()
}

/// Inserts the outcomes on the caller's unit of work.
pub(in crate::references::trust) async fn insert_all_in(
    conn: &mut SqliteConnection,
    outcomes: &[OutcomeRecord],
) -> Result<()> {
    for outcome in outcomes {
        insert_outcome(&mut *conn, outcome).await?;
    }
    Ok(())
}
