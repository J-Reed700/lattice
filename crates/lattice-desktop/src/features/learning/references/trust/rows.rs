//! How each trust value is written to and read from its table: shared by
//! the store, the capture hook (inside the version's transaction) and the
//! pack import (inside the import's transaction), so every writer stores
//! the same columns the same way.

use crate::features::learning::persistence::db;
use lattice_core::domain::provenance::{
    ExtractedFrom, Origin, OutboundLink, OverrideScope, ProvenanceRecord, ProvenanceSignals,
    RelationEvidence, RelationKind, RelationTarget, SiteStanding, Sketch, SourceRelation,
    SubjectClass, TrustLevel, TrustOverride,
};
use lattice_core::shared::error::{AppError, Result};
use serde::{de::DeserializeOwned, Serialize};
use sqlx::sqlite::SqliteRow;
use sqlx::{Row, SqliteConnection};

/// The columns `provenance_from_row` reads.
pub(super) const PROVENANCE_COLUMNS: &str = "program_id,source_version_id,origin_json,signals_json,signals_sha256,sketch,links_json,extracted_from,extraction_version,extracted_at";

/// The columns `relation_from_row` reads.
pub(super) const RELATION_COLUMNS: &str = "from_version_id,to_version_id,to_url,kind,evidence_json";

/// The columns `override_from_row` reads.
pub(super) const OVERRIDE_COLUMNS: &str =
    "scope,program_id,source_id,registrable_domain,class_json,level,standing,note,set_at";

pub(super) fn to_json<T: Serialize>(value: &T) -> Result<String> {
    serde_json::to_string(value).map_err(|e| AppError::Serialization(e.to_string()))
}

pub(super) fn from_json<T: DeserializeOwned>(text: &str, column: &str) -> Result<T> {
    serde_json::from_str(text)
        .map_err(|e| AppError::Database(format!("Invalid stored {column}: {e}")))
}

/// A stored vocabulary value the table's CHECK should have refused.
pub(super) fn unknown(column: &str, value: &str) -> AppError {
    AppError::Database(format!("Unknown stored {column}: {value}"))
}

/// Token counts are u64 in the domain and INTEGER in SQLite.
pub(super) fn tokens_to_column(count: u64) -> Result<i64> {
    i64::try_from(count).map_err(|_| AppError::InvalidInput("Token count is out of range.".into()))
}

pub(super) fn tokens_from_column(count: i64) -> Result<u64> {
    u64::try_from(count).map_err(|_| AppError::Database("Negative stored token count".into()))
}

/// Creates or replaces one version's provenance row.
pub(super) async fn write_provenance(
    conn: &mut SqliteConnection,
    record: &ProvenanceRecord,
) -> Result<()> {
    sqlx::query(
        "INSERT OR REPLACE INTO source_provenance(program_id,source_version_id,origin_key,site,origin_json,canonical_url,signals_json,signals_sha256,sketch,links_json,extracted_from,extraction_version,extracted_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?)",
    )
    .bind(&record.program_id)
    .bind(&record.source_version_id)
    .bind(&record.origin.key.0)
    .bind(record.origin.site())
    .bind(to_json(&record.origin)?)
    .bind(&record.origin.canonical_url)
    .bind(to_json(&record.signals)?)
    .bind(&record.signals_sha256)
    .bind(record.sketch.to_le_bytes())
    .bind(to_json(&record.links)?)
    .bind(record.extracted_from.as_str())
    .bind(&record.extraction_version)
    .bind(record.extracted_at)
    .execute(&mut *conn)
    .await
    .map_err(db)?;
    Ok(())
}

pub(super) fn provenance_from_row(row: &SqliteRow) -> Result<ProvenanceRecord> {
    let extracted_from: String = row.get("extracted_from");
    let sketch: Vec<u8> = row.get("sketch");
    Ok(ProvenanceRecord {
        source_version_id: row.get("source_version_id"),
        program_id: row.get("program_id"),
        origin: from_json::<Origin>(row.get::<&str, _>("origin_json"), "origin")?,
        signals: from_json::<ProvenanceSignals>(row.get::<&str, _>("signals_json"), "signals")?,
        signals_sha256: row.get("signals_sha256"),
        sketch: Sketch::from_le_bytes(&sketch)
            .ok_or_else(|| AppError::Database("Invalid stored sketch".into()))?,
        links: from_json::<Vec<OutboundLink>>(row.get::<&str, _>("links_json"), "links")?,
        extracted_from: ExtractedFrom::from_column(&extracted_from)
            .ok_or_else(|| unknown("extracted_from", &extracted_from))?,
        extraction_version: row.get("extraction_version"),
        extracted_at: row.get("extracted_at"),
    })
}

/// Adds relations; an edge already recorded (`from`, `kind`, `to`) is
/// replaced with the new evidence and time.
pub(super) async fn write_relations(
    conn: &mut SqliteConnection,
    program_id: &str,
    relations: &[SourceRelation],
    detected_at: i64,
) -> Result<()> {
    for relation in relations {
        let (to_version, to_url) = match &relation.to {
            RelationTarget::Version(id) => (Some(id.as_str()), None),
            RelationTarget::Url(url) => (None, Some(url.as_str())),
        };
        sqlx::query(
            "INSERT OR REPLACE INTO source_relations(program_id,from_version_id,to_version_id,to_url,kind,evidence_json,detected_at) VALUES(?,?,?,?,?,?,?)",
        )
        .bind(program_id)
        .bind(&relation.from)
        .bind(to_version)
        .bind(to_url)
        .bind(relation.kind.as_str())
        .bind(to_json(&relation.evidence)?)
        .bind(detected_at)
        .execute(&mut *conn)
        .await
        .map_err(db)?;
    }
    Ok(())
}

pub(super) fn relation_from_row(row: &SqliteRow) -> Result<SourceRelation> {
    let kind: String = row.get("kind");
    let to_version: Option<String> = row.get("to_version_id");
    let to_url: Option<String> = row.get("to_url");
    let to = match (to_version, to_url) {
        (Some(id), None) => RelationTarget::Version(id),
        (None, Some(url)) => RelationTarget::Url(url),
        _ => return Err(AppError::Database("Invalid stored relation target".into())),
    };
    Ok(SourceRelation {
        from: row.get("from_version_id"),
        to,
        kind: RelationKind::from_column(&kind).ok_or_else(|| unknown("relation kind", &kind))?,
        evidence: from_json::<RelationEvidence>(row.get::<&str, _>("evidence_json"), "evidence")?,
    })
}

/// The row that holds `scope`, as a WHERE clause and its binds in order.
pub(super) fn scope_filter(scope: &OverrideScope) -> (&'static str, Vec<String>) {
    match scope {
        OverrideScope::Source {
            program_id,
            source_id,
        } => (
            "scope='source' AND program_id=? AND source_id=?",
            vec![program_id.clone(), source_id.clone()],
        ),
        OverrideScope::Site { registrable_domain } => (
            "scope='site' AND registrable_domain=?",
            vec![registrable_domain.clone()],
        ),
        OverrideScope::SiteSubject {
            registrable_domain,
            class,
        } => (
            "scope='site_subject' AND registrable_domain=? AND class_key=?",
            vec![registrable_domain.clone(), class.key.0.clone()],
        ),
    }
}

/// Removes the override at `scope`; `false` when there was none.
pub(super) async fn delete_override(
    conn: &mut SqliteConnection,
    scope: &OverrideScope,
) -> Result<bool> {
    let (filter, binds) = scope_filter(scope);
    let sql = format!("DELETE FROM source_trust_overrides WHERE {filter}");
    let mut query = sqlx::query(&sql);
    for bind in &binds {
        query = query.bind(bind);
    }
    let done = query.execute(&mut *conn).await.map_err(db)?;
    Ok(done.rows_affected() > 0)
}

/// Creates or replaces the override at its scope.
pub(super) async fn write_override(
    conn: &mut SqliteConnection,
    trust_override: &TrustOverride,
) -> Result<()> {
    delete_override(conn, &trust_override.scope()).await?;
    let insert = "INSERT INTO source_trust_overrides(scope,program_id,source_id,registrable_domain,class_key,class_json,level,standing,note,set_at) VALUES(?,?,?,?,?,?,?,?,?,?)";
    let query = match trust_override {
        TrustOverride::Source {
            program_id,
            source_id,
            level,
            note,
            set_at,
        } => sqlx::query(insert)
            .bind("source")
            .bind(program_id)
            .bind(source_id)
            .bind(None::<String>)
            .bind(None::<String>)
            .bind(None::<String>)
            .bind(level.as_str())
            .bind(None::<String>)
            .bind(note)
            .bind(set_at),
        TrustOverride::Site {
            registrable_domain,
            standing,
            note,
            set_at,
        } => sqlx::query(insert)
            .bind("site")
            .bind(None::<String>)
            .bind(None::<String>)
            .bind(registrable_domain)
            .bind(None::<String>)
            .bind(None::<String>)
            .bind(None::<String>)
            .bind(standing.as_str())
            .bind(note)
            .bind(set_at),
        TrustOverride::SiteSubject {
            registrable_domain,
            class,
            level,
            note,
            set_at,
        } => sqlx::query(insert)
            .bind("site_subject")
            .bind(None::<String>)
            .bind(None::<String>)
            .bind(registrable_domain)
            .bind(class.key.0.clone())
            .bind(to_json(class)?)
            .bind(level.as_str())
            .bind(None::<String>)
            .bind(note)
            .bind(set_at),
    };
    query.execute(&mut *conn).await.map_err(db)?;
    Ok(())
}

pub(super) fn override_from_row(row: &SqliteRow) -> Result<TrustOverride> {
    let scope: String = row.get("scope");
    let note: Option<String> = row.get("note");
    let set_at: i64 = row.get("set_at");
    let level = || -> Result<TrustLevel> {
        let value: String = row.get("level");
        TrustLevel::from_column(&value).ok_or_else(|| unknown("level", &value))
    };
    match scope.as_str() {
        "source" => Ok(TrustOverride::Source {
            program_id: row.get("program_id"),
            source_id: row.get("source_id"),
            level: level()?,
            note,
            set_at,
        }),
        "site" => {
            let standing: String = row.get("standing");
            Ok(TrustOverride::Site {
                registrable_domain: row.get("registrable_domain"),
                standing: SiteStanding::from_column(&standing)
                    .ok_or_else(|| unknown("standing", &standing))?,
                note,
                set_at,
            })
        }
        "site_subject" => Ok(TrustOverride::SiteSubject {
            registrable_domain: row.get("registrable_domain"),
            class: from_json::<SubjectClass>(row.get::<&str, _>("class_json"), "subject class")?,
            level: level()?,
            note,
            set_at,
        }),
        other => Err(unknown("override scope", other)),
    }
}
