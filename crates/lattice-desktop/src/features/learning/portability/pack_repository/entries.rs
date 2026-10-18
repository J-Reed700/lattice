//! What the repository does with each pack section: one behaviour row per
//! section in `pack::sections::SECTIONS`, keyed by the same `PackSection`.
//! Export, the preview's conflict scan, a copy's fresh identifiers and the
//! import all walk `SECTION_ROWS`, so a new kind of entry is one descriptor
//! row and one behaviour row, with no edit to the codec, the exporter or
//! the orchestrator. Rows run in this table's order, which is the import's
//! insert order.
use super::{
    canvas_entries::CANVASES,
    conflicts::IdClaim,
    evidence_entries::EVIDENCE_TABLE_ROWS,
    export, importers, parse,
    program_entry::{ATTEMPTS, PROGRAM},
    provenance,
    source_entries::{SourceBodies, SourceHistory, SourceLibrary, SourceSelectors, SourceTrust},
    validation::validate_answer_keys,
    value_str,
};
use crate::features::learning::{
    dto::LearningProgramDto,
    pack::{
        sections::{
            PackSection, EVIDENCE_ENTRY, FOLLOW_UP_ENTRY, KEYS_ENTRY, OUTCOMES_ENTRY,
            PRACTICAL_ENTRY,
        },
        DecodedLearningPack, LearningPackEntry,
    },
    portability_dto::{LearningPackConflictPolicy, LearningPackExportOptionsDto},
    practical_dto::{
        LearningPracticalActivityDto, LearningPracticalRunDto, LearningPracticalRuntimeKind,
    },
};
use async_trait::async_trait;
use lattice_core::shared::error::{AppError, Result};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use sqlx::SqliteConnection;
use std::collections::HashMap;

/// What one export reads: the whole course and the user's choices.
pub(super) struct ExportScope<'a> {
    pub(super) program: &'a LearningProgramDto,
    pub(super) options: &'a LearningPackExportOptionsDto,
}

/// What one import writes from.
pub(super) struct ImportScope<'a> {
    pub(super) pack: &'a DecodedLearningPack,
    /// The program as the pack carries it, for sections validated against
    /// its identifiers.
    pub(super) original: &'a LearningProgramDto,
    /// The program as it is written here, its identifiers remapped for a copy.
    pub(super) program: &'a LearningProgramDto,
    pub(super) ids: &'a HashMap<String, String>,
    pub(super) policy: LearningPackConflictPolicy,
}

/// One section's behaviour. Every method has a no-op default, so a row
/// states only what its section does.
#[async_trait]
pub(super) trait SectionRows: Sync {
    fn section(&self) -> PackSection;
    /// The section's entries, read inside the export's snapshot transaction.
    async fn encode(
        &self,
        _connection: &mut SqliteConnection,
        _scope: &ExportScope<'_>,
    ) -> Result<Vec<LearningPackEntry>> {
        Ok(Vec::new())
    }
    /// Refuse a malformed section before anything is written; answer with
    /// what the import cannot restore.
    fn check(
        &self,
        _pack: &DecodedLearningPack,
        _program: &LearningProgramDto,
    ) -> Result<Vec<String>> {
        Ok(Vec::new())
    }
    /// The identifiers the section would write. Reading them also parses
    /// the section, so a preview and an apply both refuse a malformed one.
    fn claims(
        &self,
        _pack: &DecodedLearningPack,
        _program: &LearningProgramDto,
    ) -> Result<Vec<IdClaim>> {
        Ok(Vec::new())
    }
    /// A fresh identifier for everything the section writes, for a copy.
    fn map_ids(
        &self,
        _pack: &DecodedLearningPack,
        _program: &LearningProgramDto,
        _ids: &mut HashMap<String, String>,
    ) -> Result<()> {
        Ok(())
    }
    /// Write the section's rows in the importing transaction.
    async fn insert(
        &self,
        _connection: &mut SqliteConnection,
        _scope: &ImportScope<'_>,
    ) -> Result<()> {
        Ok(())
    }
}

/// Every section's behaviour, in insert order.
pub(super) const SECTION_ROWS: &[&dyn SectionRows] = &[
    &PROGRAM,
    &ATTEMPTS,
    &LessonStates,
    &Outcomes,
    &Events,
    &FollowUps,
    &Practical,
    &CANVASES,
    &SourceLibrary,
    &SourceHistory,
    &SourceSelectors,
    &SourceTrust,
    &EVIDENCE_TABLE_ROWS,
    &AnswerKeys,
    &Provenance,
    &SourceBodies,
];

/// What the preview needs from a pack that passed every section's checks.
pub(super) struct CheckedPack {
    pub(super) warnings: Vec<String>,
    pub(super) claims: Vec<IdClaim>,
}

/// Run every section's checks and read its claims.
pub(super) fn check(
    pack: &DecodedLearningPack,
    program: &LearningProgramDto,
) -> Result<CheckedPack> {
    let mut checked = CheckedPack {
        warnings: Vec::new(),
        claims: Vec::new(),
    };
    for rows in SECTION_ROWS {
        checked.warnings.extend(rows.check(pack, program)?);
        checked.claims.extend(rows.claims(pack, program)?);
    }
    Ok(checked)
}

/// A fresh identifier for every record a copy writes.
pub(super) fn map_ids(
    pack: &DecodedLearningPack,
    program: &LearningProgramDto,
    ids: &mut HashMap<String, String>,
) -> Result<()> {
    for rows in SECTION_ROWS {
        rows.map_ids(pack, program, ids)?;
    }
    Ok(())
}

pub(super) fn fresh(ids: &mut HashMap<String, String>, from: &str) {
    super::remap::map_id(ids, from, uuid::Uuid::new_v4().to_string());
}

/// `None` when the pack does not carry the entry.
pub(super) fn decode<T: DeserializeOwned>(
    pack: &DecodedLearningPack,
    path: &str,
) -> Result<Option<T>> {
    pack.entries.get(path).map(|bytes| parse(bytes)).transpose()
}

/// The `key` of every row in a JSON array entry.
pub(super) fn row_ids<'a>(rows: &'a [serde_json::Value], key: &str) -> Result<Vec<&'a str>> {
    rows.iter().map(|row| value_str(row, key)).collect()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct PrivateAnswerKey {
    pub(super) question_id: String,
    pub(super) correct_index: usize,
    pub(super) explanation: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct PracticalPackSnapshot {
    pub(super) activities: Vec<LearningPracticalActivityDto>,
    pub(super) runs: Vec<LearningPracticalRunDto>,
    pub(super) simulations: Vec<serde_json::Value>,
    pub(super) simulation_turns: Vec<serde_json::Value>,
    pub(super) run_snapshots: Vec<serde_json::Value>,
}

struct LessonStates;
#[async_trait]
impl SectionRows for LessonStates {
    fn section(&self) -> PackSection {
        PackSection::LessonStates
    }
    async fn encode(
        &self,
        connection: &mut SqliteConnection,
        scope: &ExportScope<'_>,
    ) -> Result<Vec<LearningPackEntry>> {
        export::lesson_states(connection, &scope.program.summary.id).await
    }
    async fn insert(
        &self,
        connection: &mut SqliteConnection,
        scope: &ImportScope<'_>,
    ) -> Result<()> {
        importers::insert_lesson_states(
            connection,
            &scope.program.summary.id,
            scope.pack,
            scope.ids,
        )
        .await
    }
}

/// A JSON array entry of program-scoped rows whose `id` is their key in
/// `table`: outcomes, evidence events and follow-ups.
pub(super) fn table_claims(
    pack: &DecodedLearningPack,
    path: &str,
    table: &'static str,
) -> Result<Vec<IdClaim>> {
    let rows: Vec<serde_json::Value> = decode(pack, path)?.unwrap_or_default();
    Ok(row_ids(&rows, "id")?
        .into_iter()
        .map(|id| IdClaim::column(table, "id", id, None))
        .collect())
}

pub(super) fn map_table_ids(
    pack: &DecodedLearningPack,
    path: &str,
    ids: &mut HashMap<String, String>,
) -> Result<()> {
    let rows: Vec<serde_json::Value> = decode(pack, path)?.unwrap_or_default();
    for row in &rows {
        let id = row
            .get("id")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| AppError::InvalidInput(format!("Pack record in {path} has no ID.")))?;
        fresh(ids, id);
    }
    Ok(())
}

struct Outcomes;
#[async_trait]
impl SectionRows for Outcomes {
    fn section(&self) -> PackSection {
        PackSection::Outcomes
    }
    async fn encode(
        &self,
        connection: &mut SqliteConnection,
        scope: &ExportScope<'_>,
    ) -> Result<Vec<LearningPackEntry>> {
        export::outcomes(connection, &scope.program.summary.id).await
    }
    fn claims(
        &self,
        pack: &DecodedLearningPack,
        _program: &LearningProgramDto,
    ) -> Result<Vec<IdClaim>> {
        table_claims(pack, OUTCOMES_ENTRY, "learning_outcome_definitions")
    }
    fn map_ids(
        &self,
        pack: &DecodedLearningPack,
        _program: &LearningProgramDto,
        ids: &mut HashMap<String, String>,
    ) -> Result<()> {
        map_table_ids(pack, OUTCOMES_ENTRY, ids)
    }
    async fn insert(
        &self,
        connection: &mut SqliteConnection,
        scope: &ImportScope<'_>,
    ) -> Result<()> {
        importers::insert_outcomes(connection, &scope.program.summary.id, scope.pack, scope.ids)
            .await
    }
}

struct Events;
#[async_trait]
impl SectionRows for Events {
    fn section(&self) -> PackSection {
        PackSection::Events
    }
    async fn encode(
        &self,
        connection: &mut SqliteConnection,
        scope: &ExportScope<'_>,
    ) -> Result<Vec<LearningPackEntry>> {
        if !scope.options.include_evidence {
            return Ok(Vec::new());
        }
        export::events(connection, &scope.program.summary.id).await
    }
    fn claims(
        &self,
        pack: &DecodedLearningPack,
        _program: &LearningProgramDto,
    ) -> Result<Vec<IdClaim>> {
        table_claims(pack, EVIDENCE_ENTRY, "learning_evidence_events")
    }
    fn map_ids(
        &self,
        pack: &DecodedLearningPack,
        _program: &LearningProgramDto,
        ids: &mut HashMap<String, String>,
    ) -> Result<()> {
        map_table_ids(pack, EVIDENCE_ENTRY, ids)
    }
    async fn insert(
        &self,
        connection: &mut SqliteConnection,
        scope: &ImportScope<'_>,
    ) -> Result<()> {
        if !scope.pack.manifest.privacy.includes_learner_evidence {
            return Ok(());
        }
        importers::insert_events(connection, &scope.program.summary.id, scope.pack, scope.ids).await
    }
}

struct FollowUps;
#[async_trait]
impl SectionRows for FollowUps {
    fn section(&self) -> PackSection {
        PackSection::FollowUps
    }
    async fn encode(
        &self,
        connection: &mut SqliteConnection,
        scope: &ExportScope<'_>,
    ) -> Result<Vec<LearningPackEntry>> {
        if !scope.options.include_evidence {
            return Ok(Vec::new());
        }
        export::follow_ups(connection, &scope.program.summary.id).await
    }
    fn claims(
        &self,
        pack: &DecodedLearningPack,
        _program: &LearningProgramDto,
    ) -> Result<Vec<IdClaim>> {
        table_claims(pack, FOLLOW_UP_ENTRY, "learning_follow_up_recommendations")
    }
    fn map_ids(
        &self,
        pack: &DecodedLearningPack,
        _program: &LearningProgramDto,
        ids: &mut HashMap<String, String>,
    ) -> Result<()> {
        map_table_ids(pack, FOLLOW_UP_ENTRY, ids)
    }
    async fn insert(
        &self,
        connection: &mut SqliteConnection,
        scope: &ImportScope<'_>,
    ) -> Result<()> {
        if !scope.pack.manifest.privacy.includes_learner_evidence {
            return Ok(());
        }
        importers::insert_follow_ups(connection, &scope.program.summary.id, scope.pack, scope.ids)
            .await
    }
}

struct Practical;
#[async_trait]
impl SectionRows for Practical {
    fn section(&self) -> PackSection {
        PackSection::Practical
    }
    async fn encode(
        &self,
        connection: &mut SqliteConnection,
        scope: &ExportScope<'_>,
    ) -> Result<Vec<LearningPackEntry>> {
        if !scope.options.include_practical_artifacts {
            return Ok(Vec::new());
        }
        export::practical(connection, &scope.program.summary.id).await
    }
    fn check(
        &self,
        pack: &DecodedLearningPack,
        _program: &LearningProgramDto,
    ) -> Result<Vec<String>> {
        let Some(snapshot) = decode::<PracticalPackSnapshot>(pack, PRACTICAL_ENTRY)? else {
            return Ok(Vec::new());
        };
        let executable = snapshot
            .activities
            .iter()
            .any(|activity| activity.runtime_kind != LearningPracticalRuntimeKind::None);
        Ok(if executable {
            vec!["Imported executable activities are restored in review-only mode without hidden evaluator files; regenerate and review the exercise before running it.".into()]
        } else {
            Vec::new()
        })
    }
    fn claims(
        &self,
        pack: &DecodedLearningPack,
        _program: &LearningProgramDto,
    ) -> Result<Vec<IdClaim>> {
        let Some(snapshot) = decode::<PracticalPackSnapshot>(pack, PRACTICAL_ENTRY)? else {
            return Ok(Vec::new());
        };
        let mut claims = Vec::new();
        for activity in &snapshot.activities {
            claims.push(IdClaim::column(
                "learning_practical_activities",
                "id",
                &activity.id,
                Some(&activity.title),
            ));
        }
        for run in &snapshot.runs {
            claims.push(IdClaim::column(
                "learning_practical_runs",
                "id",
                &run.id,
                None,
            ));
        }
        for (table, rows) in [
            ("learning_simulation_sessions", &snapshot.simulations),
            ("learning_simulation_turns", &snapshot.simulation_turns),
        ] {
            for id in row_ids(rows, "id")? {
                claims.push(IdClaim::column(table, "id", id, None));
            }
        }
        for id in row_ids(&snapshot.run_snapshots, "operationId")? {
            claims.push(IdClaim::column(
                "learning_practical_runs",
                "operation_id",
                id,
                None,
            ));
        }
        Ok(claims)
    }
    fn map_ids(
        &self,
        pack: &DecodedLearningPack,
        _program: &LearningProgramDto,
        ids: &mut HashMap<String, String>,
    ) -> Result<()> {
        let Some(snapshot) = decode::<PracticalPackSnapshot>(pack, PRACTICAL_ENTRY)? else {
            return Ok(());
        };
        for activity in &snapshot.activities {
            fresh(ids, &activity.id);
        }
        for run in &snapshot.runs {
            fresh(ids, &run.id);
        }
        for id in row_ids(&snapshot.run_snapshots, "operationId")? {
            fresh(ids, id);
        }
        for rows in [&snapshot.simulations, &snapshot.simulation_turns] {
            for row in rows {
                fresh(ids, value_str(row, "id")?);
                fresh(ids, value_str(row, "operationId")?);
            }
        }
        Ok(())
    }
    async fn insert(
        &self,
        connection: &mut SqliteConnection,
        scope: &ImportScope<'_>,
    ) -> Result<()> {
        let Some(mut snapshot) = decode::<PracticalPackSnapshot>(scope.pack, PRACTICAL_ENTRY)?
        else {
            return Ok(());
        };
        let program_id = &scope.program.summary.id;
        importers::remap_practical(
            &mut snapshot,
            program_id,
            scope.ids,
            super::evidence_entries::carries_evidence_tables(scope.pack),
            scope.policy.remaps_ids(),
        )?;
        importers::insert_practical_snapshot(connection, program_id, &snapshot).await
    }
}

struct AnswerKeys;
impl AnswerKeys {
    fn keys(pack: &DecodedLearningPack) -> Result<Vec<PrivateAnswerKey>> {
        decode(pack, KEYS_ENTRY)?
            .ok_or_else(|| AppError::InvalidInput("Pack is missing assessment answer data.".into()))
    }
}
#[async_trait]
impl SectionRows for AnswerKeys {
    fn section(&self) -> PackSection {
        PackSection::AnswerKeys
    }
    async fn encode(
        &self,
        connection: &mut SqliteConnection,
        scope: &ExportScope<'_>,
    ) -> Result<Vec<LearningPackEntry>> {
        export::answer_keys(connection, &scope.program.summary.id).await
    }
    fn check(
        &self,
        pack: &DecodedLearningPack,
        program: &LearningProgramDto,
    ) -> Result<Vec<String>> {
        validate_answer_keys(program, &Self::keys(pack)?)?;
        Ok(Vec::new())
    }
    async fn insert(
        &self,
        connection: &mut SqliteConnection,
        scope: &ImportScope<'_>,
    ) -> Result<()> {
        importers::insert_answer_keys(connection, Self::keys(scope.pack)?, scope.ids).await
    }
}

struct Provenance;
#[async_trait]
impl SectionRows for Provenance {
    fn section(&self) -> PackSection {
        PackSection::Provenance
    }
    async fn encode(
        &self,
        connection: &mut SqliteConnection,
        scope: &ExportScope<'_>,
    ) -> Result<Vec<LearningPackEntry>> {
        let snapshot = provenance::snapshot(connection, scope.program).await?;
        Ok(vec![export::entry(
            provenance::ENTRY,
            crate::features::learning::pack::LearningPackEntryKind::Curriculum,
            &snapshot,
        )?])
    }
    fn check(
        &self,
        pack: &DecodedLearningPack,
        program: &LearningProgramDto,
    ) -> Result<Vec<String>> {
        provenance::decode(pack, program)?;
        Ok(Vec::new())
    }
    async fn insert(
        &self,
        connection: &mut SqliteConnection,
        scope: &ImportScope<'_>,
    ) -> Result<()> {
        // Validated against the identifiers the pack carries, then remapped.
        let snapshot = provenance::decode(scope.pack, scope.original)?;
        provenance::restore(connection, &scope.program.summary.id, snapshot, scope.ids).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::learning::pack::sections::SECTIONS;

    #[test]
    fn every_descriptor_has_exactly_one_behaviour_row() {
        for descriptor in SECTIONS {
            let rows = SECTION_ROWS
                .iter()
                .filter(|rows| rows.section() == descriptor.section)
                .count();
            assert_eq!(rows, 1, "{:?}", descriptor.section);
        }
        assert_eq!(SECTION_ROWS.len(), SECTIONS.len());
    }
}
