//! The course's quiz banks, one pack entry per table: blueprints with their
//! slots, bank items with their outcomes, private keys and option data, and
//! the course's quiz settings. A question bank is course content, so it
//! travels whatever the learner-evidence choice; the forms taken from it,
//! their answers, results, contests and reports are learner evidence and
//! travel with the evidence tables.
//! Design: docs/design/2026-10-23-quizzes-and-tests.md §6.5.
use super::{
    conflicts::IdClaim,
    entries::{decode, fresh, ExportScope, ImportScope, SectionRows},
    evidence_tables::{
        checked_rows, insert_row, snapshot_table, AggregateTableSnapshot, AggregateTableSpec,
        MAX_TABLE_ROWS,
    },
    invalid, json, parse,
    review_entries::rewrite,
};
use crate::features::learning::{
    dto::LearningProgramDto,
    pack::{
        sections::{PackSection, BANK_TABLE_PREFIX, OUTCOMES_ENTRY},
        DecodedLearningPack, LearningPackEntry, LearningPackEntryKind,
    },
};
use async_trait::async_trait;
use lattice_core::shared::error::Result;
use serde_json::Value;
use sqlx::SqliteConnection;
use std::collections::{HashMap, HashSet};

/// In insert order; each filter binds the course once.
pub(super) const BANK_TABLES: &[AggregateTableSpec] = &[
    AggregateTableSpec {
        table: "learning_quiz_settings",
        filter: "program_id=?",
    },
    AggregateTableSpec {
        table: "learning_assessment_blueprints",
        filter: "program_id=?",
    },
    AggregateTableSpec {
        table: "learning_assessment_blueprint_slots",
        filter:
            "blueprint_id IN (SELECT id FROM learning_assessment_blueprints WHERE program_id=?)",
    },
    AggregateTableSpec {
        table: "learning_assessment_candidates",
        filter: "program_id=?",
    },
    AggregateTableSpec {
        table: "learning_assessment_candidate_outcomes",
        filter:
            "candidate_id IN (SELECT id FROM learning_assessment_candidates WHERE program_id=?)",
    },
    AggregateTableSpec {
        table: "learning_assessment_answer_keys",
        filter:
            "candidate_id IN (SELECT id FROM learning_assessment_candidates WHERE program_id=?)",
    },
];

pub(super) fn table_path(table: &str) -> String {
    format!("{BANK_TABLE_PREFIX}{table}.json")
}

/// Every bank table, in `BANK_TABLES` order, and nothing beside them.
fn decode_bank(pack: &DecodedLearningPack) -> Result<Vec<AggregateTableSnapshot>> {
    let known: Vec<String> = BANK_TABLES
        .iter()
        .map(|spec| table_path(spec.table))
        .collect();
    if let Some(unknown) = pack
        .entries
        .keys()
        .filter(|path| path.starts_with(BANK_TABLE_PREFIX))
        .find(|path| !known.contains(path))
    {
        return Err(invalid(format!(
            "Pack quiz banks have an unexpected table entry: {unknown}."
        )));
    }
    BANK_TABLES
        .iter()
        .zip(&known)
        .map(|(spec, path)| {
            let bytes = pack.entries.get(path).ok_or_else(|| {
                invalid(format!(
                    "Pack quiz banks are incomplete: missing {}.",
                    spec.table
                ))
            })?;
            let table: AggregateTableSnapshot = parse(bytes)?;
            if table.table != spec.table {
                return Err(invalid(
                    "Pack quiz bank entry names a different table than its path.",
                ));
            }
            Ok(table)
        })
        .collect()
}

fn text<'a>(row: &'a Value, column: &str) -> Result<&'a str> {
    row.get(column)
        .and_then(Value::as_str)
        .ok_or_else(|| invalid(format!("Pack quiz bank record lacks {column}.")))
}

fn optional_text<'a>(row: &'a Value, column: &str) -> Option<&'a str> {
    row.get(column).and_then(Value::as_str)
}

fn column_set<'a>(table: &'a AggregateTableSnapshot, column: &str) -> Result<HashSet<&'a str>> {
    table.rows.iter().map(|row| text(row, column)).collect()
}

/// Every row belongs to this course: its outcomes, lessons and modules, and
/// every bank item has its key.
fn validate(
    tables: &[AggregateTableSnapshot],
    program: &LearningProgramDto,
    outcome_rows: &[Value],
) -> Result<()> {
    let [settings, blueprints, slots, candidates, candidate_outcomes, keys] = tables else {
        return Err(invalid("Pack quiz banks are incomplete."));
    };
    if settings.rows.len() > 1 {
        return Err(invalid("Pack carries more than one set of quiz settings."));
    }
    let lessons: HashSet<&str> = program
        .modules
        .iter()
        .flat_map(|module| &module.lessons)
        .map(|lesson| lesson.id.as_str())
        .collect();
    let modules: HashSet<&str> = program.modules.iter().map(|m| m.id.as_str()).collect();
    let outcomes: HashSet<&str> = outcome_rows
        .iter()
        .filter_map(|row| row.get("id").and_then(Value::as_str))
        .collect();
    for blueprint in &blueprints.rows {
        let scoped = match (
            optional_text(blueprint, "scope_kind"),
            optional_text(blueprint, "scope_id"),
        ) {
            (Some("lesson"), Some(id)) => lessons.contains(id),
            (Some("module"), Some(id)) => modules.contains(id),
            (Some("course"), None) | (None, _) => true,
            _ => false,
        };
        if !scoped {
            return Err(invalid(
                "Pack carries a quiz blueprint for a lesson or module outside this course.",
            ));
        }
    }
    let blueprint_ids = column_set(blueprints, "id")?;
    for slot in &slots.rows {
        if !blueprint_ids.contains(text(slot, "blueprint_id")?)
            || !outcomes.contains(text(slot, "outcome_id")?)
        {
            return Err(invalid(
                "Pack carries a quiz blueprint slot outside the blueprints and outcomes it carries.",
            ));
        }
    }
    for candidate in &candidates.rows {
        if !outcomes.contains(text(candidate, "outcome_id")?)
            || optional_text(candidate, "lesson_id").is_some_and(|id| !lessons.contains(id))
        {
            return Err(invalid(
                "Pack carries a quiz question outside this course's outcomes and lessons.",
            ));
        }
    }
    let candidate_ids = column_set(candidates, "id")?;
    for row in &candidate_outcomes.rows {
        if !candidate_ids.contains(text(row, "candidate_id")?)
            || !outcomes.contains(text(row, "outcome_id")?)
        {
            return Err(invalid(
                "Pack carries a quiz question outcome outside the questions it carries.",
            ));
        }
    }
    if column_set(keys, "candidate_id")? != candidate_ids {
        return Err(invalid(
            "Pack quiz questions and their answer keys do not match.",
        ));
    }
    Ok(())
}

pub(super) struct BankTableRows;
pub(super) const BANK_TABLE_ROWS: BankTableRows = BankTableRows;

#[async_trait]
impl SectionRows for BankTableRows {
    fn section(&self) -> PackSection {
        PackSection::QuizBank
    }
    async fn encode(
        &self,
        connection: &mut SqliteConnection,
        scope: &ExportScope<'_>,
    ) -> Result<Vec<LearningPackEntry>> {
        let mut entries = Vec::with_capacity(BANK_TABLES.len());
        for spec in BANK_TABLES {
            let table = snapshot_table(connection, spec, &scope.program.summary.id).await?;
            if table.rows.len() > MAX_TABLE_ROWS {
                return Err(invalid(format!(
                    "This course has {} records in {}, more than the {MAX_TABLE_ROWS} per table a course pack can carry, so no computer could import it.",
                    table.rows.len(),
                    table.table
                )));
            }
            entries.push(LearningPackEntry {
                path: table_path(spec.table),
                kind: LearningPackEntryKind::Curriculum,
                bytes: json(&table)?,
            });
        }
        Ok(entries)
    }
    fn check(
        &self,
        pack: &DecodedLearningPack,
        program: &LearningProgramDto,
    ) -> Result<Vec<String>> {
        let outcomes: Vec<Value> = decode(pack, OUTCOMES_ENTRY)?.unwrap_or_default();
        validate(&decode_bank(pack)?, program, &outcomes)?;
        Ok(Vec::new())
    }
    fn claims(
        &self,
        pack: &DecodedLearningPack,
        _program: &LearningProgramDto,
    ) -> Result<Vec<IdClaim>> {
        let mut claims = Vec::new();
        for (table, spec) in decode_bank(pack)?.iter().zip(BANK_TABLES) {
            if !matches!(
                spec.table,
                "learning_assessment_blueprints" | "learning_assessment_candidates"
            ) {
                continue;
            }
            let mut seen = HashSet::new();
            for row in &table.rows {
                let id = text(row, "id")?;
                // A blueprint's revisions share its identity.
                if seen.insert(id) {
                    claims.push(IdClaim::column(spec.table, "id", id, None));
                }
            }
        }
        Ok(claims)
    }
    fn map_ids(
        &self,
        pack: &DecodedLearningPack,
        _program: &LearningProgramDto,
        ids: &mut HashMap<String, String>,
    ) -> Result<()> {
        for (table, spec) in decode_bank(pack)?.iter().zip(BANK_TABLES) {
            if matches!(
                spec.table,
                "learning_assessment_blueprints" | "learning_assessment_candidates"
            ) {
                for row in &table.rows {
                    fresh(ids, text(row, "id")?);
                }
            }
        }
        Ok(())
    }
    async fn insert(
        &self,
        connection: &mut SqliteConnection,
        scope: &ImportScope<'_>,
    ) -> Result<()> {
        let program_id = &scope.program.summary.id;
        for (table, spec) in decode_bank(scope.pack)?.iter().zip(BANK_TABLES) {
            for mut object in checked_rows(connection, spec, table).await? {
                rewrite(&mut object, program_id, scope.ids);
                insert_row(connection, "INSERT", spec.table, &table.columns, &object).await?;
            }
        }
        Ok(())
    }
}
