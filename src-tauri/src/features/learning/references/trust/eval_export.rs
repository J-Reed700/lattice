//! The user's overrides as provenance eval examples (§19 "Export provenance
//! eval", §21): one `sources-v1.jsonl`-shaped line per overridden source
//! version, labelled by the user. A source override states the version's
//! level; a site override its publisher's standing; a site-for-subject
//! override its level for claims in that class (the line's subject is the
//! class). Only what the user stated is in `expected`; `live_provenance_eval`
//! scores the fields a line has. Lines are deduplicated by signals, since a
//! site's identical pages carry one label.

use super::repository::SqliteSourceTrustStore;
use super::rows::{override_from_row, OVERRIDE_COLUMNS};
use crate::features::learning::persistence::db;
use lattice_core::application::ports::SourceTrustStore;
use lattice_core::domain::provenance::{ProvenanceRecord, TrustOverride};
use lattice_core::shared::error::{AppError, Result};
use serde_json::{json, Value};
use sqlx::Row;
use std::collections::HashSet;

/// The file the save dialog suggests.
pub fn export_file_name(date: &str) -> String {
    format!("provenance-sources-user-{date}.jsonl")
}

/// Who labelled every exported line.
const LABELLER: &str = "user";

/// One version an override covers, with who added its source.
struct Covered {
    record: ProvenanceRecord,
    added_by: String,
}

async fn covered(store: &SqliteSourceTrustStore, entry: &TrustOverride) -> Result<Vec<Covered>> {
    let rows = match entry {
        TrustOverride::Source {
            program_id,
            source_id,
            ..
        } => sqlx::query(
            "SELECT v.id AS version_id, l.added_by FROM learning_source_library l \
             JOIN learning_source_versions v ON v.program_id = l.program_id AND v.id = l.active_version_id \
             WHERE l.program_id=? AND l.id=? AND l.deleted_at IS NULL",
        )
        .bind(program_id)
        .bind(source_id)
        .fetch_all(store.pool())
        .await,
        TrustOverride::Site {
            registrable_domain, ..
        }
        | TrustOverride::SiteSubject {
            registrable_domain, ..
        } => sqlx::query(
            "SELECT p.source_version_id AS version_id, l.added_by FROM source_provenance p \
             JOIN learning_source_versions v ON v.program_id = p.program_id AND v.id = p.source_version_id \
             JOIN learning_source_library l ON l.program_id = v.program_id AND l.id = v.source_id \
             WHERE p.site=? ORDER BY p.program_id, p.source_version_id",
        )
        .bind(registrable_domain)
        .fetch_all(store.pool())
        .await,
    }
    .map_err(db)?;
    let mut covered = Vec::new();
    for row in rows {
        let version: String = row.try_get("version_id").map_err(db)?;
        if let Some(record) = store.provenance(&version).await? {
            covered.push(Covered {
                record,
                added_by: row.try_get("added_by").map_err(db)?,
            });
        }
    }
    Ok(covered)
}

/// What the override states about each version it covers.
fn labelled(entry: &TrustOverride) -> (&'static str, Value, Value, Option<&str>) {
    match entry {
        TrustOverride::Source { level, note, .. } => (
            "source",
            Value::Null,
            json!({ "level": level.as_str() }),
            note.as_deref(),
        ),
        TrustOverride::Site { standing, note, .. } => (
            "site",
            Value::Null,
            json!({ "standing": standing.as_str() }),
            note.as_deref(),
        ),
        TrustOverride::SiteSubject {
            class, level, note, ..
        } => (
            "site_subject",
            json!({
                "name": class.label,
                "kind": class.kinds.first().map_or("other", |kind| kind.as_str()),
                "aliases": [],
                "classes": [class.label],
            }),
            json!({ "level": level.as_str() }),
            note.as_deref(),
        ),
    }
}

/// The eval lines, in override order.
pub async fn lines(store: &SqliteSourceTrustStore) -> Result<Vec<String>> {
    let entries = sqlx::query(&format!(
        "SELECT {OVERRIDE_COLUMNS} FROM source_trust_overrides ORDER BY scope, registrable_domain, class_key, program_id, source_id"
    ))
    .fetch_all(store.pool())
    .await
    .map_err(db)?
    .iter()
    .map(override_from_row)
    .collect::<Result<Vec<_>>>()?;
    let mut lines = Vec::new();
    let mut seen = HashSet::new();
    for entry in &entries {
        let (scope, subject, expected, note) = labelled(entry);
        for version in covered(store, entry).await? {
            if !seen.insert((
                scope,
                subject.to_string(),
                version.record.signals_sha256.clone(),
            )) {
                continue;
            }
            let line = json!({
                "id": format!("user-{scope}-{}", lines.len() + 1),
                "area": "user",
                "language": Value::Null,
                "subject": subject,
                "added_by": version.added_by,
                "signals": version.record.signals,
                "expected": expected,
                "rationale": note.map_or_else(
                    || format!("Set by the user as a {} override.", scope.replace('_', "-")),
                    str::to_owned,
                ),
                "labeller": LABELLER,
                "origin": "real",
            });
            lines.push(serde_json::to_string(&line)?);
        }
    }
    Ok(lines)
}

/// Writes the lines to `path`; answers how many.
pub async fn export(store: &SqliteSourceTrustStore, path: &std::path::Path) -> Result<usize> {
    let lines = lines(store).await?;
    let mut text = lines.join("\n");
    if !text.is_empty() {
        text.push('\n');
    }
    tokio::fs::write(path, text)
        .await
        .map_err(|error| AppError::FileStorage(error.to_string()))?;
    Ok(lines.len())
}
