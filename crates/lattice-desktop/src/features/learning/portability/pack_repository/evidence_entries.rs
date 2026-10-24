//! Learner evidence, one pack entry per table, so no single entry has to
//! hold a learner's whole history. Decoding rebuilds the in-memory snapshot
//! the table importer understands.
use super::{
    conflicts::IdClaim,
    entries::{decode, ExportScope, ImportScope, SectionRows},
    evidence_tables::{
        insert_evidence_snapshot, snapshot_evidence_tables, AggregateTableSnapshot,
        LearningEvidenceSnapshot, EVIDENCE_TABLES, MAX_TABLE_ROWS,
    },
    invalid, json, parse,
    remap::map_evidence_ids,
    validation::validate_aggregate_id_namespaces,
};
use crate::features::learning::{
    dto::LearningProgramDto,
    pack::{
        sections::{PackSection, EVIDENCE_TABLE_PREFIX, OUTCOMES_ENTRY},
        DecodedLearningPack, LearningPackEntry, LearningPackEntryKind,
    },
    portability_dto::LearningPackConflictKind,
};
use async_trait::async_trait;
use lattice_core::shared::error::Result;
use sqlx::SqliteConnection;
use std::collections::{BTreeMap, HashMap};

fn table_path(table: &str) -> String {
    format!("{EVIDENCE_TABLE_PREFIX}{table}.json")
}

/// One table per entry, in `EVIDENCE_TABLES` order, each within the row cap.
fn encode_evidence(snapshot: &LearningEvidenceSnapshot) -> Result<Vec<LearningPackEntry>> {
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
fn decode_evidence(pack: &DecodedLearningPack) -> Result<Option<LearningEvidenceSnapshot>> {
    let entries: BTreeMap<&str, &Vec<u8>> = pack
        .entries
        .iter()
        .filter(|(path, _)| path.starts_with(EVIDENCE_TABLE_PREFIX))
        .map(|(path, bytes)| (path.as_str(), bytes))
        .collect();
    // Evidence tables travel with the rest of the learner's evidence: the
    // attempt, event and follow-up entries written by the same export option.
    let declared = pack.manifest.privacy.includes_learner_evidence
        && pack
            .manifest
            .entries
            .iter()
            .any(|entry| matches!(entry.kind, LearningPackEntryKind::Evidence));
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

/// Whether the pack carries learner evidence tables. A pack that does
/// carries all of them, which `decode_evidence` enforces.
pub(super) fn carries_evidence_tables(pack: &DecodedLearningPack) -> bool {
    pack.entries
        .keys()
        .any(|path| path.starts_with(EVIDENCE_TABLE_PREFIX))
}

/// Tables with no `program_id` of their own, owned through the course's
/// memory deck.
const DECK_OWNERS: &[(&str, &str)] = &[
    ("study_decks", "SELECT COALESCE(m.program_id,'') FROM study_decks d LEFT JOIN learning_memory m ON m.deck_id=d.id WHERE d.id=?"),
    ("study_cards", "SELECT COALESCE(m.program_id,'') FROM study_cards c JOIN study_decks d ON d.id=c.deck_id LEFT JOIN learning_memory m ON m.deck_id=d.id WHERE c.id=?"),
    ("study_reviews", "SELECT COALESCE(m.program_id,'') FROM study_reviews r JOIN study_cards c ON c.id=r.card_id JOIN study_decks d ON d.id=c.deck_id LEFT JOIN learning_memory m ON m.deck_id=d.id WHERE r.id=?"),
];

pub(super) struct EvidenceTableRows;
pub(super) const EVIDENCE_TABLE_ROWS: EvidenceTableRows = EvidenceTableRows;

#[async_trait]
impl SectionRows for EvidenceTableRows {
    fn section(&self) -> PackSection {
        PackSection::EvidenceTables
    }
    async fn encode(
        &self,
        connection: &mut SqliteConnection,
        scope: &ExportScope<'_>,
    ) -> Result<Vec<LearningPackEntry>> {
        if !scope.options.include_evidence {
            return Ok(Vec::new());
        }
        encode_evidence(&snapshot_evidence_tables(connection, &scope.program.summary.id).await?)
    }
    fn claims(
        &self,
        pack: &DecodedLearningPack,
        _program: &LearningProgramDto,
    ) -> Result<Vec<IdClaim>> {
        let Some(snapshot) = decode_evidence(pack)? else {
            return Ok(Vec::new());
        };
        let mut claims = Vec::new();
        for (table, spec) in snapshot.tables.iter().zip(EVIDENCE_TABLES) {
            let owned = table.columns.iter().any(|column| column == "program_id");
            for row in &table.rows {
                let text = |column: &str| row.get(column).and_then(serde_json::Value::as_str);
                if owned {
                    for column in ["id", "operation_id"] {
                        if let Some(id) = text(column) {
                            claims.push(IdClaim::column(spec.table, column, id, None));
                        }
                    }
                }
                if spec.table == "learning_operations" {
                    if let Some(id) = text("operation_id") {
                        claims.push(IdClaim::owned_by(
                            LearningPackConflictKind::Record,
                            spec.table,
                            "SELECT COALESCE(program_id,'') FROM learning_operations WHERE operation_id=?",
                            id,
                        ));
                    }
                } else if !owned {
                    let owner = DECK_OWNERS.iter().find(|(name, _)| *name == spec.table);
                    if let (Some((_, owner_sql)), Some(id)) = (owner, text("id")) {
                        claims.push(IdClaim::owned_by(
                            LearningPackConflictKind::Record,
                            spec.table,
                            owner_sql,
                            id,
                        ));
                    }
                }
            }
        }
        Ok(claims)
    }
    fn map_ids(
        &self,
        pack: &DecodedLearningPack,
        program: &LearningProgramDto,
        ids: &mut HashMap<String, String>,
    ) -> Result<()> {
        let Some(snapshot) = decode_evidence(pack)? else {
            return Ok(());
        };
        let outcomes: Vec<serde_json::Value> = decode(pack, OUTCOMES_ENTRY)?.unwrap_or_default();
        validate_aggregate_id_namespaces(&snapshot, program, &outcomes)?;
        map_evidence_ids(&snapshot, ids)
    }
    async fn insert(
        &self,
        connection: &mut SqliteConnection,
        scope: &ImportScope<'_>,
    ) -> Result<()> {
        let Some(snapshot) = decode_evidence(scope.pack)? else {
            return Ok(());
        };
        insert_evidence_snapshot(connection, &scope.program.summary.id, &snapshot, scope.ids).await
    }
}
