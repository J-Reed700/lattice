//! Managed file and database lifecycle for portable Learning Studio packs.
mod export;
use crate::features::learning::{
    dto::{LearningPreparation, LearningProgramDto, LearningProgramStatus},
    pack::{decode_learning_pack, encode_learning_pack, DecodedLearningPack, LearningPackManifest},
    portability_dto::*,
    source_library::LearningSourceLibraryRepository,
};
use crate::shared::error::{AppError, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::{Column, Row, SqlitePool, TypeInfo};
use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
};

const MAX_PACK_BYTES: u64 = 64 * 1024 * 1024;
const PROGRAM_ENTRY: &str = "program/program.json";
const KEYS_ENTRY: &str = "program/answer-keys.json";
const PRACTICAL_ENTRY: &str = "practical/artifacts.json";
const OUTCOMES_ENTRY: &str = "program/outcomes.json";
const EVIDENCE_ENTRY: &str = "evidence/events.json";
const FOLLOW_UP_ENTRY: &str = "evidence/follow-ups.json";
const CANVAS_ENTRY: &str = "program/canvases.json";
const LESSON_STATE_ENTRY: &str = "curriculum/lesson-state.json";

fn db(e: sqlx::Error) -> AppError {
    AppError::Database(e.to_string())
}
fn invalid(s: impl Into<String>) -> AppError {
    AppError::InvalidInput(s.into())
}
fn io_error(error: impl std::fmt::Display) -> AppError {
    AppError::Io {
        message: error.to_string(),
        kind: "learning_pack".into(),
    }
}
fn json<T: Serialize>(v: &T) -> Result<Vec<u8>> {
    serde_json::to_vec(v).map_err(|e| AppError::Serialization(e.to_string()))
}
fn json_string<T: Serialize>(v: &T) -> Result<String> {
    serde_json::to_string(v).map_err(|e| AppError::Serialization(e.to_string()))
}
fn parse<T: for<'de> Deserialize<'de>>(v: &[u8]) -> Result<T> {
    serde_json::from_slice(v).map_err(|e| AppError::InvalidData(e.to_string()))
}
async fn snapshot_evidence_tables(
    connection: &mut sqlx::SqliteConnection,
    program_id: &str,
) -> Result<LearningEvidenceSnapshot> {
    let mut tables = Vec::with_capacity(EVIDENCE_TABLES.len());
    for spec in EVIDENCE_TABLES {
        let pragma = format!("PRAGMA table_info({})", spec.table);
        let cols = sqlx::query(&pragma)
            .fetch_all(&mut *connection)
            .await
            .map_err(db)?;
        let columns: Vec<String> = cols
            .into_iter()
            .map(|row| row.get::<String, _>("name"))
            .collect();
        if columns.is_empty() {
            return Err(AppError::Database(format!(
                "Missing expected pack table {}",
                spec.table
            )));
        }
        let query = format!("SELECT * FROM {} WHERE {}", spec.table, spec.filter);
        let rows = sqlx::query(&query)
            .bind(program_id)
            .fetch_all(&mut *connection)
            .await
            .map_err(db)?;
        let mut values = Vec::with_capacity(rows.len());
        for row in rows {
            let mut object = serde_json::Map::new();
            for (index, column) in row.columns().iter().enumerate() {
                let type_name = column.type_info().name().to_ascii_uppercase();
                let value = if type_name.contains("INT") || type_name == "BOOL" {
                    row.try_get::<Option<i64>, _>(index)
                        .map_err(db)?
                        .map(serde_json::Value::from)
                        .unwrap_or(serde_json::Value::Null)
                } else if type_name.contains("REAL")
                    || type_name.contains("FLOA")
                    || type_name.contains("DOUB")
                {
                    row.try_get::<Option<f64>, _>(index)
                        .map_err(db)?
                        .and_then(serde_json::Number::from_f64)
                        .map(serde_json::Value::Number)
                        .unwrap_or(serde_json::Value::Null)
                } else if type_name.contains("BLOB") {
                    return Err(invalid(format!(
                        "Pack snapshot does not support binary column {}.{}",
                        spec.table,
                        column.name()
                    )));
                } else {
                    row.try_get::<Option<String>, _>(index)
                        .map_err(db)?
                        .map(serde_json::Value::String)
                        .unwrap_or(serde_json::Value::Null)
                };
                object.insert(column.name().to_owned(), value);
            }
            values.push(serde_json::Value::Object(object));
        }
        tables.push(AggregateTableSnapshot {
            table: spec.table.into(),
            columns,
            rows: values,
        });
    }
    Ok(LearningEvidenceSnapshot { tables })
}
fn map_evidence_ids(
    snapshot: &LearningEvidenceSnapshot,
    map: &mut HashMap<String, String>,
) -> Result<()> {
    let mut namespaces = HashMap::<String, String>::new();
    for table in &snapshot.tables {
        for row in &table.rows {
            let object = row
                .as_object()
                .ok_or_else(|| invalid("Pack aggregate contains a malformed row."))?;
            for key in ["id", "operation_id"] {
                if let Some(old) = object.get(key).and_then(serde_json::Value::as_str) {
                    let namespace = format!("{}:{key}", table.table);
                    if let Some(previous) = namespaces.get(old) {
                        if previous != &namespace {
                            return Err(invalid(
                                "Pack aggregate reuses an identifier across entity types.",
                            ));
                        }
                        continue;
                    }
                    namespaces.insert(old.to_owned(), namespace);
                    // Cross-snapshot aliases are checked before this mapper
                    // runs; preserve their already assigned canonical mapping.
                    if map.contains_key(old) {
                        continue;
                    }
                    map_id(map, old, uuid::Uuid::new_v4().to_string());
                }
            }
        }
    }
    Ok(())
}

fn validate_aggregate_id_namespaces(
    snapshot: &LearningEvidenceSnapshot,
    program: &LearningProgramDto,
    outcome_rows: &[serde_json::Value],
) -> Result<()> {
    let mut core = HashMap::<String, &'static str>::new();
    let mut register = |id: &str, namespace: &'static str| -> Result<()> {
        if core.insert(id.to_owned(), namespace).is_some() {
            return Err(invalid("Pack reuses an ID across core entity types."));
        }
        Ok(())
    };
    register(&program.summary.id, "program")?;
    for module in &program.modules {
        register(&module.id, "module")?;
        for lesson in &module.lessons {
            register(&lesson.id, "lesson")?;
            for question in &lesson.questions {
                register(&question.id, "question")?;
            }
        }
    }
    for source in &program.sources {
        register(&source.id, "source_version")?;
    }
    for attempt in &program.attempts {
        register(&attempt.id, "attempt")?;
    }
    for row in outcome_rows {
        register(value_str(row, "id")?, "outcome")?;
    }
    let mut seen = HashMap::<String, String>::new();
    for table in &snapshot.tables {
        for row in &table.rows {
            for (key, namespace) in [("id", table.table.as_str()), ("operation_id", "operation")] {
                let Some(id) = row.get(key).and_then(serde_json::Value::as_str) else {
                    continue;
                };
                if let Some(previous) = seen.insert(id.to_owned(), namespace.to_owned()) {
                    if previous != namespace {
                        return Err(invalid(
                            "Pack aggregate reuses an identifier across entity types.",
                        ));
                    }
                }
                if let Some(core_namespace) = core.get(id) {
                    let expected_alias = (table.table == "learning_attempts"
                        && key == "id"
                        && *core_namespace == "attempt")
                        || (table.table == "learning_outcome_definitions"
                            && key == "id"
                            && *core_namespace == "outcome");
                    if !expected_alias {
                        return Err(invalid(
                            "Pack aggregate identifier collides with another imported entity type.",
                        ));
                    }
                }
            }
        }
    }
    Ok(())
}
async fn insert_evidence_snapshot(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    program_id: &str,
    snapshot: &LearningEvidenceSnapshot,
    map: &HashMap<String, String>,
) -> Result<()> {
    if snapshot.tables.len() != EVIDENCE_TABLES.len() {
        return Err(invalid("Pack aggregate table set is incomplete."));
    }
    for (table, spec) in snapshot.tables.iter().zip(EVIDENCE_TABLES) {
        if table.table != spec.table {
            return Err(invalid(
                "Pack aggregate contains an unknown or reordered table.",
            ));
        }
        let pragma = format!("PRAGMA table_info({})", spec.table);
        let cols = sqlx::query(&pragma)
            .fetch_all(&mut **tx)
            .await
            .map_err(db)?;
        let expected: Vec<String> = cols
            .into_iter()
            .map(|row| row.get::<String, _>("name"))
            .collect();
        if expected != table.columns {
            return Err(invalid(
                "Pack aggregate columns do not match this application version.",
            ));
        }
        if table.rows.len() > 50_000 {
            return Err(invalid("Pack aggregate has too many records."));
        }
        let col_sql = table
            .columns
            .iter()
            .map(|col| format!("\"{col}\""))
            .collect::<Vec<_>>()
            .join(",");
        for source_row in &table.rows {
            let mut row = source_row.clone();
            let object = row
                .as_object_mut()
                .ok_or_else(|| invalid("Pack aggregate row is malformed."))?;
            if object.len() != table.columns.len()
                || table
                    .columns
                    .iter()
                    .any(|column| !object.contains_key(column))
            {
                return Err(invalid("Pack aggregate row has a different schema."));
            }
            for (column, value) in object.iter_mut() {
                if column == "program_id" {
                    *value = serde_json::Value::String(program_id.to_owned());
                    continue;
                }
                if let Some(text) = value.as_str().map(str::to_owned) {
                    if column.ends_with("_json") {
                        let mut nested =
                            serde_json::from_str::<serde_json::Value>(&text).map_err(|_| {
                                invalid(format!(
                                    "Pack aggregate contains malformed JSON in {column}."
                                ))
                            })?;
                        if !map.is_empty() {
                            if matches!(
                                column.as_str(),
                                "source_version_ids_json"
                                    | "outcome_ids_json"
                                    | "source_ids_json"
                                    | "evidence_event_ids_json"
                            ) {
                                remap_id_array(&mut nested, map)?;
                            } else if matches!(
                                column.as_str(),
                                "source_json"
                                    | "citations_json"
                                    | "snapshot_json"
                                    | "before_json"
                                    | "after_json"
                                    | "activity_snapshot_json"
                                    | "request_json"
                                    | "result_json"
                                    | "prompt_json"
                                    | "response_json"
                                    | "findings_json"
                                    | "source_coverage_gaps_json"
                                    | "evaluation_json"
                                    | "assistance_json"
                                    | "details_json"
                            ) {
                                remap_nested_value(&mut nested, map);
                            }
                            if matches!(
                                column.as_str(),
                                "source_version_ids_json"
                                    | "outcome_ids_json"
                                    | "source_ids_json"
                                    | "evidence_event_ids_json"
                                    | "source_json"
                                    | "citations_json"
                                    | "snapshot_json"
                                    | "before_json"
                                    | "after_json"
                                    | "activity_snapshot_json"
                                    | "request_json"
                                    | "result_json"
                                    | "prompt_json"
                                    | "response_json"
                                    | "findings_json"
                                    | "source_coverage_gaps_json"
                                    | "evaluation_json"
                                    | "assistance_json"
                                    | "details_json"
                            ) {
                                *value = serde_json::Value::String(json_string(&nested)?);
                            }
                        }
                    } else if column == "id" || column.ends_with("_id") || column == "operation_id"
                    {
                        if let Some(mapped) = map.get(&text) {
                            *value = serde_json::Value::String(mapped.clone());
                        }
                    }
                }
            }
            if spec.table == "learning_memory"
                && object.contains_key("journal_id")
                && object.get("journal_id").is_some_and(|v| !v.is_null())
            {
                object.insert("journal_id".into(), serde_json::Value::Null);
            }
            if spec.table == "learning_recall_duplicate_suggestions" {
                let first = object
                    .get("card_id")
                    .and_then(serde_json::Value::as_str)
                    .ok_or_else(|| invalid("Recall duplicate record lacks a card ID."))?
                    .to_owned();
                let second = object
                    .get("possible_duplicate_card_id")
                    .and_then(serde_json::Value::as_str)
                    .ok_or_else(|| invalid("Recall duplicate record lacks a compared card ID."))?
                    .to_owned();
                if first == second {
                    return Err(invalid(
                        "Recall duplicate record references the same card twice.",
                    ));
                }
                if first > second {
                    object.insert("card_id".into(), serde_json::Value::String(second));
                    object.insert(
                        "possible_duplicate_card_id".into(),
                        serde_json::Value::String(first),
                    );
                }
            }
            let verb = if spec.table == "learning_memory" {
                ["INSERT", " OR REPLACE"].concat()
            } else {
                "INSERT".to_owned()
            };
            let mut query = sqlx::QueryBuilder::<sqlx::Sqlite>::new(format!(
                "{verb} INTO {} ({}) VALUES (",
                spec.table, col_sql
            ));
            let mut values = query.separated(",");
            for column in &table.columns {
                let value = object
                    .get(column)
                    .ok_or_else(|| invalid("Pack aggregate row is missing a column."))?;
                match value {
                    serde_json::Value::Null => {
                        values.push_bind(Option::<String>::None);
                    }
                    serde_json::Value::Bool(v) => {
                        values.push_bind(i64::from(*v));
                    }
                    serde_json::Value::Number(v) => {
                        if let Some(i) = v.as_i64() {
                            values.push_bind(i);
                        } else if let Some(f) = v.as_f64() {
                            values.push_bind(f);
                        } else {
                            return Err(invalid("Pack aggregate has an invalid number."));
                        }
                    }
                    serde_json::Value::String(v) => {
                        values.push_bind(v.clone());
                    }
                    serde_json::Value::Array(_) | serde_json::Value::Object(_) => {
                        values.push_bind(json_string(value)?);
                    }
                }
            }
            values.push_unseparated(")");
            query.build().execute(&mut **tx).await.map_err(db)?;
        }
    }
    Ok(())
}
fn hash<T: Serialize>(v: &T) -> Result<String> {
    Ok(format!("{:x}", Sha256::digest(json(v)?)))
}
fn now() -> i64 {
    chrono::Utc::now().timestamp_millis()
}
fn managed_dir(container: &crate::interfaces::di::Container) -> PathBuf {
    container.core.data_dir().join("learning-packs")
}
fn safe_filename(name: &str) -> Result<&str> {
    let n = name.trim();
    if n.is_empty()
        || n.len() > 100
        || n == "."
        || n == ".."
        || n.contains('/')
        || n.contains('\\')
        || n.contains(':')
        || n.chars().any(char::is_control)
    {
        return Err(invalid("Choose a simple pack filename without a path."));
    }
    Ok(n)
}
async fn managed_path(container: &crate::interfaces::di::Container, name: &str) -> Result<PathBuf> {
    let root = managed_dir(container);
    tokio::fs::create_dir_all(&root).await.map_err(io_error)?;
    let root = tokio::fs::canonicalize(&root).await.map_err(io_error)?;
    let file = safe_filename(name)?;
    let file = if file.to_lowercase().ends_with(".lattice-learning") {
        file.to_owned()
    } else {
        format!("{file}.lattice-learning")
    };
    let path = root.join(file);
    if path.parent() != Some(root.as_path()) {
        return Err(invalid("Pack path escaped its managed directory."));
    }
    if let Ok(meta) = tokio::fs::symlink_metadata(&path).await {
        if meta.file_type().is_symlink() || !meta.is_file() {
            return Err(invalid("Pack destination must be a regular managed file."));
        }
        return Err(invalid(
            "That managed pack filename already exists; choose another name.",
        ));
    }
    Ok(path)
}
async fn read_untrusted(path: &Path) -> Result<Vec<u8>> {
    let metadata = tokio::fs::symlink_metadata(path).await.map_err(io_error)?;
    if metadata.file_type().is_symlink()
        || !metadata.is_file()
        || metadata.len() == 0
        || metadata.len() > MAX_PACK_BYTES
    {
        return Err(invalid(
            "Import must be a regular learning-pack file no larger than 64 MiB.",
        ));
    }
    tokio::fs::read(path).await.map_err(io_error)
}
async fn atomic_write(path: &Path, bytes: &[u8]) -> Result<()> {
    use tokio::io::AsyncWriteExt;
    let tmp = path.with_extension(format!("{}.tmp", uuid::Uuid::new_v4()));
    let mut file = tokio::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&tmp)
        .await
        .map_err(io_error)?;
    file.write_all(bytes).await.map_err(io_error)?;
    file.sync_all().await.map_err(io_error)?;
    drop(file);
    tokio::fs::rename(&tmp, path).await.map_err(io_error)?;
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PrivateAnswerKey {
    question_id: String,
    correct_index: usize,
    explanation: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PracticalPackSnapshot {
    activities: Vec<crate::features::learning::practical_dto::LearningPracticalActivityDto>,
    runs: Vec<crate::features::learning::practical_dto::LearningPracticalRunDto>,
    simulations: Vec<serde_json::Value>,
    simulation_turns: Vec<serde_json::Value>,
    run_snapshots: Vec<serde_json::Value>,
    simulation_operations: Vec<serde_json::Value>,
    practical_operations: Vec<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CanvasPackSnapshot {
    canvases: Vec<serde_json::Value>,
    snapshots: Vec<serde_json::Value>,
    operations: Vec<serde_json::Value>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SourceHistorySnapshot {
    operations: Vec<serde_json::Value>,
    checks: Vec<serde_json::Value>,
}

#[derive(Clone)]
struct BackupExportRecord {
    id: String,
    program_id: String,
    operation_id: String,
    payload_hash: String,
    format_version: i64,
    root_sha256: String,
    destination_path: String,
    privacy_manifest_json: String,
    entry_manifest_json: String,
    manifest_json: String,
    created_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AggregateTableSnapshot {
    table: String,
    columns: Vec<String>,
    rows: Vec<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LearningEvidenceSnapshot {
    tables: Vec<AggregateTableSnapshot>,
}

#[derive(Clone, Copy)]
struct AggregateTableSpec {
    table: &'static str,
    filter: &'static str,
}

// Only these explicit program-scoped tables can be included in a pack. The
// SQL identifiers and joins are constants, never read from archive input.
const EVIDENCE_TABLES: &[AggregateTableSpec] = &[
    AggregateTableSpec { table: "learning_assessment_blueprints", filter: "program_id=?" },
    AggregateTableSpec { table: "learning_assessment_blueprint_slots", filter: "blueprint_id IN (SELECT id FROM learning_assessment_blueprints WHERE program_id=?)" },
    AggregateTableSpec { table: "learning_assessment_candidates", filter: "program_id=?" },
    AggregateTableSpec { table: "learning_assessment_candidate_outcomes", filter: "candidate_id IN (SELECT id FROM learning_assessment_candidates WHERE program_id=?)" },
    AggregateTableSpec { table: "learning_assessment_answer_keys", filter: "candidate_id IN (SELECT id FROM learning_assessment_candidates WHERE program_id=?)" },
    AggregateTableSpec { table: "learning_assessment_blueprint_candidates", filter: "program_id=?" },
    AggregateTableSpec { table: "learning_assessment_forms", filter: "program_id=?" },
    AggregateTableSpec { table: "learning_assessment_form_items", filter: "form_id IN (SELECT id FROM learning_assessment_forms WHERE program_id=?)" },
    AggregateTableSpec { table: "learning_assessment_form_item_outcomes", filter: "form_id IN (SELECT id FROM learning_assessment_forms WHERE program_id=?)" },
    AggregateTableSpec { table: "learning_assessment_response_revisions", filter: "form_id IN (SELECT id FROM learning_assessment_forms WHERE program_id=?)" },
    AggregateTableSpec { table: "learning_assessment_submissions", filter: "program_id=?" },
    AggregateTableSpec { table: "learning_assessment_item_results", filter: "form_id IN (SELECT form_id FROM learning_assessment_submissions WHERE program_id=?)" },
    AggregateTableSpec { table: "learning_assessment_operations", filter: "program_id=?" },
    AggregateTableSpec { table: "learning_curriculum_revisions", filter: "program_id=?" },
    AggregateTableSpec { table: "learning_curriculum_changes", filter: "revision_id IN (SELECT id FROM learning_curriculum_revisions WHERE program_id=?)" },
    AggregateTableSpec { table: "learning_curriculum_operations", filter: "program_id=?" },
    AggregateTableSpec { table: "learning_generation_jobs", filter: "program_id=?" },
    AggregateTableSpec { table: "learning_generation_job_events", filter: "job_id IN (SELECT id FROM learning_generation_jobs WHERE program_id=?)" },
    AggregateTableSpec { table: "learning_diagnostic_attempts", filter: "program_id=?" },
    AggregateTableSpec { table: "learning_diagnostic_operations", filter: "diagnostic_id IN (SELECT id FROM learning_diagnostic_attempts WHERE program_id=?)" },
    AggregateTableSpec { table: "learning_practice_sessions", filter: "program_id=?" },
    AggregateTableSpec { table: "learning_practice_artifact_revisions", filter: "program_id=?" },
    AggregateTableSpec { table: "learning_practice_operations", filter: "program_id=?" },
    AggregateTableSpec { table: "learning_practice_assistance", filter: "program_id=?" },
    AggregateTableSpec { table: "learning_practice_tutor_turns", filter: "program_id=?" },
    AggregateTableSpec { table: "learning_practice_proposals", filter: "program_id=?" },
    AggregateTableSpec { table: "learning_practice_submissions", filter: "program_id=?" },
    AggregateTableSpec { table: "learning_practice_evidence_events", filter: "program_id=?" },
    AggregateTableSpec { table: "study_decks", filter: "id IN (SELECT deck_id FROM learning_memory WHERE program_id=? AND deck_id IS NOT NULL)" },
    AggregateTableSpec { table: "study_cards", filter: "deck_id IN (SELECT deck_id FROM learning_memory WHERE program_id=? AND deck_id IS NOT NULL)" },
    AggregateTableSpec { table: "study_reviews", filter: "card_id IN (SELECT c.id FROM study_cards c JOIN learning_memory m ON m.deck_id=c.deck_id WHERE m.program_id=?)" },
    AggregateTableSpec { table: "learning_memory", filter: "program_id=?" },
    AggregateTableSpec { table: "learning_card_drafts", filter: "program_id=?" },
    AggregateTableSpec { table: "learning_card_origins", filter: "program_id=?" },
    AggregateTableSpec { table: "learning_recall_card_profiles", filter: "program_id=?" },
    AggregateTableSpec { table: "learning_recall_card_versions", filter: "card_id IN (SELECT card_id FROM learning_recall_card_profiles WHERE program_id=?)" },
    AggregateTableSpec { table: "learning_recall_duplicate_suggestions", filter: "program_id=?" },
    AggregateTableSpec { table: "learning_recall_scheduler_transitions", filter: "card_id IN (SELECT card_id FROM learning_recall_card_profiles WHERE program_id=?)" },
    AggregateTableSpec { table: "learning_recall_scheduler_migrations", filter: "program_id=?" },
    AggregateTableSpec { table: "learning_recall_operations", filter: "program_id=?" },
];

async fn record_external_id_conflict(
    pool: &SqlitePool,
    table: &str,
    column: &str,
    incoming_id: &str,
    incoming_program: &str,
    title: &str,
    policy: LearningPackConflictPolicy,
    conflicts: &mut Vec<LearningPackConflictDto>,
) -> Result<bool> {
    // Callers pass only fixed table/column identifiers from this module.
    let sql = format!("SELECT program_id FROM {table} WHERE {column}=?");
    let owner = sqlx::query_scalar::<_, String>(&sql)
        .bind(incoming_id)
        .fetch_optional(pool)
        .await
        .map_err(db)?;
    let Some(owner) = owner else { return Ok(true) };
    if owner == incoming_program || policy == LearningPackConflictPolicy::CreateCopy {
        return Ok(true);
    }
    conflicts.push(LearningPackConflictDto {
        entity_kind: table.into(),
        incoming_id: incoming_id.into(),
        incoming_title: title.into(),
        existing_id: incoming_id.into(),
        existing_title: "Identifier is already owned by another program".into(),
        resolution: "blocked".into(),
    });
    Ok(false)
}

async fn record_joined_id_conflict(
    pool: &SqlitePool,
    entity: &str,
    incoming_id: &str,
    owner_query: &str,
    incoming_program: &str,
    policy: LearningPackConflictPolicy,
    conflicts: &mut Vec<LearningPackConflictDto>,
) -> Result<bool> {
    let owner = sqlx::query_scalar::<_, String>(owner_query)
        .bind(incoming_id)
        .fetch_optional(pool)
        .await
        .map_err(db)?;
    let Some(owner) = owner else { return Ok(true) };
    if owner == incoming_program || policy == LearningPackConflictPolicy::CreateCopy {
        return Ok(true);
    }
    conflicts.push(LearningPackConflictDto {
        entity_kind: entity.into(),
        incoming_id: incoming_id.into(),
        incoming_title: entity.into(),
        existing_id: incoming_id.into(),
        existing_title: "Identifier is already owned by another program".into(),
        resolution: "blocked".into(),
    });
    Ok(false)
}

#[derive(Clone)]
pub struct LearningPackRepository {
    pool: SqlitePool,
}
impl LearningPackRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    async fn require_replace_is_recoverable(&self, program_id: &str) -> Result<()> {
        // Until every optional Learning Studio aggregate has a pack importer,
        // replacement must not cascade-delete any state outside the portable
        // snapshot. Derived retrieval chunks are intentionally rebuildable.
        const OMITTED: &[(&str, &str)] = &[
            ("notebook links", "SELECT COUNT(*) FROM learning_lesson_note_links WHERE program_id=?"),
            ("runtime profiles", "SELECT COUNT(DISTINCT runtime_profile_id) FROM learning_practical_activities WHERE program_id=? AND runtime_profile_id IS NOT NULL"),
            ("hidden practical evaluators", "SELECT COUNT(*) FROM learning_practical_files f JOIN learning_practical_activities a ON a.id=f.activity_id WHERE a.program_id=? AND f.role IN ('check','solution')"),
        ];
        for (label, sql) in OMITTED {
            let mut query = sqlx::query_scalar::<_, i64>(sql);
            let placeholders = sql.matches('?').count();
            for _ in 0..placeholders {
                query = query.bind(program_id);
            }
            if query.fetch_one(&self.pool).await.map_err(db)? > 0 {
                return Err(invalid(format!("Replace was blocked because the target has {label} that this pack version cannot restore. The existing program was left unchanged.")));
            }
        }
        Ok(())
    }

    pub async fn workspace(&self, program_id: &str) -> Result<LearningPortabilityWorkspaceDto> {
        let source_workspace = match LearningSourceLibraryRepository::new(self.pool.clone())
            .workspace(program_id)
            .await
        {
            Ok(value) => value,
            Err(AppError::NotFound(_)) => {
                crate::features::learning::dto::LearningSourceWorkspaceDto {
                    program_id: program_id.into(),
                    sources: vec![],
                }
            }
            Err(error) => return Err(error),
        };
        let exports = sqlx::query("SELECT manifest_json,destination_path,created_at,id,program_id FROM learning_pack_exports WHERE program_id=? ORDER BY created_at DESC")
            .bind(program_id).fetch_all(&self.pool).await.map_err(db)?.into_iter().map(|r| -> Result<_> {
                Ok(LearningPackExportDto { id:r.get("id"), program_id:r.get("program_id"), destination_path:r.get("destination_path"), manifest:parse(r.get::<String,_>("manifest_json").as_bytes())?, created_at:r.get("created_at") })
            }).collect::<Result<Vec<_>>>()?;
        let import_previews = sqlx::query("SELECT * FROM learning_pack_import_previews WHERE program_id=? ORDER BY created_at DESC")
            .bind(program_id).fetch_all(&self.pool).await.map_err(db)?.into_iter().map(|r| -> Result<_> {
                let status = match r.get::<String,_>("status").as_str() { "pending"=>LearningPackPreviewStatus::Pending,"applied"=>LearningPackPreviewStatus::Applied,"cancelled"=>LearningPackPreviewStatus::Cancelled,"stale"=>LearningPackPreviewStatus::Stale,_=>return Err(AppError::Database("Invalid pack preview status".into())) };
                let policy = match r.get::<String,_>("conflict_policy").as_str() { "create_copy"=>LearningPackConflictPolicy::CreateCopy,"merge_safe"=>LearningPackConflictPolicy::MergeSafe,"replace_after_backup"=>LearningPackConflictPolicy::ReplaceAfterBackup,_=>return Err(AppError::Database("Invalid pack conflict policy".into())) };
                let changes:Vec<LearningPackChangeDto>=parse(r.get::<String,_>("changes_json").as_bytes())?;
                let conflicts:Vec<LearningPackConflictDto>=parse(r.get::<String,_>("conflicts_json").as_bytes())?;
                let warnings:Vec<String>=parse(r.get::<String,_>("warnings_json").as_bytes())?;
                let manifest:LearningPackManifest=parse(r.get::<String,_>("manifest_json").as_bytes())?;
                let can_apply=status==LearningPackPreviewStatus::Pending && conflicts.iter().all(|c| c.resolution!="blocked");
                Ok(LearningPackImportPreviewDto { id:r.get("id"),source_path:r.get("source_path"),incoming_program_id:r.get("program_id"),incoming_program_title:r.get("program_title"),conflict_policy:policy,conflicts,changes,warnings,status,can_apply,created_at:r.get("created_at"),decided_at:r.get("decided_at"),manifest })
            }).collect::<Result<Vec<_>>>()?;
        let imports = sqlx::query("SELECT * FROM learning_pack_imports WHERE imported_program_id=? ORDER BY imported_at DESC")
            .bind(program_id).fetch_all(&self.pool).await.map_err(db)?.into_iter().map(|r| -> Result<_> { Ok(LearningPackImportResultDto { id:r.get("id"),preview_id:r.get("preview_id"),imported_program_id:r.get("imported_program_id"),backup_id:r.get("backup_id"),applied_changes:parse(r.get::<String,_>("applied_changes_json").as_bytes())?,imported_at:r.get("imported_at") }) }).collect::<Result<Vec<_>>>()?;
        Ok(LearningPortabilityWorkspaceDto {
            program_id: program_id.into(),
            exports,
            import_previews,
            imports,
            source_workspace,
        })
    }

    pub async fn export(
        &self,
        container: &crate::interfaces::di::Container,
        req: &ExportLearningPackRequestDto,
    ) -> Result<LearningPortabilityWorkspaceDto> {
        validate_uuid(&req.operation_id, "operation")?;
        validate_uuid(&req.program_id, "program")?;
        let payload = hash(req)?;
        if let Some(path) = self.replay(&req.operation_id, "export", &payload).await? {
            if !Path::new(&path).exists() {
                return Err(invalid(
                    "The prior pack export file is missing; choose a new operation ID.",
                ));
            }
            return self.workspace(&req.program_id).await;
        }
        let snapshot = export::snapshot(&self.pool, req).await?;
        let manifest_bytes = encode_learning_pack(snapshot)?;
        let path = managed_path(container, &req.file_name).await?;
        atomic_write(&path, &manifest_bytes).await?;
        let decoded = decode_learning_pack(&manifest_bytes)?;
        let id = uuid::Uuid::new_v4().to_string();
        let created = now();
        let mut tx = self.pool.begin().await.map_err(db)?;
        sqlx::query("INSERT INTO learning_pack_exports(id,program_id,operation_id,payload_hash,format_version,root_sha256,destination_path,privacy_manifest_json,entry_manifest_json,manifest_json,created_at) VALUES(?,?,?,?,?,?,?,?,?,?,?)")
            .bind(&id).bind(&req.program_id).bind(&req.operation_id).bind(&payload).bind(decoded.manifest.version as i64).bind(&decoded.manifest.root_sha256).bind(path.to_string_lossy().to_string()).bind(json_string(&decoded.manifest.privacy)?).bind(json_string(&decoded.manifest.entries)?).bind(json_string(&decoded.manifest)?).bind(created).execute(&mut *tx).await.map_err(db)?;
        sqlx::query("INSERT INTO learning_portability_operations(operation_id,program_id,kind,payload_hash,result_id,created_at) VALUES(?,?,'export',?,?,?)").bind(&req.operation_id).bind(&req.program_id).bind(&payload).bind(&id).bind(created).execute(&mut *tx).await.map_err(db)?;
        tx.commit().await.map_err(db)?;
        self.workspace(&req.program_id).await
    }

    pub async fn preview(
        &self,
        req: &PreviewLearningPackImportRequestDto,
    ) -> Result<LearningPortabilityWorkspaceDto> {
        validate_uuid(&req.operation_id, "operation")?;
        validate_uuid(&req.preview_id, "preview")?;
        let payload = hash(req)?;
        if self
            .replay(&req.operation_id, "preview_import", &payload)
            .await?
            .is_some()
        {
            return self.workspace_for_preview(&req.preview_id).await;
        }
        let path = PathBuf::from(&req.source_path);
        let bytes = read_untrusted(&path).await?;
        let decoded = decode_learning_pack(&bytes)?;
        let mut program: LearningProgramDto = parse(
            decoded
                .entries
                .get(PROGRAM_ENTRY)
                .ok_or_else(|| invalid("Pack is missing its program."))?,
        )?;
        validate_program(&program)?;
        let mut warnings = Vec::<String>::new();
        if let Some(bytes) = decoded.entries.get(PRACTICAL_ENTRY) {
            let snapshot: PracticalPackSnapshot = parse(bytes)?;
            if snapshot.activities.iter().any(|activity| {
                activity.runtime_kind != crate::features::learning::practical_dto::LearningPracticalRuntimeKind::None
            }) {
                warnings.push("Imported executable activities are restored in review-only mode without hidden evaluator files; regenerate and review the exercise before running it.".into());
            }
        }
        if let Some(bytes) = decoded.entries.get("program/attempts.json") {
            program.attempts = parse(bytes)?;
        }
        let exists: Option<i64> = sqlx::query_scalar("SELECT 1 FROM learning_programs WHERE id=?")
            .bind(&program.summary.id)
            .fetch_optional(&self.pool)
            .await
            .map_err(db)?;
        let mut conflicts = Vec::new();
        let mut changes = vec![LearningPackChangeDto {
            entity_kind: "program".into(),
            entity_id: program.summary.id.clone(),
            action: "import".into(),
            description: format!("Import {}", program.summary.title),
        }];
        let mut can_apply = true;
        match req.conflict_policy {
            LearningPackConflictPolicy::CreateCopy => {
                if exists.is_some() {
                    conflicts.push(LearningPackConflictDto {
                        entity_kind: "program".into(),
                        incoming_id: program.summary.id.clone(),
                        incoming_title: program.summary.title.clone(),
                        existing_id: program.summary.id.clone(),
                        existing_title: sqlx::query_scalar(
                            "SELECT title FROM learning_programs WHERE id=?",
                        )
                        .bind(&program.summary.id)
                        .fetch_one(&self.pool)
                        .await
                        .map_err(db)?,
                        resolution: "create_copy_with_new_ids".into(),
                    });
                }
            }
            LearningPackConflictPolicy::MergeSafe => {
                if exists.is_some() {
                    can_apply = false;
                    conflicts.push(LearningPackConflictDto {
                        entity_kind: "program".into(),
                        incoming_id: program.summary.id.clone(),
                        incoming_title: program.summary.title.clone(),
                        existing_id: program.summary.id.clone(),
                        existing_title: sqlx::query_scalar(
                            "SELECT title FROM learning_programs WHERE id=?",
                        )
                        .bind(&program.summary.id)
                        .fetch_one(&self.pool)
                        .await
                        .map_err(db)?,
                        resolution: "blocked".into(),
                    });
                }
            }
            LearningPackConflictPolicy::ReplaceAfterBackup => {
                if exists.is_none() {
                    if let Some(change) = changes.first_mut() {
                        change.action = "create".into();
                    }
                } else if let Err(error) = self
                    .require_replace_is_recoverable(&program.summary.id)
                    .await
                {
                    can_apply = false;
                    conflicts.push(LearningPackConflictDto {
                        entity_kind: "program_history".into(),
                        incoming_id: program.summary.id.clone(),
                        incoming_title: program.summary.title.clone(),
                        existing_id: program.summary.id.clone(),
                        existing_title: error.to_string(),
                        resolution: "blocked".into(),
                    });
                }
            }
        }
        // Global primary-key collisions must be previewed even if the incoming
        // program ID itself is free. CreateCopy always remaps these IDs; merge
        // blocks collisions; replace may reuse IDs only inside its target.
        let mut candidates = Vec::<(&str, &str, &str)>::new();
        for module in &program.modules {
            candidates.push(("module", &module.id, &module.title));
            for lesson in &module.lessons {
                candidates.push(("lesson", &lesson.id, &lesson.title));
                for question in &lesson.questions {
                    candidates.push(("question", &question.id, &question.prompt));
                }
            }
        }
        for attempt in &program.attempts {
            candidates.push(("attempt", &attempt.id, "Submitted assessment"));
        }
        for (kind, id, title) in candidates {
            let table = match kind {
                "module" => "learning_modules",
                "lesson" => "learning_lessons",
                "question" => "learning_questions",
                _ => "learning_attempts",
            };
            let sql = format!(
                "SELECT program_id,{} AS title FROM {table} WHERE id=?",
                if kind == "attempt" { "kind" } else { "title" }
            );
            if let Some(row) = sqlx::query(&sql)
                .bind(id)
                .fetch_optional(&self.pool)
                .await
                .map_err(db)?
            {
                let existing_program: String = row.get("program_id");
                if existing_program != program.summary.id {
                    let resolution =
                        if req.conflict_policy == LearningPackConflictPolicy::CreateCopy {
                            "create_copy_with_new_ids"
                        } else {
                            "blocked"
                        };
                    if resolution == "blocked" {
                        can_apply = false;
                    }
                    conflicts.push(LearningPackConflictDto {
                        entity_kind: kind.into(),
                        incoming_id: id.into(),
                        incoming_title: title.into(),
                        existing_id: id.into(),
                        existing_title: row.get("title"),
                        resolution: resolution.into(),
                    });
                }
            }
        }
        // Preview table-scoped primary-key collisions beyond the legacy
        // curriculum. Create-copy remaps these IDs; replace is allowed to
        // reuse IDs owned by its own target only.
        let mut checked_ids = HashSet::<(String, String)>::new();
        if let Some(bytes) = decoded.entries.get("sources/library.json") {
            let workspace: crate::features::learning::dto::LearningSourceWorkspaceDto =
                parse(bytes)?;
            for source in &workspace.sources {
                if checked_ids.insert(("learning_source_library".into(), source.id.clone())) {
                    let source_title = source
                        .active_version
                        .as_ref()
                        .map(|version| version.title.as_str())
                        .unwrap_or(source.origin.as_str());
                    can_apply &= record_external_id_conflict(
                        &self.pool,
                        "learning_source_library",
                        "id",
                        &source.id,
                        &program.summary.id,
                        source_title,
                        req.conflict_policy,
                        &mut conflicts,
                    )
                    .await?;
                }
                for version in &source.versions {
                    let external_count = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM learning_source_versions WHERE id=? AND program_id<>?")
                        .bind(&version.id).bind(&program.summary.id).fetch_one(&self.pool).await.map_err(db)?;
                    if external_count > 0
                        && req.conflict_policy != LearningPackConflictPolicy::CreateCopy
                    {
                        can_apply = false;
                        conflicts.push(LearningPackConflictDto {
                            entity_kind: "learning_source_versions".into(),
                            incoming_id: version.id.clone(),
                            incoming_title: version.title.clone(),
                            existing_id: version.id.clone(),
                            existing_title: "Identifier is already owned by another program".into(),
                            resolution: "blocked".into(),
                        });
                    }
                }
            }
        }
        if let Some(bytes) = decoded.entries.get("sources/selectors.json") {
            let rows: Vec<serde_json::Value> = parse(bytes)?;
            for row in rows {
                let id = value_str(&row, "id")?;
                can_apply &= record_external_id_conflict(
                    &self.pool,
                    "learning_source_selectors",
                    "id",
                    id,
                    &program.summary.id,
                    "Source selector",
                    req.conflict_policy,
                    &mut conflicts,
                )
                .await?;
            }
        }
        if let Some(bytes) = decoded.entries.get("sources/history.json") {
            let history: SourceHistorySnapshot = parse(bytes)?;
            for (table, rows) in [
                ("learning_source_operations", history.operations),
                ("learning_source_refresh_checks", history.checks),
            ] {
                for row in rows {
                    let id = value_str(&row, "operationId")?;
                    if checked_ids.insert((table.into(), id.into())) {
                        can_apply &= record_external_id_conflict(
                            &self.pool,
                            table,
                            "operation_id",
                            id,
                            &program.summary.id,
                            "Source history operation",
                            req.conflict_policy,
                            &mut conflicts,
                        )
                        .await?;
                    }
                }
            }
        }
        if let Some(bytes) = decoded.entries.get(PRACTICAL_ENTRY) {
            let snapshot: PracticalPackSnapshot = parse(bytes)?;
            for activity in &snapshot.activities {
                can_apply &= record_external_id_conflict(
                    &self.pool,
                    "learning_practical_activities",
                    "id",
                    &activity.id,
                    &program.summary.id,
                    &activity.title,
                    req.conflict_policy,
                    &mut conflicts,
                )
                .await?;
            }
            for run in &snapshot.runs {
                can_apply &= record_external_id_conflict(
                    &self.pool,
                    "learning_practical_runs",
                    "id",
                    &run.id,
                    &program.summary.id,
                    "Practical run",
                    req.conflict_policy,
                    &mut conflicts,
                )
                .await?;
            }
            for (table, rows) in [
                ("learning_simulation_sessions", &snapshot.simulations),
                ("learning_simulation_turns", &snapshot.simulation_turns),
            ] {
                for row in rows {
                    can_apply &= record_external_id_conflict(
                        &self.pool,
                        table,
                        "id",
                        value_str(row, "id")?,
                        &program.summary.id,
                        table,
                        req.conflict_policy,
                        &mut conflicts,
                    )
                    .await?;
                }
            }
            for raw in &snapshot.run_snapshots {
                let id = value_str(raw, "operationId")?;
                can_apply &= record_external_id_conflict(
                    &self.pool,
                    "learning_practical_run_operations",
                    "operation_id",
                    id,
                    &program.summary.id,
                    "Practical run operation",
                    req.conflict_policy,
                    &mut conflicts,
                )
                .await?;
            }
            for operation in &snapshot.practical_operations {
                let table = value_str(operation, "table")?;
                if !matches!(
                    table,
                    "learning_practical_activity_operations" | "learning_practical_run_operations"
                ) {
                    return Err(invalid("Pack practical operation table is invalid."));
                }
                let id = value_str(operation, "operationId")?;
                if checked_ids.insert((table.into(), id.into())) {
                    can_apply &= record_external_id_conflict(
                        &self.pool,
                        table,
                        "operation_id",
                        id,
                        &program.summary.id,
                        table,
                        req.conflict_policy,
                        &mut conflicts,
                    )
                    .await?;
                }
            }
            for operation in &snapshot.simulation_operations {
                can_apply &= record_external_id_conflict(
                    &self.pool,
                    "learning_simulation_operations",
                    "operation_id",
                    value_str(operation, "operationId")?,
                    &program.summary.id,
                    "Simulation operation",
                    req.conflict_policy,
                    &mut conflicts,
                )
                .await?;
            }
        }
        if let Some(bytes) = decoded.entries.get(CANVAS_ENTRY) {
            let snapshot: CanvasPackSnapshot = parse(bytes)?;
            for (table, rows, column) in [
                ("learning_canvases", &snapshot.canvases, "id"),
                ("learning_canvas_snapshots", &snapshot.snapshots, "id"),
                (
                    "learning_canvas_operations",
                    &snapshot.operations,
                    "operationId",
                ),
            ] {
                for row in rows {
                    let id = if column == "id" {
                        value_str(row, "id")?
                    } else {
                        value_str(row, "operationId")?
                    };
                    can_apply &= record_external_id_conflict(
                        &self.pool,
                        table,
                        if column == "id" { "id" } else { "operation_id" },
                        id,
                        &program.summary.id,
                        table,
                        req.conflict_policy,
                        &mut conflicts,
                    )
                    .await?;
                }
            }
        }
        if let Some(bytes) = decoded.entries.get("evidence/learning-aggregate.json") {
            let snapshot: LearningEvidenceSnapshot = parse(bytes)?;
            if snapshot.tables.len() != EVIDENCE_TABLES.len() {
                return Err(invalid("Pack aggregate table set is incomplete."));
            }
            for (table, spec) in snapshot.tables.iter().zip(EVIDENCE_TABLES) {
                if table.table != spec.table {
                    return Err(invalid(
                        "Pack aggregate contains an unknown or reordered table.",
                    ));
                }
                if !table.columns.iter().any(|column| column == "program_id") {
                    continue;
                }
                for row in &table.rows {
                    for (column, db_column) in [("id", "id"), ("operation_id", "operation_id")] {
                        if let Some(id) = row.get(column).and_then(serde_json::Value::as_str) {
                            if checked_ids.insert((table.table.clone(), id.into())) {
                                can_apply &= record_external_id_conflict(
                                    &self.pool,
                                    spec.table,
                                    db_column,
                                    id,
                                    &program.summary.id,
                                    &table.table,
                                    req.conflict_policy,
                                    &mut conflicts,
                                )
                                .await?;
                            }
                        }
                    }
                }
            }
        }
        for (entry, table) in [
            (OUTCOMES_ENTRY, "learning_outcome_definitions"),
            (EVIDENCE_ENTRY, "learning_evidence_events"),
            (FOLLOW_UP_ENTRY, "learning_follow_up_recommendations"),
        ] {
            if let Some(bytes) = decoded.entries.get(entry) {
                let rows: Vec<serde_json::Value> = parse(bytes)?;
                for row in rows {
                    let id = value_str(&row, "id")?;
                    if checked_ids.insert((table.into(), id.into())) {
                        can_apply &= record_external_id_conflict(
                            &self.pool,
                            table,
                            "id",
                            id,
                            &program.summary.id,
                            table,
                            req.conflict_policy,
                            &mut conflicts,
                        )
                        .await?;
                    }
                }
            }
        }
        if let Some(bytes) = decoded.entries.get("evidence/learning-aggregate.json") {
            let snapshot: LearningEvidenceSnapshot = parse(bytes)?;
            for table in &snapshot.tables {
                if table.columns.iter().any(|column| column == "program_id") {
                    continue;
                }
                let owner_query = match table.table.as_str() {
                    "study_decks" => "SELECT COALESCE(m.program_id,'') FROM study_decks d LEFT JOIN learning_memory m ON m.deck_id=d.id WHERE d.id=?",
                    "study_cards" => "SELECT COALESCE(m.program_id,'') FROM study_cards c JOIN study_decks d ON d.id=c.deck_id LEFT JOIN learning_memory m ON m.deck_id=d.id WHERE c.id=?",
                    "study_reviews" => "SELECT COALESCE(m.program_id,'') FROM study_reviews r JOIN study_cards c ON c.id=r.card_id JOIN study_decks d ON d.id=c.deck_id LEFT JOIN learning_memory m ON m.deck_id=d.id WHERE r.id=?",
                    "learning_recall_scheduler_transitions" => "SELECT COALESCE(p.program_id,'') FROM learning_recall_scheduler_transitions t LEFT JOIN learning_recall_card_profiles p ON p.card_id=t.card_id WHERE t.id=?",
                    _ => continue,
                };
                for row in &table.rows {
                    if let Some(id) = row.get("id").and_then(serde_json::Value::as_str) {
                        if checked_ids.insert((table.table.clone(), id.into())) {
                            can_apply &= record_joined_id_conflict(
                                &self.pool,
                                &table.table,
                                id,
                                owner_query,
                                &program.summary.id,
                                req.conflict_policy,
                                &mut conflicts,
                            )
                            .await?;
                        }
                    }
                }
            }
        }
        let result_program_id = if req.conflict_policy == LearningPackConflictPolicy::CreateCopy {
            uuid::Uuid::new_v4().to_string()
        } else {
            program.summary.id.clone()
        };
        let id = req.preview_id.clone();
        let timestamp = now();
        let manifest_json = serde_json::to_string(&decoded.manifest)
            .map_err(|e| AppError::Serialization(e.to_string()))?;
        let mut tx = self.pool.begin().await.map_err(db)?;
        sqlx::query("INSERT INTO learning_pack_import_previews(id,operation_id,payload_hash,source_path,root_sha256,program_id,program_title,conflict_policy,conflicts_json,changes_json,warnings_json,manifest_json,status,created_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,'pending',?)")
            .bind(&id).bind(&req.operation_id).bind(&payload).bind(&req.source_path).bind(&decoded.manifest.root_sha256).bind(&result_program_id).bind(&program.summary.title).bind(policy_str(req.conflict_policy)).bind(json_string(&conflicts)?).bind(json_string(&changes)?).bind(json_string(&warnings)?).bind(&manifest_json).bind(timestamp).execute(&mut *tx).await.map_err(db)?;
        sqlx::query("INSERT INTO learning_portability_operations(operation_id,program_id,kind,payload_hash,result_id,created_at) VALUES(?,?,'preview_import',?,?,?)").bind(&req.operation_id).bind(&result_program_id).bind(&payload).bind(&id).bind(timestamp).execute(&mut *tx).await.map_err(db)?;
        tx.commit().await.map_err(db)?;
        let mut w = self.workspace(&result_program_id).await?;
        if !can_apply {
            if let Some(p) = w.import_previews.first_mut() {
                p.can_apply = false;
            }
        }
        Ok(w)
    }
    pub async fn cancel(
        &self,
        req: &CancelLearningPackImportPreviewRequestDto,
    ) -> Result<LearningPortabilityWorkspaceDto> {
        validate_uuid(&req.operation_id, "operation")?;
        validate_uuid(&req.preview_id, "preview")?;
        let payload = hash(req)?;
        if self
            .replay(&req.operation_id, "cancel_import", &payload)
            .await?
            .is_some()
        {
            return self.workspace_for_preview(&req.preview_id).await;
        }
        let now = now();
        let mut tx = self.pool.begin().await.map_err(db)?;
        let row =
            sqlx::query("SELECT program_id,status FROM learning_pack_import_previews WHERE id=?")
                .bind(&req.preview_id)
                .fetch_optional(&mut *tx)
                .await
                .map_err(db)?
                .ok_or_else(|| AppError::NotFound("Pack preview not found".into()))?;
        if row.get::<String, _>("status") != "pending" {
            return Err(invalid("Only a pending pack preview can be cancelled."));
        }
        let program_id: String = row.get("program_id");
        sqlx::query("UPDATE learning_pack_import_previews SET status='cancelled',decided_at=? WHERE id=? AND status='pending'").bind(now).bind(&req.preview_id).execute(&mut *tx).await.map_err(db)?;
        sqlx::query("INSERT INTO learning_portability_operations(operation_id,program_id,kind,payload_hash,result_id,created_at) VALUES(?,?,'cancel_import',?,?,?)").bind(&req.operation_id).bind(&program_id).bind(&payload).bind(&req.preview_id).bind(now).execute(&mut *tx).await.map_err(db)?;
        tx.commit().await.map_err(db)?;
        self.workspace(&program_id).await
    }
    pub async fn apply(
        &self,
        container: &crate::interfaces::di::Container,
        req: &ApplyLearningPackImportRequestDto,
    ) -> Result<LearningPortabilityWorkspaceDto> {
        validate_uuid(&req.operation_id, "operation")?;
        validate_uuid(&req.preview_id, "preview")?;
        let payload = hash(req)?;
        if self
            .replay(&req.operation_id, "apply_import", &payload)
            .await?
            .is_some()
        {
            return self.workspace_for_preview(&req.preview_id).await;
        }
        let preview = sqlx::query("SELECT * FROM learning_pack_import_previews WHERE id=?")
            .bind(&req.preview_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(db)?
            .ok_or_else(|| AppError::NotFound("Pack preview not found".into()))?;
        if preview.get::<String, _>("status") != "pending" {
            return Err(invalid("Only a pending pack preview can be applied."));
        }
        let preview_conflicts: Vec<LearningPackConflictDto> =
            parse(preview.get::<String, _>("conflicts_json").as_bytes())?;
        if preview_conflicts
            .iter()
            .any(|conflict| conflict.resolution == "blocked")
        {
            return Err(invalid(
                "This import preview contains unresolved conflicts and cannot be applied.",
            ));
        }
        let source_path = PathBuf::from(preview.get::<String, _>("source_path"));
        let raw = read_untrusted(&source_path).await?; // Re-read current bytes; a preview never authorizes changed data.
        let decoded = decode_learning_pack(&raw)?;
        let expected: String = preview.get("root_sha256");
        if decoded.manifest.root_sha256 != expected
            || decoded.manifest.root_sha256 != req.expected_root_sha256
        {
            return Err(invalid(
                "The pack changed after preview. Preview it again before applying.",
            ));
        }
        let mut program: LearningProgramDto = parse(
            decoded
                .entries
                .get(PROGRAM_ENTRY)
                .ok_or_else(|| invalid("Pack is missing its program."))?,
        )?;
        validate_program(&program)?;
        let source_workspace: Option<crate::features::learning::dto::LearningSourceWorkspaceDto> =
            decoded
                .entries
                .get("sources/library.json")
                .map(|bytes| parse(bytes))
                .transpose()?;
        let mut practical_snapshot: Option<PracticalPackSnapshot> = decoded
            .entries
            .get(PRACTICAL_ENTRY)
            .map(|bytes| parse(bytes))
            .transpose()?;
        let mut canvas_snapshot: Option<CanvasPackSnapshot> = decoded
            .entries
            .get(CANVAS_ENTRY)
            .map(|bytes| parse(bytes))
            .transpose()?;
        let evidence_snapshot: Option<LearningEvidenceSnapshot> = decoded
            .entries
            .get("evidence/learning-aggregate.json")
            .map(|bytes| parse(bytes))
            .transpose()?;
        let mut source_history: Option<SourceHistorySnapshot> = decoded
            .entries
            .get("sources/history.json")
            .map(|bytes| parse(bytes))
            .transpose()?;
        let imported_keys: Vec<PrivateAnswerKey> = match decoded.entries.get(KEYS_ENTRY) {
            Some(bytes) => parse(bytes)?,
            None => return Err(invalid("Pack is missing assessment answer data.")),
        };
        validate_program(&program)?;
        validate_answer_keys(&program, &imported_keys)?;
        let target_id: String = preview.get("program_id");
        let policy = parse_policy(&preview.get::<String, _>("conflict_policy"))?;
        let existing: Option<i64> =
            sqlx::query_scalar("SELECT 1 FROM learning_programs WHERE id=?")
                .bind(&target_id)
                .fetch_optional(&self.pool)
                .await
                .map_err(db)?;
        let backup_record = if policy == LearningPackConflictPolicy::ReplaceAfterBackup
            && existing.is_some()
        {
            self.require_replace_is_recoverable(&target_id).await?;
            // Use the same full snapshot exporter as user backups so replacement
            // can be recovered with its assessment keys and immutable sources.
            let backup_id = uuid::Uuid::new_v4().to_string();
            let backup_operation_id = uuid::Uuid::new_v4().to_string();
            let backup_name = format!("backup-{backup_id}.lattice-learning");
            self.export(
                container,
                &ExportLearningPackRequestDto {
                    operation_id: backup_operation_id.clone(),
                    program_id: target_id.clone(),
                    file_name: backup_name,
                    include_evidence: true,
                    include_practical_artifacts: true,
                    include_source_bodies: true,
                    source_body_redistribution_confirmed: true,
                },
            )
            .await?;
            let row=sqlx::query("SELECT program_id,operation_id,payload_hash,format_version,root_sha256,destination_path,privacy_manifest_json,entry_manifest_json,manifest_json,created_at FROM learning_pack_exports WHERE operation_id=? AND program_id=?")
                    .bind(&backup_operation_id).bind(&target_id).fetch_optional(&self.pool).await.map_err(db)?.ok_or_else(||AppError::Database("Replacement backup export was not recorded.".into()))?;
            Some(BackupExportRecord {
                // The recovery ID is also embedded in the managed filename, so
                // the restored export row remains directly discoverable after
                // the original row is removed by the replacement cascade.
                id: backup_id,
                program_id: row.get("program_id"),
                operation_id: row.get("operation_id"),
                payload_hash: row.get("payload_hash"),
                format_version: row.get("format_version"),
                root_sha256: row.get("root_sha256"),
                destination_path: row.get("destination_path"),
                privacy_manifest_json: row.get("privacy_manifest_json"),
                entry_manifest_json: row.get("entry_manifest_json"),
                manifest_json: row.get("manifest_json"),
                created_at: row.get("created_at"),
            })
        } else {
            None
        };
        if policy == LearningPackConflictPolicy::MergeSafe && existing.is_some() {
            return Err(invalid(
                "Merge-safe import is blocked because the incoming program ID already exists.",
            ));
        }
        let mut id_map = HashMap::<String, String>::new();
        if policy == LearningPackConflictPolicy::CreateCopy {
            map_id(&mut id_map, &program.summary.id, target_id.clone());
            let outcome_rows: Vec<serde_json::Value> = decoded
                .entries
                .get(OUTCOMES_ENTRY)
                .map(|bytes| parse(bytes))
                .transpose()?
                .unwrap_or_default();
            if let Some(snapshot) = &evidence_snapshot {
                validate_aggregate_id_namespaces(snapshot, &program, &outcome_rows)?;
            }
            if let Some(history) = &source_history {
                for op in &history.operations {
                    map_id(
                        &mut id_map,
                        value_str(op, "operationId")?,
                        uuid::Uuid::new_v4().to_string(),
                    );
                }
            }
            {
                for row in &outcome_rows {
                    let id = row
                        .get("id")
                        .and_then(serde_json::Value::as_str)
                        .ok_or_else(|| invalid("Pack outcome has no ID."))?;
                    map_id(&mut id_map, id, uuid::Uuid::new_v4().to_string());
                }
            }
            for entry in [EVIDENCE_ENTRY, FOLLOW_UP_ENTRY] {
                if let Some(bytes) = decoded.entries.get(entry) {
                    let rows: Vec<serde_json::Value> = parse(bytes)?;
                    for row in &rows {
                        map_id(
                            &mut id_map,
                            value_str(row, "id")?,
                            uuid::Uuid::new_v4().to_string(),
                        );
                    }
                }
            }
            if let Some(workspace) = &source_workspace {
                for source in &workspace.sources {
                    map_id(&mut id_map, &source.id, uuid::Uuid::new_v4().to_string());
                    for version in &source.versions {
                        map_id(&mut id_map, &version.id, uuid::Uuid::new_v4().to_string());
                    }
                }
            }
            if let Some(bytes) = decoded.entries.get("sources/selectors.json") {
                let selectors: Vec<serde_json::Value> = parse(bytes)?;
                for selector in &selectors {
                    map_id(
                        &mut id_map,
                        value_str(selector, "id")?,
                        uuid::Uuid::new_v4().to_string(),
                    );
                }
            }
            if let Some(snapshot) = &practical_snapshot {
                for activity in &snapshot.activities {
                    map_id(&mut id_map, &activity.id, uuid::Uuid::new_v4().to_string());
                }
                for run in &snapshot.runs {
                    map_id(&mut id_map, &run.id, uuid::Uuid::new_v4().to_string());
                }
                for raw in &snapshot.run_snapshots {
                    map_id(
                        &mut id_map,
                        value_str(raw, "operationId")?,
                        uuid::Uuid::new_v4().to_string(),
                    );
                }
                for session in &snapshot.simulations {
                    map_id(
                        &mut id_map,
                        value_str(session, "id")?,
                        uuid::Uuid::new_v4().to_string(),
                    );
                    map_id(
                        &mut id_map,
                        value_str(session, "operationId")?,
                        uuid::Uuid::new_v4().to_string(),
                    );
                }
                for turn in &snapshot.simulation_turns {
                    map_id(
                        &mut id_map,
                        value_str(turn, "id")?,
                        uuid::Uuid::new_v4().to_string(),
                    );
                    map_id(
                        &mut id_map,
                        value_str(turn, "operationId")?,
                        uuid::Uuid::new_v4().to_string(),
                    );
                }
                for operation in &snapshot.simulation_operations {
                    map_id(
                        &mut id_map,
                        value_str(operation, "operationId")?,
                        uuid::Uuid::new_v4().to_string(),
                    );
                }
                for operation in &snapshot.practical_operations {
                    map_id(
                        &mut id_map,
                        value_str(operation, "operationId")?,
                        uuid::Uuid::new_v4().to_string(),
                    );
                }
            }
            if let Some(snapshot) = &canvas_snapshot {
                for row in &snapshot.canvases {
                    map_id(
                        &mut id_map,
                        value_str(row, "id")?,
                        uuid::Uuid::new_v4().to_string(),
                    );
                }
                for row in &snapshot.snapshots {
                    map_id(
                        &mut id_map,
                        value_str(row, "id")?,
                        uuid::Uuid::new_v4().to_string(),
                    );
                }
                for row in &snapshot.operations {
                    map_id(
                        &mut id_map,
                        value_str(row, "operationId")?,
                        uuid::Uuid::new_v4().to_string(),
                    );
                }
            }
            for m in &program.modules {
                map_id(&mut id_map, &m.id, uuid::Uuid::new_v4().to_string());
                for l in &m.lessons {
                    map_id(&mut id_map, &l.id, uuid::Uuid::new_v4().to_string());
                    for q in &l.questions {
                        map_id(&mut id_map, &q.id, uuid::Uuid::new_v4().to_string());
                    }
                }
            }
            for s in &program.sources {
                if !id_map.contains_key(&s.id) {
                    map_id(&mut id_map, &s.id, uuid::Uuid::new_v4().to_string());
                }
            }
            for attempt in &program.attempts {
                map_id(&mut id_map, &attempt.id, uuid::Uuid::new_v4().to_string());
            }
            if let Some(snapshot) = &evidence_snapshot {
                map_evidence_ids(snapshot, &mut id_map)?;
            }
            rewrite_program(&mut program, &id_map);
        } else {
            program.summary.id = target_id.clone();
        }
        // Replacing is one SQLite transaction: a failed insert rolls the old program back.
        let mut tx = self.pool.begin().await.map_err(db)?;
        sqlx::query("PRAGMA defer_foreign_keys=ON")
            .execute(&mut *tx)
            .await
            .map_err(db)?;
        let fresh: Option<String> = sqlx::query_scalar(
            "SELECT status FROM learning_pack_import_previews WHERE id=? AND status='pending'",
        )
        .bind(&req.preview_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(db)?;
        if fresh.is_none() {
            return Err(invalid("Pack preview changed while it was being applied."));
        }
        if policy == LearningPackConflictPolicy::ReplaceAfterBackup {
            sqlx::query("DELETE FROM study_decks WHERE id=(SELECT deck_id FROM learning_memory WHERE program_id=? AND deck_id IS NOT NULL)")
                .bind(&target_id).execute(&mut *tx).await.map_err(db)?;
            sqlx::query("DELETE FROM learning_programs WHERE id=?")
                .bind(&target_id)
                .execute(&mut *tx)
                .await
                .map_err(db)?;
        }
        insert_program(&mut tx, &program, &id_map).await?;
        if let Some(backup) = &backup_record {
            // The export row normally cascades with the target program. Reattach
            // its recovery metadata to the replacement row in this transaction.
            sqlx::query("INSERT INTO learning_pack_exports(id,program_id,operation_id,payload_hash,format_version,root_sha256,destination_path,privacy_manifest_json,entry_manifest_json,manifest_json,created_at) VALUES(?,?,?,?,?,?,?,?,?,?,?)")
                .bind(&backup.id).bind(&backup.program_id).bind(&backup.operation_id).bind(&backup.payload_hash).bind(backup.format_version).bind(&backup.root_sha256).bind(&backup.destination_path).bind(&backup.privacy_manifest_json).bind(&backup.entry_manifest_json).bind(&backup.manifest_json).bind(backup.created_at).execute(&mut *tx).await.map_err(db)?;
        }
        if let Some(bytes) = decoded.entries.get(LESSON_STATE_ENTRY) {
            let states: Vec<serde_json::Value> = parse(bytes)?;
            if states.len() > 2_000 {
                return Err(invalid("Pack has too many curriculum lesson states."));
            }
            for mut state in states {
                remap_json_field(&mut state, "lessonId", &id_map)?;
                remap_json_field(&mut state, "replacementLessonId", &id_map)?;
                let lesson = value_str(&state, "lessonId")?;
                let status = value_str(&state, "state")?;
                if !matches!(
                    status,
                    "outline" | "ready" | "completed" | "skipped" | "replaced" | "challenged"
                ) {
                    return Err(invalid("Pack lesson curriculum state is invalid."));
                }
                sqlx::query("UPDATE learning_lessons SET curriculum_state=?,replacement_lesson_id=? WHERE program_id=? AND id=?")
                    .bind(status).bind(state.get("replacementLessonId").filter(|v|!v.is_null()).and_then(serde_json::Value::as_str)).bind(&program.summary.id).bind(lesson).execute(&mut *tx).await.map_err(db)?;
            }
        }
        insert_outcomes(&mut tx, &program.summary.id, &decoded, &id_map).await?;
        if decoded.manifest.privacy.includes_learner_evidence {
            insert_evidence(&mut tx, &program.summary.id, &decoded, &id_map).await?;
        }
        if let Some(snapshot) = &mut practical_snapshot {
            for activity in &mut snapshot.activities {
                activity.id = remap(&id_map, &activity.id);
                activity.program_id = program.summary.id.clone();
                activity.lesson_id = remap(&id_map, &activity.lesson_id);
                activity.predecessor_id = activity
                    .predecessor_id
                    .as_deref()
                    .map(|id| remap(&id_map, id));
                remap_refs(&id_map, &mut activity.outcome_ids);
                remap_refs(&id_map, &mut activity.source_version_ids);
            }
            for run in &mut snapshot.runs {
                run.id = remap(&id_map, &run.id);
                run.program_id = program.summary.id.clone();
                run.activity_id = remap(&id_map, &run.activity_id);
                run.practice_session_id = if evidence_snapshot.is_some() {
                    run.practice_session_id
                        .as_deref()
                        .map(|id| remap(&id_map, id))
                } else {
                    None
                };
            }
            for raw in &mut snapshot.run_snapshots {
                remap_json_field(raw, "runId", &id_map)?;
                remap_json_field(raw, "operationId", &id_map)?;
                if policy == LearningPackConflictPolicy::CreateCopy {
                    remap_json_string_field(raw, "activitySnapshot", &id_map)?;
                }
            }
            for session in &mut snapshot.simulations {
                remap_json_field(session, "id", &id_map)?;
                remap_json_field(session, "activityId", &id_map)?;
                remap_json_field(session, "practiceSessionId", &id_map)?;
                remap_json_field(session, "operationId", &id_map)?;
                if policy == LearningPackConflictPolicy::CreateCopy {
                    remap_json_string_field(session, "activitySnapshot", &id_map)?;
                }
                if evidence_snapshot.is_none() {
                    session["practiceSessionId"] = serde_json::Value::Null;
                }
            }
            for turn in &mut snapshot.simulation_turns {
                remap_json_field(turn, "id", &id_map)?;
                remap_json_field(turn, "sessionId", &id_map)?;
                remap_json_field(turn, "operationId", &id_map)?;
                if policy == LearningPackConflictPolicy::CreateCopy {
                    remap_json_string_field(turn, "citations", &id_map)?;
                }
            }
            for operation in &mut snapshot.simulation_operations {
                remap_json_field(operation, "operationId", &id_map)?;
                remap_json_field(operation, "sessionId", &id_map)?;
                remap_json_field(operation, "resultId", &id_map)?;
            }
            for operation in &mut snapshot.practical_operations {
                remap_json_field(operation, "operationId", &id_map)?;
                remap_json_field(operation, "activityId", &id_map)?;
                remap_json_field(operation, "runId", &id_map)?;
            }
            insert_practical_snapshot(&mut tx, &program.summary.id, snapshot).await?;
        }
        if let Some(snapshot) = &mut canvas_snapshot {
            insert_canvas_snapshot(&mut tx, &program.summary.id, snapshot, &id_map).await?;
        }
        if let Some(workspace) = &source_workspace {
            insert_source_workspace(&mut tx, &program.summary.id, workspace, &decoded, &id_map)
                .await?;
        }
        if let Some(history) = &mut source_history {
            insert_source_history(&mut tx, &program.summary.id, history, &id_map).await?;
        }
        if let Some(bytes) = decoded.entries.get("sources/selectors.json") {
            let selectors: Vec<serde_json::Value> = parse(bytes)?;
            for selector in selectors {
                let old_source = selector
                    .get("sourceId")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| invalid("Pack selector has no source ID."))?;
                let old_version = selector
                    .get("sourceVersionId")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| invalid("Pack selector has no version ID."))?;
                let source_id = remap(&id_map, old_source);
                let version_id = remap(&id_map, old_version);
                let body_included = decoded.entries.contains_key(&source_body_path(old_version));
                let old_status = selector
                    .get("status")
                    .and_then(serde_json::Value::as_str)
                    .ok_or_else(|| invalid("Pack selector has an invalid status."))?;
                if !matches!(
                    old_status,
                    "exact" | "contextual" | "ambiguous" | "not_found"
                ) {
                    return Err(invalid("Pack selector has an invalid status."));
                }
                let status = if body_included {
                    old_status
                } else {
                    "not_found"
                };
                let exact = selector
                    .get("exactText")
                    .and_then(serde_json::Value::as_str)
                    .ok_or_else(|| invalid("Pack selector has no exact text."))?;
                let prefix = selector
                    .get("prefixText")
                    .and_then(serde_json::Value::as_str)
                    .ok_or_else(|| invalid("Pack selector has no prefix text."))?;
                let suffix = selector
                    .get("suffixText")
                    .and_then(serde_json::Value::as_str)
                    .ok_or_else(|| invalid("Pack selector has no suffix text."))?;
                let start = if body_included {
                    selector
                        .get("startByte")
                        .and_then(serde_json::Value::as_i64)
                } else {
                    None
                };
                let end = if body_included {
                    selector.get("endByte").and_then(serde_json::Value::as_i64)
                } else {
                    None
                };
                let candidate_count = if body_included {
                    selector
                        .get("candidateCount")
                        .and_then(serde_json::Value::as_i64)
                        .ok_or_else(|| invalid("Pack selector has an invalid candidate count."))?
                } else {
                    0
                };
                let created_at = selector
                    .get("createdAt")
                    .and_then(serde_json::Value::as_i64)
                    .ok_or_else(|| invalid("Pack selector has an invalid timestamp."))?;
                if candidate_count < 0
                    || (matches!(status, "exact" | "contextual") && candidate_count < 1)
                    || (status == "not_found" && candidate_count != 0)
                    || start.is_some_and(|v| v < 0)
                    || end.is_some_and(|v| v < 0)
                    || exact.chars().count() > 2000
                    || prefix.chars().count() > 500
                    || suffix.chars().count() > 500
                    || matches!(status, "exact" | "contextual")
                        && start.zip(end).is_none_or(|(s, e)| e <= s)
                {
                    return Err(invalid("Pack selector offsets are invalid."));
                }
                let selector_id = if policy == LearningPackConflictPolicy::CreateCopy {
                    uuid::Uuid::new_v4().to_string()
                } else {
                    selector
                        .get("id")
                        .and_then(serde_json::Value::as_str)
                        .ok_or_else(|| invalid("Pack selector has no ID."))?
                        .to_owned()
                };
                sqlx::query("INSERT INTO learning_source_selectors(id,program_id,source_id,source_version_id,exact_text,prefix_text,suffix_text,start_byte,end_byte,candidate_count,status,created_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?)")
                    .bind(selector_id).bind(&program.summary.id).bind(source_id).bind(version_id).bind(exact).bind(prefix).bind(suffix).bind(start).bind(end).bind(candidate_count).bind(status).bind(created_at).execute(&mut *tx).await.map_err(db)?;
            }
        }
        if let Some(snapshot) = &evidence_snapshot {
            insert_evidence_snapshot(&mut tx, &program.summary.id, snapshot, &id_map).await?;
        }
        {
            for key in imported_keys {
                let qid = id_map
                    .get(&key.question_id)
                    .cloned()
                    .unwrap_or(key.question_id);
                sqlx::query("INSERT INTO learning_answer_keys(question_id,correct_index,explanation) VALUES(?,?,?)").bind(qid).bind(key.correct_index as i64).bind(key.explanation).execute(&mut *tx).await.map_err(db)?;
            }
        }
        let result_id = uuid::Uuid::new_v4().to_string();
        let timestamp = now();
        let changes: Vec<LearningPackChangeDto> =
            parse(preview.get::<String, _>("changes_json").as_bytes())?;
        sqlx::query("INSERT INTO learning_pack_imports(id,preview_id,operation_id,payload_hash,imported_program_id,backup_id,applied_changes_json,imported_at) VALUES(?,?,?,?,?,?,?,?)")
            .bind(&result_id).bind(&req.preview_id).bind(&req.operation_id).bind(&payload).bind(&program.summary.id).bind(backup_record.as_ref().map(|x|x.id.as_str())).bind(json_string(&changes)?).bind(timestamp).execute(&mut *tx).await.map_err(db)?;
        sqlx::query(
            "UPDATE learning_pack_import_previews SET status='applied',decided_at=? WHERE id=?",
        )
        .bind(timestamp)
        .bind(&req.preview_id)
        .execute(&mut *tx)
        .await
        .map_err(db)?;
        sqlx::query("INSERT INTO learning_portability_operations(operation_id,program_id,kind,payload_hash,result_id,created_at) VALUES(?,?,'apply_import',?,?,?)").bind(&req.operation_id).bind(&program.summary.id).bind(&payload).bind(&result_id).bind(timestamp).execute(&mut *tx).await.map_err(db)?;
        tx.commit().await.map_err(db)?;
        self.workspace(&program.summary.id).await
    }
    async fn workspace_for_preview(&self, id: &str) -> Result<LearningPortabilityWorkspaceDto> {
        let pid: String =
            sqlx::query_scalar("SELECT program_id FROM learning_pack_import_previews WHERE id=?")
                .bind(id)
                .fetch_one(&self.pool)
                .await
                .map_err(db)?;
        self.workspace(&pid).await
    }
    async fn replay(&self, id: &str, kind: &str, payload: &str) -> Result<Option<String>> {
        let row=sqlx::query("SELECT kind,payload_hash,result_id FROM learning_portability_operations WHERE operation_id=?").bind(id).fetch_optional(&self.pool).await.map_err(db)?;
        if let Some(r) = row {
            if r.get::<String, _>("kind") != kind || r.get::<String, _>("payload_hash") != payload {
                return Err(invalid(
                    "Operation ID was reused with a different pack request.",
                ));
            }
            return Ok(r.get("result_id"));
        }
        Ok(None)
    }
}

fn validate_uuid(id: &str, label: &str) -> Result<()> {
    uuid::Uuid::parse_str(id)
        .map(|_| ())
        .map_err(|_| invalid(format!("Invalid {label} ID.")))
}
fn policy_str(v: LearningPackConflictPolicy) -> &'static str {
    match v {
        LearningPackConflictPolicy::CreateCopy => "create_copy",
        LearningPackConflictPolicy::MergeSafe => "merge_safe",
        LearningPackConflictPolicy::ReplaceAfterBackup => "replace_after_backup",
    }
}
fn validate_program(p: &LearningProgramDto) -> Result<()> {
    validate_uuid(&p.summary.id, "program")?;
    let mut ids = HashSet::new();
    ids.insert(p.summary.id.as_str());
    let mut sources = HashSet::new();
    for source in &p.sources {
        validate_source_id(&source.id)?;
        if !ids.insert(source.id.as_str()) {
            return Err(invalid("Pack has duplicate entity IDs."));
        }
        sources.insert(source.id.as_str());
    }
    let mut modules = HashSet::new();
    let mut lessons = HashSet::new();
    let mut questions = HashMap::<&str, usize>::new();
    for m in &p.modules {
        validate_uuid(&m.id, "module")?;
        if !ids.insert(m.id.as_str()) {
            return Err(invalid("Pack has duplicate module IDs."));
        }
        if m.prerequisite_module_ids
            .iter()
            .any(|id| !modules.contains(id.as_str()))
            || m.prerequisite_module_ids
                .iter()
                .collect::<HashSet<_>>()
                .len()
                != m.prerequisite_module_ids.len()
        {
            return Err(invalid(
                "Pack prerequisites must reference distinct earlier modules.",
            ));
        }
        if let Some(project) = &m.project {
            crate::features::learning::teaching::validate_project(project)?;
        }
        modules.insert(m.id.as_str());
        for l in &m.lessons {
            validate_uuid(&l.id, "lesson")?;
            if !ids.insert(l.id.as_str()) {
                return Err(invalid("Pack has duplicate entity IDs."));
            }
            lessons.insert(l.id.as_str());
            for block in &l.blocks {
                crate::features::learning::teaching::validate_saved_rubric(&block.rubric)?;
                if block
                    .source_ids
                    .iter()
                    .any(|id| !sources.contains(id.as_str()))
                {
                    return Err(invalid(
                        "Pack lesson block refers to an unknown source version.",
                    ));
                }
            }
            for q in &l.questions {
                validate_uuid(&q.id, "question")?;
                if !ids.insert(q.id.as_str()) {
                    return Err(invalid("Pack has duplicate entity IDs."));
                }
                if !(2..=8).contains(&q.options.len())
                    || q.options.iter().any(|o| o.trim().is_empty())
                    || q.options.iter().collect::<HashSet<_>>().len() != q.options.len()
                {
                    return Err(invalid("Pack question choices are invalid."));
                }
                if q.source_ids.iter().any(|id| !sources.contains(id.as_str())) {
                    return Err(invalid(
                        "Pack question refers to an unknown source version.",
                    ));
                }
                questions.insert(q.id.as_str(), q.options.len());
            }
        }
    }
    if p.summary
        .current_lesson_id
        .as_deref()
        .is_some_and(|id| !lessons.contains(id))
    {
        return Err(invalid(
            "Pack current lesson is missing from its curriculum.",
        ));
    }
    for attempt in &p.attempts {
        validate_uuid(&attempt.id, "attempt")?;
        if !ids.insert(attempt.id.as_str())
            || !modules.contains(attempt.module_id.as_str())
            || attempt
                .lesson_id
                .as_deref()
                .is_some_and(|id| !lessons.contains(id))
        {
            return Err(invalid(
                "Pack attempt has an invalid or duplicate curriculum reference.",
            ));
        }
        for r in &attempt.results {
            if !questions.contains_key(r.question_id.as_str())
                || r.correct_index >= r.options.len()
                || r.selected_index >= r.options.len()
                || r.source_ids.iter().any(|id| !sources.contains(id.as_str()))
            {
                return Err(invalid(
                    "Pack attempt evidence has an invalid reference or answer index.",
                ));
            }
        }
    }
    if p.modules.is_empty()
        || p.modules.len() > 100
        || p.modules.iter().map(|m| m.lessons.len()).sum::<usize>() > 1000
    {
        return Err(invalid("Pack curriculum exceeds import limits."));
    }
    Ok(())
}
fn validate_answer_keys(program: &LearningProgramDto, keys: &[PrivateAnswerKey]) -> Result<()> {
    let question_options: HashMap<&str, usize> = program
        .modules
        .iter()
        .flat_map(|m| m.lessons.iter())
        .flat_map(|l| l.questions.iter())
        .map(|q| (q.id.as_str(), q.options.len()))
        .collect();
    let mut seen = HashSet::new();
    for key in keys {
        let Some(count) = question_options.get(key.question_id.as_str()) else {
            return Err(invalid("Pack answer key refers to an unknown question."));
        };
        if !seen.insert(key.question_id.as_str())
            || key.correct_index >= *count
            || key.explanation.trim().is_empty()
        {
            return Err(invalid("Pack answer key data is invalid."));
        }
    }
    if seen.len() != question_options.len() {
        return Err(invalid("Pack omits one or more assessment answer keys."));
    }
    Ok(())
}
fn validate_source_id(id: &str) -> Result<()> {
    if id.trim().is_empty() || id.len() > 300 || id.chars().any(char::is_control) {
        return Err(invalid("Pack source version ID is invalid."));
    }
    Ok(())
}
fn source_body_path(id: &str) -> String {
    format!(
        "sources/bodies/{}.txt",
        Sha256::digest(id.as_bytes())
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>()
    )
}
fn enum_label<T: Serialize>(value: &T) -> Result<String> {
    serde_json::to_value(value)
        .map_err(|e| AppError::Serialization(e.to_string()))?
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| invalid("Pack contains an invalid enum value."))
}
fn value_str<'a>(v: &'a serde_json::Value, key: &str) -> Result<&'a str> {
    v.get(key)
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| invalid(format!("Pack record has an invalid {key}.")))
}
fn value_i64(v: &serde_json::Value, key: &str) -> Result<i64> {
    v.get(key)
        .and_then(serde_json::Value::as_i64)
        .ok_or_else(|| invalid(format!("Pack record has an invalid {key}.")))
}
fn value_option_str(v: &serde_json::Value, key: &str) -> Result<Option<String>> {
    match v.get(key) {
        Some(serde_json::Value::Null) | None => Ok(None),
        Some(value) => value
            .as_str()
            .map(|s| Some(s.to_owned()))
            .ok_or_else(|| invalid(format!("Pack record has an invalid {key}."))),
    }
}
async fn insert_outcomes(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    program_id: &str,
    pack: &DecodedLearningPack,
    map: &HashMap<String, String>,
) -> Result<()> {
    let Some(bytes) = pack.entries.get(OUTCOMES_ENTRY) else {
        return Ok(());
    };
    let rows: Vec<serde_json::Value> = parse(bytes)?;
    if rows.len() > 2000 {
        return Err(invalid("Pack has too many learning outcomes."));
    }
    for row in rows {
        let old_id = value_str(&row, "id")?;
        let id = remap(map, old_id);
        let module = value_option_str(&row, "moduleId")?.map(|v| remap(map, &v));
        let lesson = value_option_str(&row, "lessonId")?.map(|v| remap(map, &v));
        let title = value_str(&row, "title")?;
        let description = value_str(&row, "description")?;
        let ordinal = value_i64(&row, "ordinal")?;
        let created = value_i64(&row, "createdAt")?;
        if title.trim().is_empty() || title.len() > 500 || description.len() > 5000 || ordinal < 0 {
            return Err(invalid("Pack outcome metadata is invalid."));
        }
        sqlx::query("INSERT INTO learning_outcome_definitions(id,program_id,module_id,lesson_id,title,description,ordinal,created_at) VALUES(?,?,?,?,?,?,?,?)").bind(id).bind(program_id).bind(module).bind(lesson).bind(title).bind(description).bind(ordinal).bind(created).execute(&mut **tx).await.map_err(db)?;
    }
    Ok(())
}

async fn insert_canvas_snapshot(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    program_id: &str,
    snapshot: &mut CanvasPackSnapshot,
    map: &HashMap<String, String>,
) -> Result<()> {
    for row in &mut snapshot.canvases {
        remap_json_field(row, "id", map)?;
        remap_json_field(row, "lessonId", map)?;
        let scene = value_str(row, "sceneJson")?;
        let _: serde_json::Value = parse(scene.as_bytes())?;
        let count = value_i64(row, "elementCount")?;
        let revision = value_i64(row, "revision")?;
        if count < 0 || revision < 0 || scene.len() > 2_000_000 {
            return Err(invalid("Pack canvas data exceeds its bounds."));
        }
        sqlx::query("INSERT INTO learning_canvases(id,program_id,lesson_id,title,description,scene_json,element_count,revision,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?)")
            .bind(value_str(row,"id")?).bind(program_id).bind(row.get("lessonId").filter(|v|!v.is_null()).and_then(serde_json::Value::as_str)).bind(value_str(row,"title")?).bind(value_str(row,"description")?).bind(scene).bind(count).bind(revision).bind(value_i64(row,"createdAt")?).bind(value_i64(row,"updatedAt")?).execute(&mut **tx).await.map_err(db)?;
    }
    for row in &mut snapshot.snapshots {
        remap_json_field(row, "id", map)?;
        remap_json_field(row, "canvasId", map)?;
        let scene = value_str(row, "sceneJson")?;
        let _: serde_json::Value = parse(scene.as_bytes())?;
        sqlx::query("INSERT INTO learning_canvas_snapshots(id,program_id,canvas_id,name,title,description,scene_json,element_count,canvas_revision,created_at) VALUES(?,?,?,?,?,?,?,?,?,?)")
            .bind(value_str(row,"id")?).bind(program_id).bind(value_str(row,"canvasId")?).bind(value_str(row,"name")?).bind(value_str(row,"title")?).bind(value_str(row,"description")?).bind(scene).bind(value_i64(row,"elementCount")?).bind(value_i64(row,"canvasRevision")?).bind(value_i64(row,"createdAt")?).execute(&mut **tx).await.map_err(db)?;
    }
    for row in &mut snapshot.operations {
        remap_json_field(row, "operationId", map)?;
        remap_json_field(row, "canvasId", map)?;
        remap_json_field(row, "resultSnapshotId", map)?;
        let kind = value_str(row, "kind")?;
        if !matches!(kind, "create" | "save" | "snapshot" | "restore") {
            return Err(invalid("Pack canvas operation kind is invalid."));
        }
        sqlx::query("INSERT INTO learning_canvas_operations(operation_id,program_id,canvas_id,kind,payload_hash,result_revision,result_snapshot_id,created_at) VALUES(?,?,?,?,?,?,?,?)")
            .bind(value_str(row,"operationId")?).bind(program_id).bind(value_str(row,"canvasId")?).bind(kind).bind(value_str(row,"payloadHash")?).bind(value_i64(row,"resultRevision")?).bind(row.get("resultSnapshotId").filter(|v|!v.is_null()).and_then(serde_json::Value::as_str)).bind(value_i64(row,"createdAt")?).execute(&mut **tx).await.map_err(db)?;
    }
    Ok(())
}
async fn insert_evidence(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    program_id: &str,
    pack: &DecodedLearningPack,
    map: &HashMap<String, String>,
) -> Result<()> {
    if let Some(bytes) = pack.entries.get(EVIDENCE_ENTRY) {
        let rows: Vec<serde_json::Value> = parse(bytes)?;
        if rows.len() > 20_000 {
            return Err(invalid("Pack has too many evidence records."));
        }
        for row in rows {
            let id = remap(map, value_str(&row, "id")?);
            let outcome = value_option_str(&row, "outcomeId")?.map(|v| remap(map, &v));
            let source_kind = value_str(&row, "sourceKind")?;
            let source_id = remap(map, value_str(&row, "sourceId")?);
            let dimension = value_str(&row, "dimension")?;
            let result = value_str(&row, "result")?;
            let score = row
                .get("score")
                .filter(|v| !v.is_null())
                .and_then(serde_json::Value::as_f64);
            let observation = value_str(&row, "observation")?;
            let quote = value_option_str(&row, "evidenceQuote")?;
            let assistance = value_str(&row, "assistance")?;
            let observed = value_i64(&row, "observedAt")?;
            if !matches!(
                source_kind,
                "practice" | "assessment" | "recall" | "practical" | "simulation" | "manual"
            ) || !matches!(
                dimension,
                "recall" | "explanation" | "application" | "transfer"
            ) || !matches!(
                result,
                "observed" | "not_observed" | "uncertain" | "not_assessed"
            ) || score.is_some_and(|v| !(0.0..=1.0).contains(&v))
            {
                return Err(invalid("Pack evidence record is invalid."));
            }
            sqlx::query("INSERT INTO learning_evidence_events(id,program_id,outcome_id,source_kind,source_id,dimension,result,score,observation,evidence_quote,assistance_json,observed_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?)").bind(id).bind(program_id).bind(outcome).bind(source_kind).bind(source_id).bind(dimension).bind(result).bind(score).bind(observation).bind(quote).bind(assistance).bind(observed).execute(&mut **tx).await.map_err(db)?;
        }
    }
    if let Some(bytes) = pack.entries.get(FOLLOW_UP_ENTRY) {
        let rows: Vec<serde_json::Value> = parse(bytes)?;
        if rows.len() > 5000 {
            return Err(invalid("Pack has too many follow-up recommendations."));
        }
        for row in rows {
            let id = remap(map, value_str(&row, "id")?);
            let outcome = value_option_str(&row, "outcomeId")?.map(|v| remap(map, &v));
            let reason = value_str(&row, "reasonCode")?;
            let explanation = value_str(&row, "explanation")?;
            let kind = value_str(&row, "actionKind")?;
            let action = value_option_str(&row, "actionRef")?.map(|v| remap(map, &v));
            let status = value_str(&row, "status")?;
            let evidence_json = value_str(&row, "evidenceEventIds")?;
            let mut evidence: Vec<String> = parse(evidence_json.as_bytes())?;
            remap_refs(map, &mut evidence);
            let created = value_i64(&row, "createdAt")?;
            let decided = row
                .get("decidedAt")
                .filter(|v| !v.is_null())
                .and_then(serde_json::Value::as_i64);
            if !matches!(
                reason,
                "missed_outcome"
                    | "assisted_success"
                    | "low_transfer"
                    | "stale_evidence"
                    | "uncertain_grade"
            ) || !matches!(
                kind,
                "lesson" | "practice" | "assessment" | "recall" | "practical"
            ) || !matches!(status, "pending" | "accepted" | "dismissed" | "completed")
            {
                return Err(invalid("Pack follow-up record is invalid."));
            }
            sqlx::query("INSERT INTO learning_follow_up_recommendations(id,program_id,outcome_id,reason_code,explanation,action_kind,action_ref,status,evidence_event_ids_json,created_at,decided_at) VALUES(?,?,?,?,?,?,?,?,?,?,?)").bind(id).bind(program_id).bind(outcome).bind(reason).bind(explanation).bind(kind).bind(action).bind(status).bind(json_string(&evidence)?).bind(created).bind(decided).execute(&mut **tx).await.map_err(db)?;
        }
    }
    Ok(())
}

async fn insert_practical_snapshot(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    program_id: &str,
    snapshot: &PracticalPackSnapshot,
) -> Result<()> {
    use crate::features::learning::practical_dto::{
        LearningPracticalPublicFileRole, LearningPracticalRuntimeKind,
    };
    for activity in &snapshot.activities {
        validate_uuid(&activity.id, "practical activity")?;
        validate_uuid(&activity.lesson_id, "practical lesson")?;
        let profile_available = if let Some(profile) = activity.runtime_profile_id.as_deref() {
            sqlx::query_scalar::<_, i64>("SELECT 1 FROM learning_runtime_profiles WHERE id=?")
                .bind(profile)
                .fetch_optional(&mut **tx)
                .await
                .map_err(db)?
                .is_some()
        } else {
            false
        };
        let runtime = if profile_available
            && activity.runtime_kind == LearningPracticalRuntimeKind::Container
        {
            activity.runtime_kind
        } else {
            LearningPracticalRuntimeKind::None
        };
        let kind = enum_label(&activity.kind)?;
        let status = enum_label(&activity.status)?;
        let mode = enum_label(&activity.practice_mode)?;
        let runtime_name = enum_label(&runtime)?;
        let runtime_profile = if profile_available {
            activity.runtime_profile_id.as_deref()
        } else {
            None
        };
        let engine = if profile_available {
            activity
                .runtime_engine
                .as_ref()
                .map(enum_label)
                .transpose()?
        } else {
            None
        };
        let image = if profile_available {
            activity.runtime_image_id.as_deref()
        } else {
            None
        };
        let command = if profile_available {
            activity
                .runtime_command
                .as_ref()
                .map(json_string)
                .transpose()?
        } else {
            None
        };
        let limits = if profile_available {
            activity
                .runtime_limits
                .as_ref()
                .map(json_string)
                .transpose()?
        } else {
            None
        };
        sqlx::query("INSERT INTO learning_practical_activities(id,program_id,lesson_id,predecessor_id,kind,title,brief,status,practice_mode,allowed_aids_json,outcome_ids_json,source_version_ids_json,rubric_json,runtime_kind,runtime_profile_id,runtime_engine,runtime_image_id,runtime_command_json,runtime_limits_json,generator_model,revision,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)")
            .bind(&activity.id).bind(program_id).bind(&activity.lesson_id).bind(Option::<&str>::None).bind(kind).bind(&activity.title).bind(&activity.brief).bind(status).bind(mode).bind(json_string(&activity.allowed_aids)?).bind(json_string(&activity.outcome_ids)?).bind(json_string(&activity.source_version_ids)?).bind(json_string(&activity.rubric)?).bind(runtime_name).bind(runtime_profile).bind(engine).bind(image).bind(command).bind(limits).bind(&activity.generator_model).bind(activity.revision).bind(activity.created_at).bind(activity.updated_at).execute(&mut **tx).await.map_err(db)?;
        for (ordinal, file) in activity.files.iter().enumerate() {
            if !matches!(
                file.role,
                LearningPracticalPublicFileRole::Starter
                    | LearningPracticalPublicFileRole::Reference
            ) {
                return Err(invalid(
                    "A pack cannot import hidden practical evaluator files.",
                ));
            }
            if format!("{:x}", Sha256::digest(file.content.as_bytes())) != file.content_sha256 {
                return Err(invalid("Practical activity file checksum does not match."));
            }
            let role = enum_label(&file.role)?;
            sqlx::query("INSERT INTO learning_practical_files(activity_id,ordinal,path,role,content,content_sha256,editable) VALUES(?,?,?,?,?,?,?)").bind(&activity.id).bind(ordinal as i64).bind(&file.path).bind(role).bind(&file.content).bind(&file.content_sha256).bind(file.editable as i64).execute(&mut **tx).await.map_err(db)?;
        }
    }
    for activity in &snapshot.activities {
        if let Some(predecessor) = &activity.predecessor_id {
            sqlx::query("UPDATE learning_practical_activities SET predecessor_id=? WHERE program_id=? AND id=?")
                .bind(predecessor).bind(program_id).bind(&activity.id).execute(&mut **tx).await.map_err(db)?;
        }
    }
    for run in &snapshot.runs {
        validate_uuid(&run.id, "practical run")?;
        let _activity = snapshot
            .activities
            .iter()
            .find(|a| a.id == run.activity_id)
            .ok_or_else(|| invalid("Practical run references a missing activity."))?;
        let status = enum_label(&run.status)?;
        let engine = run.engine.as_ref().map(enum_label).transpose()?;
        let checks = json_string(&run.checks)?;
        let raw_snapshot = snapshot
            .run_snapshots
            .iter()
            .find(|raw| {
                raw.get("runId").and_then(serde_json::Value::as_str) == Some(run.id.as_str())
            })
            .ok_or_else(|| invalid("Pack omits a practical run's immutable activity snapshot."))?;
        let snapshot_json = value_str(raw_snapshot, "activitySnapshot")?.to_owned();
        let _: serde_json::Value = parse(snapshot_json.as_bytes())?;
        let files = json_string(&run.learner_files)?;
        let operation_id = value_str(raw_snapshot, "operationId")?;
        let payload_hash = value_str(raw_snapshot, "payloadHash")?;
        sqlx::query("INSERT INTO learning_practical_runs(id,program_id,activity_id,activity_revision,activity_snapshot_json,practice_session_id,operation_id,payload_hash,engine,image_id,status,learner_files_json,stdout,stderr,output_truncated,exit_code,duration_ms,check_results_json,created_at,completed_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)")
            .bind(&run.id).bind(program_id).bind(&run.activity_id).bind(run.activity_revision).bind(snapshot_json).bind(&run.practice_session_id).bind(operation_id).bind(payload_hash).bind(engine).bind(&run.image_id).bind(status).bind(files).bind(&run.stdout).bind(&run.stderr).bind(run.output_truncated as i64).bind(run.exit_code).bind(run.duration_ms).bind(checks).bind(run.created_at).bind(run.completed_at).execute(&mut **tx).await.map_err(db)?;
        for (ordinal, check) in run.checks.iter().enumerate() {
            let status = enum_label(&check.status)?;
            sqlx::query("INSERT INTO learning_practical_run_checks(run_id,ordinal,name,status,message,duration_ms) VALUES(?,?,?,?,?,?)").bind(&run.id).bind(ordinal as i64).bind(&check.name).bind(status).bind(&check.message).bind(check.duration_ms).execute(&mut **tx).await.map_err(db)?;
        }
    }
    for operation in &snapshot.practical_operations {
        let table = value_str(operation, "table")?;
        let operation_id = value_str(operation, "operationId")?;
        let kind = value_str(operation, "kind")?;
        if table == "learning_practical_activity_operations" {
            if !matches!(kind, "generate" | "revise" | "retire") {
                return Err(invalid("Practical activity operation kind is invalid."));
            }
            sqlx::query("INSERT INTO learning_practical_activity_operations(operation_id,program_id,activity_id,kind,payload_hash,result_revision,created_at) VALUES(?,?,?,?,?,?,?)")
                .bind(operation_id).bind(program_id).bind(value_str(operation,"activityId")?).bind(kind).bind(value_str(operation,"payloadHash")?).bind(value_i64(operation,"resultRevision")?).bind(value_i64(operation,"createdAt")?).execute(&mut **tx).await.map_err(db)?;
        } else if table == "learning_practical_run_operations" {
            if !matches!(kind, "start" | "cancel") {
                return Err(invalid("Practical run operation kind is invalid."));
            }
            let run = value_str(operation, "runId")?;
            sqlx::query("INSERT INTO learning_practical_run_operations(operation_id,program_id,run_id,kind,payload_hash,created_at) VALUES(?,?,?,?,?,?)")
                .bind(operation_id).bind(program_id).bind(run).bind(kind).bind(value_str(operation,"payloadHash")?).bind(value_i64(operation,"createdAt")?).execute(&mut **tx).await.map_err(db)?;
        } else {
            return Err(invalid(
                "Pack contains an unknown practical operation table.",
            ));
        }
    }
    for session in &snapshot.simulations {
        let id = value_str(session, "id")?;
        let activity = value_str(session, "activityId")?;
        let revision = value_i64(session, "activityRevision")?;
        let snapshot_json = value_str(session, "activitySnapshot")?;
        let operation = value_str(session, "operationId")?;
        let payload = value_str(session, "payloadHash")?;
        let status = value_str(session, "status")?;
        if !matches!(status, "active" | "submitted" | "cancelled") {
            return Err(invalid("Pack simulation status is invalid."));
        }
        sqlx::query("INSERT INTO learning_simulation_sessions(id,program_id,activity_id,activity_revision,activity_snapshot_json,practice_session_id,operation_id,payload_hash,learner_role,counterpart_role,status,revision,created_at,updated_at,submitted_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)")
            .bind(id).bind(program_id).bind(activity).bind(revision).bind(snapshot_json).bind(session.get("practiceSessionId").filter(|v|!v.is_null()).and_then(serde_json::Value::as_str)).bind(operation).bind(payload).bind(value_str(session,"learnerRole")?).bind(value_str(session,"counterpartRole")?).bind(status).bind(value_i64(session,"revision")?).bind(value_i64(session,"createdAt")?).bind(value_i64(session,"updatedAt")?).bind(session.get("submittedAt").filter(|v|!v.is_null()).and_then(serde_json::Value::as_i64)).execute(&mut **tx).await.map_err(db)?;
    }
    for turn in &snapshot.simulation_turns {
        let speaker = value_str(turn, "speaker")?;
        if !matches!(speaker, "learner" | "counterpart" | "coach") {
            return Err(invalid("Pack simulation turn speaker is invalid."));
        }
        let citations = value_str(turn, "citations")?;
        let _: serde_json::Value = parse(citations.as_bytes())?;
        sqlx::query("INSERT INTO learning_simulation_turns(id,program_id,session_id,operation_id,payload_hash,ordinal,speaker,content,citations_json,model_name,created_at) VALUES(?,?,?,?,?,?,?,?,?,?,?)")
            .bind(value_str(turn,"id")?).bind(program_id).bind(value_str(turn,"sessionId")?).bind(value_str(turn,"operationId")?).bind(value_str(turn,"payloadHash")?).bind(value_i64(turn,"ordinal")?).bind(speaker).bind(value_str(turn,"content")?).bind(citations).bind(turn.get("modelName").and_then(serde_json::Value::as_str)).bind(value_i64(turn,"createdAt")?).execute(&mut **tx).await.map_err(db)?;
    }
    for operation in &snapshot.simulation_operations {
        let kind = value_str(operation, "kind")?;
        if !matches!(kind, "start" | "turn" | "finish" | "cancel") {
            return Err(invalid("Simulation operation kind is invalid."));
        }
        sqlx::query("INSERT INTO learning_simulation_operations(operation_id,program_id,session_id,kind,payload_hash,result_id,created_at) VALUES(?,?,?,?,?,?,?)")
            .bind(value_str(operation,"operationId")?).bind(program_id).bind(value_str(operation,"sessionId")?).bind(kind).bind(value_str(operation,"payloadHash")?).bind(operation.get("resultId").filter(|v|!v.is_null()).and_then(serde_json::Value::as_str)).bind(value_i64(operation,"createdAt")?).execute(&mut **tx).await.map_err(db)?;
    }
    Ok(())
}

async fn insert_source_workspace(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    program_id: &str,
    workspace: &crate::features::learning::dto::LearningSourceWorkspaceDto,
    pack: &DecodedLearningPack,
    map: &HashMap<String, String>,
) -> Result<()> {
    for source in &workspace.sources {
        let source_id = remap(map, &source.id);
        let kind = match source.kind {
            crate::features::learning::dto::LearningSourceKind::Web => "web",
            crate::features::learning::dto::LearningSourceKind::Document => "document",
            crate::features::learning::dto::LearningSourceKind::Pasted => "pasted",
        };
        let policy = match source.freshness_policy {
            crate::features::learning::dto::LearningSourcePolicy::Fixed => "fixed",
            crate::features::learning::dto::LearningSourcePolicy::Manual => "manual",
            crate::features::learning::dto::LearningSourcePolicy::BeforeUse => "before_use",
        };
        sqlx::query("INSERT INTO learning_source_library(id,program_id,kind,origin,requested_url,freshness_policy,active_version_id,pending_version_id,revision,deleted_at,deletion_reason,created_at,updated_at) VALUES(?,?,?,?,?,?,NULL,NULL,?,?,?,?,?)")
            .bind(&source_id).bind(program_id).bind(kind).bind(&source.origin).bind(&source.requested_url).bind(policy).bind(source.revision).bind(source.deleted_at).bind(&source.deletion_reason).bind(source.created_at).bind(source.updated_at).execute(&mut **tx).await.map_err(db)?;
        let mut restored_digests = HashSet::new();
        for version in &source.versions {
            validate_source_id(&version.id)?;
            let version_id = remap(map, &version.id);
            let body = pack.entries.get(&source_body_path(&version.id));
            let full_text = match body {
                Some(bytes) => std::str::from_utf8(bytes)
                    .map_err(|e| AppError::InvalidData(e.to_string()))?
                    .to_owned(),
                None => version.excerpt.clone(),
            };
            if full_text.len() > 1_000_000 {
                return Err(invalid(
                    "A source version in the pack exceeds the restore size limit.",
                ));
            }
            let content_digest = format!("{:x}", Sha256::digest(full_text.as_bytes()));
            if body.is_none() && !restored_digests.insert(content_digest.clone()) {
                return Err(invalid("Two source versions have the same available excerpt. Include licensed full source bodies to preserve both immutable versions."));
            }
            if body.is_some() && version.content_sha256 != content_digest {
                return Err(invalid(
                    "A source body does not match its declared content digest.",
                ));
            }
            let restored_word_count = full_text.split_whitespace().count() as i64;
            let extraction = if body.is_some() {
                version.extraction_version.clone()
            } else {
                "pack_excerpt".into()
            };
            let truncated = body.is_none() || version.truncated;
            sqlx::query("INSERT INTO learning_source_versions(id,program_id,source_id,version_number,title,publisher,requested_url,resolved_url,full_text,excerpt,content_sha256,word_count,truncated,extraction_version,acquired_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)")
                .bind(&version_id).bind(program_id).bind(&source_id).bind(version.version_number).bind(&version.title).bind(&version.publisher).bind(&source.requested_url).bind(&version.resolved_url).bind(&full_text).bind(&version.excerpt).bind(if body.is_some(){version.content_sha256.clone()}else{content_digest}).bind(restored_word_count).bind(truncated as i64).bind(extraction).bind(version.acquired_at).execute(&mut **tx).await.map_err(db)?;
        }
        let active = source.active_version_id.as_deref().map(|id| remap(map, id));
        let pending = source
            .pending_version_id
            .as_deref()
            .map(|id| remap(map, id));
        sqlx::query("UPDATE learning_source_library SET active_version_id=?,pending_version_id=? WHERE id=? AND program_id=?")
            .bind(active).bind(pending).bind(&source_id).bind(program_id).execute(&mut **tx).await.map_err(db)?;
    }
    Ok(())
}

async fn insert_source_history(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    program_id: &str,
    history: &mut SourceHistorySnapshot,
    map: &HashMap<String, String>,
) -> Result<()> {
    for operation in &mut history.operations {
        remap_json_field(operation, "operationId", map)?;
        remap_json_field(operation, "sourceId", map)?;
        remap_json_field(operation, "resultVersionId", map)?;
        let kind = value_str(operation, "kind")?;
        if !matches!(
            kind,
            "add_web" | "add_document" | "add_text" | "refresh" | "adopt" | "policy"
        ) {
            return Err(invalid("Pack source operation kind is invalid."));
        }
        sqlx::query("INSERT INTO learning_source_operations(operation_id,program_id,source_id,kind,payload_hash,result_version_id,result_revision,created_at) VALUES(?,?,?,?,?,?,?,?)")
            .bind(value_str(operation,"operationId")?).bind(program_id).bind(value_str(operation,"sourceId")?).bind(kind).bind(value_str(operation,"payloadHash")?).bind(operation.get("resultVersionId").filter(|v|!v.is_null()).and_then(serde_json::Value::as_str)).bind(value_i64(operation,"resultRevision")?).bind(value_i64(operation,"createdAt")?).execute(&mut **tx).await.map_err(db)?;
    }
    for check in &mut history.checks {
        remap_json_field(check, "operationId", map)?;
        remap_json_field(check, "sourceId", map)?;
        remap_json_field(check, "pendingVersionId", map)?;
        let status = value_str(check, "status")?;
        if !matches!(status, "unchanged" | "update_available" | "failed") {
            return Err(invalid("Pack source check status is invalid."));
        }
        sqlx::query("INSERT INTO learning_source_refresh_checks(operation_id,program_id,source_id,status,checked_at,active_digest,pending_version_id,message) VALUES(?,?,?,?,?,?,?,?)")
            .bind(value_str(check,"operationId")?).bind(program_id).bind(value_str(check,"sourceId")?).bind(status).bind(value_i64(check,"checkedAt")?).bind(check.get("activeDigest").filter(|v|!v.is_null()).and_then(serde_json::Value::as_str)).bind(check.get("pendingVersionId").filter(|v|!v.is_null()).and_then(serde_json::Value::as_str)).bind(check.get("message").filter(|v|!v.is_null()).and_then(serde_json::Value::as_str)).execute(&mut **tx).await.map_err(db)?;
    }
    Ok(())
}

fn map_id(map: &mut HashMap<String, String>, from: &str, to: String) {
    // Do not let a later entity silently change the destination of an earlier
    // mapping. Duplicate namespaces are rejected by the aggregate/core
    // validators; deliberate repeated references reuse the first mapping.
    map.entry(from.to_owned()).or_insert(to);
}
fn remap(map: &HashMap<String, String>, value: &str) -> String {
    map.get(value).cloned().unwrap_or_else(|| value.to_owned())
}
fn remap_refs(map: &HashMap<String, String>, values: &mut Vec<String>) {
    for value in values {
        *value = remap(map, value);
    }
}
fn remap_json_field(
    value: &mut serde_json::Value,
    key: &str,
    map: &HashMap<String, String>,
) -> Result<()> {
    if let Some(old) = value
        .get(key)
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned)
    {
        value[key] = serde_json::Value::String(remap(map, &old));
    }
    Ok(())
}
fn remap_json_string_field(
    value: &mut serde_json::Value,
    key: &str,
    map: &HashMap<String, String>,
) -> Result<()> {
    let Some(serialized) = value
        .get(key)
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned)
    else {
        return Ok(());
    };
    let mut nested: serde_json::Value = parse(serialized.as_bytes())?;
    remap_nested_value(&mut nested, map);
    value[key] = serde_json::Value::String(json_string(&nested)?);
    Ok(())
}
fn remap_nested_value(value: &mut serde_json::Value, map: &HashMap<String, String>) {
    match value {
        serde_json::Value::String(_) => {}
        serde_json::Value::Array(values) => {
            values
                .iter_mut()
                .for_each(|child| remap_nested_value(child, map));
        }
        serde_json::Value::Object(values) => {
            for (key, child) in values.iter_mut() {
                if matches!(
                    key.as_str(),
                    "id" | "programId"
                        | "moduleId"
                        | "lessonId"
                        | "questionId"
                        | "sourceId"
                        | "versionId"
                        | "sourceVersionId"
                        | "outcomeId"
                        | "activityId"
                        | "runId"
                        | "sessionId"
                        | "revisesSessionId"
                        | "cardId"
                        | "operationId"
                        | "resultId"
                        | "snapshotId"
                        | "canvasId"
                        | "deckId"
                        | "reviewId"
                        | "blueprintId"
                        | "candidateId"
                        | "formId"
                        | "revisionId"
                        | "jobId"
                        | "diagnosticId"
                        | "practiceSessionId"
                        | "tutorTurnId"
                        | "proposalId"
                        | "evidenceEventId"
                        | "predecessorId"
                        | "replacementLessonId"
                        | "retryOfJobId"
                        | "acceptedCardId"
                        | "possibleDuplicateCardId"
                        | "sourceIds"
                        | "sourceVersionIds"
                        | "outcomeIds"
                        | "prerequisiteModuleIds"
                        | "evidenceEventIds"
                ) {
                    remap_scalar_or_id_array(child, map);
                } else {
                    remap_nested_value(child, map);
                }
            }
        }
        _ => {}
    }
}
fn remap_scalar_or_id_array(value: &mut serde_json::Value, map: &HashMap<String, String>) {
    match value {
        serde_json::Value::String(old) => {
            if let Some(new) = map.get(old) {
                *old = new.clone()
            }
        }
        serde_json::Value::Array(values) => {
            for child in values {
                if let serde_json::Value::String(old) = child {
                    if let Some(new) = map.get(old) {
                        *old = new.clone();
                    }
                }
            }
        }
        _ => {}
    }
}
fn remap_id_array(value: &mut serde_json::Value, map: &HashMap<String, String>) -> Result<()> {
    let values = value
        .as_array_mut()
        .ok_or_else(|| invalid("Pack ID list must be a JSON array."))?;
    for child in values {
        let old = child
            .as_str()
            .ok_or_else(|| invalid("Pack ID list contains a non-text value."))?;
        *child = serde_json::Value::String(remap(map, old));
    }
    Ok(())
}
fn rewrite_program(program: &mut LearningProgramDto, map: &HashMap<String, String>) {
    program.summary.id = remap(map, &program.summary.id);
    program.summary.current_lesson_id = program
        .summary
        .current_lesson_id
        .as_deref()
        .map(|id| remap(map, id));
    for module in &mut program.modules {
        module.id = remap(map, &module.id);
        remap_refs(map, &mut module.prerequisite_module_ids);
        for lesson in &mut module.lessons {
            lesson.id = remap(map, &lesson.id);
            for block in &mut lesson.blocks {
                remap_refs(map, &mut block.source_ids);
            }
            for question in &mut lesson.questions {
                question.id = remap(map, &question.id);
                remap_refs(map, &mut question.source_ids);
            }
        }
    }
    for source in &mut program.sources {
        source.id = remap(map, &source.id);
    }
    for attempt in &mut program.attempts {
        attempt.id = remap(map, &attempt.id);
        attempt.module_id = remap(map, &attempt.module_id);
        attempt.lesson_id = attempt.lesson_id.as_deref().map(|id| remap(map, id));
        for result in &mut attempt.results {
            result.question_id = remap(map, &result.question_id);
            remap_refs(map, &mut result.source_ids);
        }
    }
}
fn parse_policy(value: &str) -> Result<LearningPackConflictPolicy> {
    match value {
        "create_copy" => Ok(LearningPackConflictPolicy::CreateCopy),
        "merge_safe" => Ok(LearningPackConflictPolicy::MergeSafe),
        "replace_after_backup" => Ok(LearningPackConflictPolicy::ReplaceAfterBackup),
        _ => Err(AppError::Database("Invalid pack conflict policy".into())),
    }
}
async fn insert_program(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    program: &LearningProgramDto,
    map: &HashMap<String, String>,
) -> Result<()> {
    let summary = &program.summary;
    let status = match summary.status {
        LearningProgramStatus::Draft => "draft",
        LearningProgramStatus::Active => "active",
    };
    sqlx::query("INSERT INTO learning_programs(id,title,goal,status,revision,prior_knowledge,minutes_per_session,model_name,current_lesson_id,created_at) VALUES(?,?,?,?,?,?,?,?,?,?)")
        .bind(&summary.id).bind(&summary.title).bind(&summary.goal).bind(status).bind(summary.revision).bind(&program.prior_knowledge).bind(program.minutes_per_session).bind(&program.model_name).bind(&summary.current_lesson_id).bind(summary.created_at).execute(&mut **tx).await.map_err(db)?;
    for (mi, module) in program.modules.iter().enumerate() {
        sqlx::query("INSERT INTO learning_modules(id,program_id,ordinal,title,summary,outcomes_json,prerequisite_ids_json,project_json) VALUES(?,?,?,?,?,?,?,?)").bind(&module.id).bind(&summary.id).bind(mi as i64).bind(&module.title).bind(&module.summary).bind(json_string(&module.outcomes)?).bind(json_string(&module.prerequisite_module_ids)?).bind(json_string(&module.project)?).execute(&mut **tx).await.map_err(db)?;
        for (li, lesson) in module.lessons.iter().enumerate() {
            sqlx::query("INSERT INTO learning_lessons(id,program_id,module_id,ordinal,title,objective,estimated_minutes,preparation,completed) VALUES(?,?,?,?,?,?,?,?,?)")
                .bind(&lesson.id).bind(&summary.id).bind(&module.id).bind(li as i64).bind(&lesson.title).bind(&lesson.objective).bind(lesson.estimated_minutes).bind(if lesson.preparation==LearningPreparation::Ready{"ready"}else{"outline"}).bind(lesson.completed as i64).execute(&mut **tx).await.map_err(db)?;
            for (bi, block) in lesson.blocks.iter().enumerate() {
                let mut sources = block.source_ids.clone();
                remap_refs(map, &mut sources);
                sqlx::query("INSERT INTO learning_blocks(lesson_id,ordinal,kind,title,body,source_ids_json,rubric_json) VALUES(?,?,?,?,?,?,?)").bind(&lesson.id).bind(bi as i64).bind(crate::features::learning::repository::block_kind(block.kind.clone())).bind(&block.title).bind(&block.body).bind(json_string(&sources)?).bind(json_string(&block.rubric)?).execute(&mut **tx).await.map_err(db)?;
            }
            for (qi, question) in lesson.questions.iter().enumerate() {
                let mut sources = question.source_ids.clone();
                remap_refs(map, &mut sources);
                sqlx::query("INSERT INTO learning_questions(id,lesson_id,module_id,kind,prompt,options_json,source_ids_json,ordinal) VALUES(?,?,?,?,?,?,?,?)").bind(&question.id).bind(&lesson.id).bind(&module.id).bind(crate::features::learning::repository::assessment(question.kind.clone())).bind(&question.prompt).bind(json_string(&question.options)?).bind(json_string(&sources)?).bind(qi as i64).execute(&mut **tx).await.map_err(db)?;
            }
        }
    }
    for source in &program.sources {
        sqlx::query("INSERT INTO learning_sources(id,program_id,title,url,excerpt,acquired_at) VALUES(?,?,?,?,?,?)").bind(&source.id).bind(&summary.id).bind(&source.title).bind(&source.url).bind(&source.excerpt).bind(source.acquired_at).execute(&mut **tx).await.map_err(db)?;
    }
    for attempt in &program.attempts {
        // Evidence is carried only when explicitly selected at export. IDs and
        // citations are remapped with the rest of the imported aggregate.
        let mut result_rows = attempt.results.clone();
        for result in &mut result_rows {
            result.question_id = remap(map, &result.question_id);
            remap_refs(map, &mut result.source_ids);
        }
        let answers: Vec<crate::features::learning::dto::LearningAnswerDto> = result_rows
            .iter()
            .map(|r| crate::features::learning::dto::LearningAnswerDto {
                question_id: r.question_id.clone(),
                selected_index: r.selected_index,
            })
            .collect();
        let correct = result_rows
            .iter()
            .filter(|r| r.selected_index == r.correct_index)
            .count() as i64;
        sqlx::query("INSERT INTO learning_attempts(id,program_id,module_id,lesson_id,kind,correct,total,answers_json,results_json,submitted_at,payload_hash) VALUES(?,?,?,?,?,?,?,?,?,?,?)")
            .bind(&attempt.id).bind(&summary.id).bind(&attempt.module_id).bind(&attempt.lesson_id).bind(crate::features::learning::repository::assessment(attempt.kind.clone())).bind(correct).bind(result_rows.len() as i64).bind(json_string(&answers)?).bind(json_string(&result_rows)?).bind(attempt.submitted_at).bind("imported").execute(&mut **tx).await.map_err(db)?;
    }
    Ok(())
}
