use super::rows::{
    override_from_row, provenance_from_row, relation_from_row, OVERRIDE_COLUMNS,
    PROVENANCE_COLUMNS, RELATION_COLUMNS,
};
use crate::persistence::db;
use lattice_core::domain::provenance::{ProvenanceRecord, SourceRelation, TrustOverride};
use lattice_core::shared::error::Result;
use sqlx::{Row, SqliteConnection};

pub(in crate::references::trust) async fn provenance(
    conn: &mut SqliteConnection,
    program_id: &str,
) -> Result<Vec<ProvenanceRecord>> {
    sqlx::query(&format!(
        "SELECT {PROVENANCE_COLUMNS} FROM source_provenance WHERE program_id=? ORDER BY source_version_id"
    ))
    .bind(program_id)
    .fetch_all(&mut *conn)
    .await
    .map_err(db)?
    .iter()
    .map(provenance_from_row)
    .collect()
}

/// Each relation with when it was detected.
pub(in crate::references::trust) async fn relations(
    conn: &mut SqliteConnection,
    program_id: &str,
) -> Result<Vec<(SourceRelation, i64)>> {
    sqlx::query(&format!(
        "SELECT {RELATION_COLUMNS},detected_at FROM source_relations WHERE program_id=? ORDER BY from_version_id,kind,COALESCE(to_version_id,to_url)"
    ))
    .bind(program_id)
    .fetch_all(&mut *conn)
    .await
    .map_err(db)?
    .iter()
    .map(|row| Ok((relation_from_row(row)?, row.get("detected_at"))))
    .collect()
}

pub(in crate::references::trust) async fn source_overrides(
    conn: &mut SqliteConnection,
    program_id: &str,
) -> Result<Vec<TrustOverride>> {
    sqlx::query(&format!(
        "SELECT {OVERRIDE_COLUMNS} FROM source_trust_overrides WHERE scope='source' AND program_id=? ORDER BY source_id"
    ))
    .bind(program_id)
    .fetch_all(&mut *conn)
    .await
    .map_err(db)?
    .iter()
    .map(override_from_row)
    .collect()
}
