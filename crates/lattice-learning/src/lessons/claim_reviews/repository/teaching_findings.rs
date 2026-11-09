//! The `learning_lesson_teaching_findings` table.

use std::collections::BTreeSet;

use lattice_core::shared::error::{AppError, Result};
use sqlx::{Row, SqliteConnection, SqlitePool};

use crate::persistence::db;

/// A finding's columns that tell whether it still holds.
pub(in crate::lessons::claim_reviews) struct HeldRow {
    pub kind: String,
    pub site: String,
    pub unit_index: Option<i64>,
    pub unit_sha256: String,
}

/// A finding as stored.
pub(in crate::lessons::claim_reviews) struct StoredFinding {
    pub finding_key: String,
    pub kind: String,
    pub site: String,
    pub unit_index: Option<i64>,
    pub unit_title: String,
    pub note: String,
    pub summary: Option<String>,
    pub passage: Option<String>,
    pub sentence: Option<String>,
    pub repairs: i64,
    pub state: String,
    pub resolution: Option<String>,
    pub resolved_at: Option<i64>,
    pub part_changed: bool,
}

/// The values of one finding to record.
pub(in crate::lessons::claim_reviews) struct FindingWrite<'a> {
    pub key: &'a str,
    pub kind: &'a str,
    pub site: &'a str,
    pub unit_index: Option<i64>,
    pub unit_title: &'a str,
    pub unit_sha256: &'a str,
    pub note: &'a str,
    pub summary: Option<&'a str>,
    pub passage: Option<&'a str>,
    pub sentence: Option<&'a str>,
    pub repairs: i64,
    pub job_id: &'a str,
    pub found_at: i64,
}

pub(in crate::lessons::claim_reviews) async fn held_rows(
    pool: &SqlitePool,
    lesson_id: &str,
) -> Result<Vec<HeldRow>> {
    let rows = sqlx::query(
        "SELECT kind, site, unit_index, unit_sha256 FROM learning_lesson_teaching_findings WHERE lesson_id=?",
    )
    .bind(lesson_id)
    .fetch_all(pool)
    .await
    .map_err(db)?;
    Ok(rows
        .iter()
        .map(|row| HeldRow {
            kind: row.get("kind"),
            site: row.get("site"),
            unit_index: row.get("unit_index"),
            unit_sha256: row.get("unit_sha256"),
        })
        .collect())
}

/// Deletes the lesson's findings whose keys are not in `keys_json`.
pub(in crate::lessons::claim_reviews) async fn delete_not_raised(
    connection: &mut SqliteConnection,
    lesson_id: &str,
    keys_json: &str,
) -> Result<()> {
    sqlx::query(
        "DELETE FROM learning_lesson_teaching_findings WHERE lesson_id=? AND finding_key NOT IN (SELECT value FROM json_each(?))",
    )
    .bind(lesson_id)
    .bind(keys_json)
    .execute(&mut *connection)
    .await
    .map_err(db)?;
    Ok(())
}

pub(in crate::lessons::claim_reviews) async fn upsert(
    connection: &mut SqliteConnection,
    lesson_id: &str,
    finding: &FindingWrite<'_>,
) -> Result<()> {
    sqlx::query(
        "INSERT INTO learning_lesson_teaching_findings \
         (lesson_id, finding_key, kind, site, unit_index, unit_title, unit_sha256, note, summary, passage, sentence, repairs, state, job_id, found_at, kept_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 'waiting', ?, ?, NULL) \
         ON CONFLICT(lesson_id, finding_key) DO UPDATE SET \
         unit_title=excluded.unit_title, note=excluded.note, summary=excluded.summary, passage=excluded.passage, \
         sentence=excluded.sentence, \
         repairs=max(repairs, excluded.repairs), job_id=excluded.job_id, found_at=excluded.found_at, \
         state=CASE WHEN state IN ('kept','resolved') AND unit_sha256=excluded.unit_sha256 THEN state ELSE 'waiting' END, \
         kept_at=CASE WHEN state='kept' AND unit_sha256=excluded.unit_sha256 THEN kept_at ELSE NULL END, \
         resolution=CASE WHEN state='resolved' AND unit_sha256=excluded.unit_sha256 THEN resolution ELSE NULL END, \
         resolved_at=CASE WHEN state='resolved' AND unit_sha256=excluded.unit_sha256 THEN resolved_at ELSE NULL END, \
         unit_sha256=excluded.unit_sha256",
    )
    .bind(lesson_id)
    .bind(finding.key)
    .bind(finding.kind)
    .bind(finding.site)
    .bind(finding.unit_index)
    .bind(finding.unit_title)
    .bind(finding.unit_sha256)
    .bind(finding.note)
    .bind(finding.summary)
    .bind(finding.passage)
    .bind(finding.sentence)
    .bind(finding.repairs)
    .bind(finding.job_id)
    .bind(finding.found_at)
    .execute(&mut *connection)
    .await
    .map_err(db)?;
    Ok(())
}

/// The lesson's findings: by place, then kind.
pub(in crate::lessons::claim_reviews) async fn stored_findings(
    pool: &SqlitePool,
    lesson_id: &str,
) -> Result<Vec<StoredFinding>> {
    let rows = sqlx::query(
        "SELECT finding_key, kind, site, unit_index, unit_title, note, summary, passage, sentence, repairs, state, resolution, resolved_at, \
         part_changed_at IS NOT NULL AND part_changed_at >= found_at AS part_changed \
         FROM learning_lesson_teaching_findings WHERE lesson_id=? \
         ORDER BY CASE site WHEN 'lesson' THEN 0 WHEN 'section' THEN 1 ELSE 2 END, unit_index, kind",
    )
    .bind(lesson_id)
    .fetch_all(pool)
    .await
    .map_err(db)?;
    Ok(rows
        .iter()
        .map(|row| StoredFinding {
            finding_key: row.get("finding_key"),
            kind: row.get("kind"),
            site: row.get("site"),
            unit_index: row.get("unit_index"),
            unit_title: row.get("unit_title"),
            note: row.get("note"),
            summary: row.get("summary"),
            passage: row.get("passage"),
            sentence: row.get("sentence"),
            repairs: row.get("repairs"),
            state: row.get("state"),
            resolution: row.get("resolution"),
            resolved_at: row.get("resolved_at"),
            part_changed: row.get::<i64, _>("part_changed") != 0,
        })
        .collect())
}

/// The state change a settle writes.
pub(in crate::lessons::claim_reviews) struct Settle<'a> {
    pub from: &'a [&'a str],
    pub to: &'a str,
    pub kept_at: Option<i64>,
    pub resolution: Option<&'a str>,
    pub resolved_at: Option<i64>,
}

/// Moves the findings in `keys` that are in a `from` state to `to`, when the
/// lesson is in the program; returns how many it moved.
pub(in crate::lessons::claim_reviews) async fn settle(
    connection: &mut SqliteConnection,
    program_id: &str,
    lesson_id: &str,
    keys: &BTreeSet<&str>,
    change: &Settle<'_>,
) -> Result<u64> {
    let changed = sqlx::query(
        "UPDATE learning_lesson_teaching_findings SET state=?, kept_at=?, resolution=?, resolved_at=? \
         WHERE lesson_id=? AND state IN (SELECT value FROM json_each(?)) AND finding_key IN (SELECT value FROM json_each(?)) \
         AND EXISTS (SELECT 1 FROM learning_lessons WHERE id=? AND program_id=?)",
    )
    .bind(change.to)
    .bind(change.kept_at)
    .bind(change.resolution)
    .bind(change.resolved_at)
    .bind(lesson_id)
    .bind(serde_json::to_string(change.from).map_err(AppError::from)?)
    .bind(serde_json::to_string(keys).map_err(AppError::from)?)
    .bind(lesson_id)
    .bind(program_id)
    .execute(&mut *connection)
    .await
    .map_err(db)?;
    Ok(changed.rows_affected())
}

/// Marks the findings about a part as written before the user changed it.
pub(in crate::lessons::claim_reviews) async fn mark_part_changed(
    connection: &mut SqliteConnection,
    lesson_id: &str,
    site: &str,
    unit_index: usize,
    at: i64,
) -> Result<()> {
    sqlx::query(
        "UPDATE learning_lesson_teaching_findings SET part_changed_at=? \
         WHERE lesson_id=? AND site=? AND unit_index=?",
    )
    .bind(at)
    .bind(lesson_id)
    .bind(site)
    .bind(i64::try_from(unit_index).unwrap_or(i64::MAX))
    .execute(&mut *connection)
    .await
    .map_err(db)?;
    Ok(())
}
