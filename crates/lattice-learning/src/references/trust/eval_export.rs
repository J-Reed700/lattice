//! The user's overrides as provenance eval examples (§19 "Export provenance
//! eval", §21): one `sources-v1.jsonl`-shaped line per overridden source
//! version, labelled by the user. A source override states the version's
//! level; a site override its publisher's standing; a site-for-subject
//! override its level for claims in that class (the line's subject is the
//! class). Only what the user stated is in `expected`; `live_provenance_eval`
//! scores the fields a line has. Lines are deduplicated by signals, since a
//! site's identical pages carry one label.

use super::repository::eval_export::{all_overrides, site_versions, source_versions, CoveredRow};
use super::repository::SqliteSourceTrustStore;
use lattice_core::application::ports::SourceTrustStore;
use lattice_core::domain::provenance::{ProvenanceRecord, TrustOverride};
use lattice_core::shared::error::{AppError, Result};
use lattice_core::shared::persistence::Database;
use serde_json::{json, Value};
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

async fn covered(
    database: &Database,
    store: &SqliteSourceTrustStore,
    entry: &TrustOverride,
) -> Result<Vec<Covered>> {
    let rows: Vec<CoveredRow> = match entry {
        TrustOverride::Source {
            program_id,
            source_id,
            ..
        } => source_versions(database, program_id, source_id).await?,
        TrustOverride::Site {
            registrable_domain, ..
        }
        | TrustOverride::SiteSubject {
            registrable_domain, ..
        } => site_versions(database, registrable_domain).await?,
    };
    let mut covered = Vec::new();
    for row in rows {
        if let Some(record) = store.provenance(&row.version_id).await? {
            covered.push(Covered {
                record,
                added_by: row.added_by,
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
pub async fn lines(database: &Database, store: &SqliteSourceTrustStore) -> Result<Vec<String>> {
    let entries = all_overrides(database).await?;
    let mut lines = Vec::new();
    let mut seen = HashSet::new();
    for entry in &entries {
        let (scope, subject, expected, note) = labelled(entry);
        for version in covered(database, store, entry).await? {
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
pub async fn export(
    database: &Database,
    store: &SqliteSourceTrustStore,
    path: &std::path::Path,
) -> Result<usize> {
    let lines = lines(database, store).await?;
    let mut text = lines.join("\n");
    if !text.is_empty() {
        text.push('\n');
    }
    tokio::fs::write(path, text)
        .await
        .map_err(|error| AppError::FileStorage(error.to_string()))?;
    Ok(lines.len())
}
