use super::rows::{
    override_from_row, provenance_from_row, relation_from_row, write_override, write_provenance,
    write_relations, OVERRIDE_COLUMNS, PROVENANCE_COLUMNS, RELATION_COLUMNS,
};
use crate::persistence::db;
use crate::references::trust::pack::{
    PackRelation, SourceTrustPackEntry, SOURCE_TRUST_PACK_VERSION,
};
use lattice_core::domain::provenance::{ProvenanceRecord, SourceRelation, TrustOverride};
use lattice_core::shared::error::Result;
use lattice_core::shared::persistence::WriteTx;
use sqlx::{Row, SqliteConnection};

pub(in crate::references::trust) async fn provenance(
    conn: &mut SqliteConnection,
    program_id: &str,
) -> Result<Vec<ProvenanceRecord>> {
    sqlx::query(sqlx::AssertSqlSafe(format!(
        "SELECT {PROVENANCE_COLUMNS} FROM source_provenance WHERE program_id=? ORDER BY source_version_id"
    )))
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
    sqlx::query(sqlx::AssertSqlSafe(format!(
        "SELECT {RELATION_COLUMNS},detected_at FROM source_relations WHERE program_id=? ORDER BY from_version_id,kind,COALESCE(to_version_id,to_url)"
    )))
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
    sqlx::query(sqlx::AssertSqlSafe(format!(
        "SELECT {OVERRIDE_COLUMNS} FROM source_trust_overrides WHERE scope='source' AND program_id=? ORDER BY source_id"
    )))
    .bind(program_id)
    .fetch_all(&mut *conn)
    .await
    .map_err(db)?
    .iter()
    .map(override_from_row)
    .collect()
}

impl SourceTrustPackEntry {
    /// The course's trust rows, read in the export's snapshot transaction.
    pub async fn read(conn: &mut SqliteConnection, program_id: &str) -> Result<Self> {
        let provenance = provenance(conn, program_id).await?;
        let relations = relations(conn, program_id)
            .await?
            .into_iter()
            .map(|(relation, detected_at)| PackRelation {
                relation,
                detected_at,
            })
            .collect();
        let overrides = source_overrides(conn, program_id).await?;
        Ok(Self {
            version: SOURCE_TRUST_PACK_VERSION,
            provenance,
            relations,
            overrides,
        })
    }

    /// Writes the rows in the import's transaction, after the versions they
    /// hang from.
    pub async fn write(&self, tx: &mut WriteTx, program_id: &str) -> Result<()> {
        for record in &self.provenance {
            write_provenance(tx, record).await?;
        }
        for PackRelation {
            relation,
            detected_at,
        } in &self.relations
        {
            write_relations(tx, program_id, std::slice::from_ref(relation), *detected_at).await?;
        }
        for entry in &self.overrides {
            write_override(tx, entry).await?;
        }
        Ok(())
    }
}
