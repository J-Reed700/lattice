//! The program-scoped evidence tables a pack carries, and the only place their
//! per-table rules live: which tables, how each is filtered to one program,
//! how many rows one may hold, and how rows are read and written.
use super::{
    db, invalid, json_string,
    remap::{remap_id_array, remap_nested_value},
};
use lattice_core::shared::error::{AppError, Result};
use serde::{Deserialize, Serialize};
use sqlx::{Column, Row, SqliteConnection, TypeInfo};
use std::collections::HashMap;

/// The most rows one evidence table may carry. Import refuses more, so
/// export does too: a pack no computer can import is no backup.
pub(super) const MAX_TABLE_ROWS: usize = 50_000;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct AggregateTableSnapshot {
    pub(super) table: String,
    pub(super) columns: Vec<String>,
    pub(super) rows: Vec<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct LearningEvidenceSnapshot {
    pub(super) tables: Vec<AggregateTableSnapshot>,
}

#[derive(Clone, Copy)]
pub(super) struct AggregateTableSpec {
    pub(super) table: &'static str,
    pub(super) filter: &'static str,
}

// Only these explicit program-scoped tables can be included in a pack. Quiz
// banks are course content and travel in their own section (bank_entries);
// the forms, answers and grades taken from them are evidence here. The
// SQL identifiers and joins are constants, never read from archive input.
// Lesson jobs are runtime work, not learning evidence: a pack carries their
// published lessons and curriculum revisions, and importing a queued job would
// start it in the receiving app.
pub(super) const EVIDENCE_TABLES: &[AggregateTableSpec] = &[
    AggregateTableSpec { table: "learning_assessment_forms", filter: "program_id=?" },
    AggregateTableSpec { table: "learning_assessment_form_items", filter: "form_id IN (SELECT id FROM learning_assessment_forms WHERE program_id=?)" },
    AggregateTableSpec { table: "learning_assessment_form_item_outcomes", filter: "form_id IN (SELECT id FROM learning_assessment_forms WHERE program_id=?)" },
    AggregateTableSpec { table: "learning_assessment_response_revisions", filter: "form_id IN (SELECT id FROM learning_assessment_forms WHERE program_id=?)" },
    AggregateTableSpec { table: "learning_assessment_submissions", filter: "program_id=?" },
    AggregateTableSpec { table: "learning_assessment_item_results", filter: "form_id IN (SELECT form_id FROM learning_assessment_submissions WHERE program_id=?)" },
    AggregateTableSpec { table: "learning_assessment_grade_contests", filter: "program_id=?" },
    AggregateTableSpec { table: "learning_assessment_item_reports", filter: "program_id=?" },
    AggregateTableSpec { table: "learning_curriculum_revisions", filter: "program_id=?" },
    AggregateTableSpec { table: "learning_curriculum_changes", filter: "revision_id IN (SELECT id FROM learning_curriculum_revisions WHERE program_id=?)" },
    AggregateTableSpec { table: "learning_diagnostic_attempts", filter: "program_id=?" },
    AggregateTableSpec { table: "learning_practice_sessions", filter: "program_id=?" },
    AggregateTableSpec { table: "learning_practice_artifact_revisions", filter: "program_id=?" },
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
    AggregateTableSpec { table: "learning_operations", filter: "program_id=? AND scope<>'portability'" },
];

/// Columns holding a JSON list of identifiers.
const ID_LIST_COLUMNS: &[&str] = &[
    "source_version_ids_json",
    "outcome_ids_json",
    "source_ids_json",
    "evidence_event_ids_json",
    "proposal_ids_json",
];

/// Columns holding JSON whose identifiers sit under known keys.
const NESTED_ID_COLUMNS: &[&str] = &[
    "source_json",
    "citations_json",
    "snapshot_json",
    "before_json",
    "after_json",
    "activity_snapshot_json",
    "request_json",
    "result_json",
    "prompt_json",
    "response_json",
    "findings_json",
    "source_coverage_gaps_json",
    "evaluation_json",
    "assistance_json",
    "details_json",
    "revealed_solution_citations_json",
    "rationale_json",
    "first_results_json",
    "regrade_results_json",
];

pub(super) async fn snapshot_evidence_tables(
    connection: &mut sqlx::SqliteConnection,
    program_id: &str,
) -> Result<LearningEvidenceSnapshot> {
    let mut tables = Vec::with_capacity(EVIDENCE_TABLES.len());
    for spec in EVIDENCE_TABLES {
        tables.push(snapshot_table(connection, spec, program_id).await?);
    }
    Ok(LearningEvidenceSnapshot { tables })
}

/// One table's rows for one program, each a JSON object keyed by column.
pub(super) async fn snapshot_table(
    connection: &mut SqliteConnection,
    spec: &AggregateTableSpec,
    program_id: &str,
) -> Result<AggregateTableSnapshot> {
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
    Ok(AggregateTableSnapshot {
        table: spec.table.into(),
        columns,
        rows: values,
    })
}

/// The rows of `table`, refused unless it is `spec`'s table with this
/// database's columns in order, within the row cap, every row holding
/// exactly those columns.
pub(super) async fn checked_rows(
    tx: &mut SqliteConnection,
    spec: &AggregateTableSpec,
    table: &AggregateTableSnapshot,
) -> Result<Vec<serde_json::Map<String, serde_json::Value>>> {
    if table.table != spec.table {
        return Err(invalid(
            "Pack aggregate contains an unknown or reordered table.",
        ));
    }
    let pragma = format!("PRAGMA table_info({})", spec.table);
    let cols = sqlx::query(&pragma).fetch_all(&mut *tx).await.map_err(db)?;
    let expected: Vec<String> = cols
        .into_iter()
        .map(|row| row.get::<String, _>("name"))
        .collect();
    if expected != table.columns {
        return Err(invalid(
            "Pack aggregate columns do not match this application version.",
        ));
    }
    if table.rows.len() > MAX_TABLE_ROWS {
        return Err(invalid("Pack aggregate has too many records."));
    }
    table
        .rows
        .iter()
        .map(|row| {
            let object = row
                .as_object()
                .ok_or_else(|| invalid("Pack aggregate row is malformed."))?;
            if object.len() != table.columns.len()
                || table
                    .columns
                    .iter()
                    .any(|column| !object.contains_key(column))
            {
                return Err(invalid("Pack aggregate row has a different schema."));
            }
            Ok(object.clone())
        })
        .collect()
}

/// Write one row with `verb`, its values bound in `columns` order.
pub(super) async fn insert_row(
    tx: &mut SqliteConnection,
    verb: &str,
    table: &str,
    columns: &[String],
    object: &serde_json::Map<String, serde_json::Value>,
) -> Result<()> {
    let col_sql = columns
        .iter()
        .map(|col| format!("\"{col}\""))
        .collect::<Vec<_>>()
        .join(",");
    let mut query = sqlx::QueryBuilder::<sqlx::Sqlite>::new(format!(
        "{verb} INTO {table} ({col_sql}) VALUES ("
    ));
    let mut values = query.separated(",");
    for column in columns {
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
    query.build().execute(&mut *tx).await.map_err(db)?;
    Ok(())
}

pub(super) async fn insert_evidence_snapshot(
    tx: &mut SqliteConnection,
    program_id: &str,
    snapshot: &LearningEvidenceSnapshot,
    map: &HashMap<String, String>,
) -> Result<()> {
    if snapshot.tables.len() != EVIDENCE_TABLES.len() {
        return Err(invalid("Pack aggregate table set is incomplete."));
    }
    for (table, spec) in snapshot.tables.iter().zip(EVIDENCE_TABLES) {
        for mut object in checked_rows(tx, spec, table).await? {
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
                            let ids = ID_LIST_COLUMNS.contains(&column.as_str());
                            if ids {
                                remap_id_array(&mut nested, map)?;
                            } else if NESTED_ID_COLUMNS.contains(&column.as_str()) {
                                remap_nested_value(&mut nested, map);
                            }
                            if ids || NESTED_ID_COLUMNS.contains(&column.as_str()) {
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
            insert_row(tx, &verb, spec.table, &table.columns, &object).await?;
        }
    }
    Ok(())
}
