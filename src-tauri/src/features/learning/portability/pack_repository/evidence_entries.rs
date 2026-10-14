//! Learner evidence and canvases, one pack entry each, so no single entry has
//! to hold a learner's whole history. Decoding rebuilds the in-memory
//! snapshots the importer already understands; the per-table import is
//! unchanged.
use super::{
    invalid, json, parse, value_str, AggregateTableSnapshot, CanvasPackSnapshot,
    LearningEvidenceSnapshot, EVIDENCE_TABLES,
};
use crate::features::learning::pack::{
    DecodedLearningPack, LearningPackEntry, LearningPackEntryKind, CANVAS_PREFIX,
    EVIDENCE_TABLE_PREFIX,
};
use crate::shared::error::Result;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// The most rows one evidence table may carry. Import refuses more, so
/// export does too: a pack no computer can import is no backup.
pub(super) const MAX_TABLE_ROWS: usize = 50_000;

fn table_path(table: &str) -> String {
    format!("{EVIDENCE_TABLE_PREFIX}{table}.json")
}

fn canvas_path(canvas_id: &str) -> Result<String> {
    // The ID becomes a file name: keep it to the characters our IDs use.
    if canvas_id.is_empty()
        || canvas_id.len() > 200
        || !canvas_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
    {
        return Err(invalid("A canvas ID cannot be used as a pack entry name."));
    }
    Ok(format!("{CANVAS_PREFIX}{canvas_id}.json"))
}

/// One table per entry, in `EVIDENCE_TABLES` order, each within the row cap.
pub(super) fn encode_evidence(
    snapshot: &LearningEvidenceSnapshot,
) -> Result<Vec<LearningPackEntry>> {
    if snapshot.tables.len() != EVIDENCE_TABLES.len()
        || snapshot
            .tables
            .iter()
            .zip(EVIDENCE_TABLES)
            .any(|(table, spec)| table.table != spec.table)
    {
        return Err(invalid("Pack aggregate table set is incomplete."));
    }
    if let Some(table) = snapshot
        .tables
        .iter()
        .find(|table| table.rows.len() > MAX_TABLE_ROWS)
    {
        return Err(invalid(format!(
            "This course has {} records in {}, more than the {MAX_TABLE_ROWS} per table a course pack can carry, so no computer could import it.",
            table.rows.len(),
            table.table
        )));
    }
    snapshot
        .tables
        .iter()
        .map(|table| {
            Ok(LearningPackEntry {
                path: table_path(&table.table),
                kind: LearningPackEntryKind::Evidence,
                bytes: json(table)?,
            })
        })
        .collect()
}

/// `None` when the pack carries no learner evidence. When it does, every
/// table must be present exactly once and nothing else may sit beside them.
pub(super) fn decode_evidence(
    pack: &DecodedLearningPack,
) -> Result<Option<LearningEvidenceSnapshot>> {
    let entries: BTreeMap<&str, &Vec<u8>> = pack
        .entries
        .iter()
        .filter(|(path, _)| path.starts_with(EVIDENCE_TABLE_PREFIX))
        .map(|(path, bytes)| (path.as_str(), bytes))
        .collect();
    // Evidence tables travel with the rest of the learner's evidence: the
    // attempt, event and follow-up entries written by the same export option.
    let declared = pack.manifest.privacy.includes_learner_evidence
        && pack.manifest.entries.iter().any(|entry| {
            matches!(
                entry.kind,
                LearningPackEntryKind::Evidence | LearningPackEntryKind::Attempt
            )
        });
    if !declared && entries.is_empty() {
        return Ok(None);
    }
    let known: Vec<String> = EVIDENCE_TABLES
        .iter()
        .map(|spec| table_path(spec.table))
        .collect();
    if let Some(unknown) = entries
        .keys()
        .find(|path| !known.iter().any(|k| k == *path))
    {
        return Err(invalid(format!(
            "Pack learner evidence has an unexpected table entry: {unknown}."
        )));
    }
    let mut tables = Vec::with_capacity(EVIDENCE_TABLES.len());
    for (spec, path) in EVIDENCE_TABLES.iter().zip(&known) {
        let bytes = entries.get(path.as_str()).ok_or_else(|| {
            invalid(format!(
                "Pack learner evidence is incomplete: missing {}.",
                spec.table
            ))
        })?;
        let table: AggregateTableSnapshot = parse(bytes)?;
        if table.table != spec.table {
            return Err(invalid(
                "Pack evidence entry names a different table than its path.",
            ));
        }
        tables.push(table);
    }
    Ok(Some(LearningEvidenceSnapshot { tables }))
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CanvasEntry {
    canvas: serde_json::Value,
    snapshots: Vec<serde_json::Value>,
}

/// One entry per canvas holding the canvas row and its snapshots.
pub(super) fn encode_canvases(snapshot: &CanvasPackSnapshot) -> Result<Vec<LearningPackEntry>> {
    let mut grouped: BTreeMap<&str, CanvasEntry> = BTreeMap::new();
    for canvas in &snapshot.canvases {
        let id = value_str(canvas, "id")?;
        let entry = CanvasEntry {
            canvas: canvas.clone(),
            snapshots: Vec::new(),
        };
        if grouped.insert(id, entry).is_some() {
            return Err(invalid("Pack canvases repeat an ID."));
        }
    }
    for row in &snapshot.snapshots {
        grouped
            .get_mut(value_str(row, "canvasId")?)
            .ok_or_else(|| invalid("A canvas snapshot has no canvas in this program."))?
            .snapshots
            .push(row.clone());
    }
    grouped
        .into_iter()
        .map(|(id, entry)| {
            Ok(LearningPackEntry {
                path: canvas_path(id)?,
                kind: LearningPackEntryKind::Canvas,
                bytes: json(&entry)?,
            })
        })
        .collect()
}

/// `None` when the pack has no canvases.
pub(super) fn decode_canvases(pack: &DecodedLearningPack) -> Result<Option<CanvasPackSnapshot>> {
    let mut snapshot = CanvasPackSnapshot {
        canvases: Vec::new(),
        snapshots: Vec::new(),
    };
    for (path, bytes) in &pack.entries {
        if !path.starts_with(CANVAS_PREFIX) {
            continue;
        }
        let entry: CanvasEntry = parse(bytes)?;
        let id = value_str(&entry.canvas, "id")?;
        if canvas_path(id)? != *path {
            return Err(invalid(
                "Pack canvas entry names a different canvas than its path.",
            ));
        }
        for row in &entry.snapshots {
            if value_str(row, "canvasId")? != id {
                return Err(invalid("Pack canvas snapshot belongs to another canvas."));
            }
        }
        snapshot.canvases.push(entry.canvas);
        snapshot.snapshots.extend(entry.snapshots);
    }
    Ok((!snapshot.canvases.is_empty()).then_some(snapshot))
}
