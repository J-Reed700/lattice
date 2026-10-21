//! Lesson claim reviews, one pack entry per table: each published lesson's
//! review, its claims and their history, its Verifier audits, the job the
//! review names, and the course's labelled checker examples. They are course
//! provenance, so they travel whatever the learner-evidence choice. A review
//! that is not finished stays behind: its draft lives in its job's
//! checkpoints, working state too large for a pack, and the export notice
//! and the replace preview say so.
//! Design: docs/design/2026-10-16-lesson-claim-review.md §5.2.
use super::{
    conflicts::IdClaim,
    entries::{fresh, ExportScope, ImportScope, SectionRows},
    evidence_tables::{
        checked_rows, insert_row, snapshot_table, AggregateTableSnapshot, AggregateTableSpec,
        MAX_TABLE_ROWS,
    },
    invalid, json, parse,
};
use crate::features::learning::{
    dto::LearningProgramDto,
    pack::{
        sections::{PackSection, REVIEW_TABLE_PREFIX},
        DecodedLearningPack, LearningPackEntry, LearningPackEntryKind,
    },
    portability_dto::LearningPackConflictKind,
};
use async_trait::async_trait;
use lattice_core::shared::error::Result;
use serde_json::{Map, Value};
use sqlx::SqliteConnection;
use std::collections::{HashMap, HashSet};

/// In insert order; each filter binds the course once. Only finished
/// reviews go, so every job here completed and published its lessons.
const REVIEW_TABLES: &[AggregateTableSpec] = &[
    AggregateTableSpec { table: "jobs", filter: "id IN (SELECT job_id FROM learning_lesson_reviews WHERE program_id=? AND state='ready')" },
    AggregateTableSpec { table: "learning_jobs", filter: "job_id IN (SELECT job_id FROM learning_lesson_reviews WHERE program_id=? AND state='ready')" },
    AggregateTableSpec { table: "learning_lesson_reviews", filter: "program_id=? AND state='ready'" },
    AggregateTableSpec { table: "learning_lesson_claims", filter: "lesson_id IN (SELECT lesson_id FROM learning_lesson_reviews WHERE program_id=? AND state='ready')" },
    AggregateTableSpec { table: "learning_lesson_claim_events", filter: "claim_id IN (SELECT c.claim_id FROM learning_lesson_claims c JOIN learning_lesson_reviews r ON r.lesson_id=c.lesson_id WHERE r.program_id=? AND r.state='ready')" },
    AggregateTableSpec { table: "learning_lesson_audits", filter: "lesson_id IN (SELECT lesson_id FROM learning_lesson_reviews WHERE program_id=? AND state='ready')" },
    AggregateTableSpec { table: "learning_checker_examples", filter: "program_id=?" },
];

fn table_path(table: &str) -> String {
    format!("{REVIEW_TABLE_PREFIX}{table}.json")
}

/// Every review table, in `REVIEW_TABLES` order, and nothing beside them.
fn decode_reviews(pack: &DecodedLearningPack) -> Result<Vec<AggregateTableSnapshot>> {
    let known: Vec<String> = REVIEW_TABLES
        .iter()
        .map(|spec| table_path(spec.table))
        .collect();
    if let Some(unknown) = pack
        .entries
        .keys()
        .filter(|path| path.starts_with(REVIEW_TABLE_PREFIX))
        .find(|path| !known.contains(path))
    {
        return Err(invalid(format!(
            "Pack lesson reviews have an unexpected table entry: {unknown}."
        )));
    }
    REVIEW_TABLES
        .iter()
        .zip(&known)
        .map(|(spec, path)| {
            let bytes = pack.entries.get(path).ok_or_else(|| {
                invalid(format!(
                    "Pack lesson reviews are incomplete: missing {}.",
                    spec.table
                ))
            })?;
            let table: AggregateTableSnapshot = parse(bytes)?;
            if table.table != spec.table {
                return Err(invalid(
                    "Pack review entry names a different table than its path.",
                ));
            }
            Ok(table)
        })
        .collect()
}

fn text<'a>(row: &'a Value, column: &str) -> Result<&'a str> {
    row.get(column)
        .and_then(Value::as_str)
        .ok_or_else(|| invalid(format!("Pack lesson review record lacks {column}.")))
}

fn column_set<'a>(table: &'a AggregateTableSnapshot, column: &str) -> Result<HashSet<&'a str>> {
    table.rows.iter().map(|row| text(row, column)).collect()
}

/// Every row hangs off a finished review of one of the pack's own lessons,
/// so an import can neither attach records to another course nor bring in
/// a job that would start running here.
fn validate(tables: &[AggregateTableSnapshot], program: &LearningProgramDto) -> Result<()> {
    let [jobs, learning_jobs, reviews, claims, events, audits, _examples] = tables else {
        return Err(invalid("Pack lesson reviews are incomplete."));
    };
    let lessons: HashSet<&str> = program
        .modules
        .iter()
        .flat_map(|module| &module.lessons)
        .map(|lesson| lesson.id.as_str())
        .collect();
    let job_ids = column_set(jobs, "id")?;
    for job in &jobs.rows {
        if matches!(text(job, "status")?, "pending" | "running") {
            return Err(invalid(
                "Pack carries a lesson job that would start running here.",
            ));
        }
    }
    if column_set(learning_jobs, "job_id")? != job_ids {
        return Err(invalid(
            "Pack lesson jobs and their course records do not match.",
        ));
    }
    for review in &reviews.rows {
        if text(review, "state")? != "ready"
            || !lessons.contains(text(review, "lesson_id")?)
            || !job_ids.contains(text(review, "job_id")?)
        {
            return Err(invalid(
                "Pack carries a lesson review that does not belong to a finished lesson of this course.",
            ));
        }
    }
    let reviewed = column_set(reviews, "lesson_id")?;
    let claim_ids = column_set(claims, "claim_id")?;
    let attached = |table: &AggregateTableSnapshot, column: &str, owners: &HashSet<&str>| {
        for row in &table.rows {
            if !owners.contains(text(row, column)?) {
                return Err(invalid(format!(
                    "Pack carries a {} record outside the reviews it carries.",
                    table.table
                )));
            }
        }
        Ok(())
    };
    attached(claims, "lesson_id", &reviewed)?;
    attached(events, "claim_id", &claim_ids)?;
    attached(audits, "lesson_id", &reviewed)
}

/// Every identifier the pack maps, wherever it appears in `text`. A review's
/// rows name lessons, claims, sources and jobs inside JSON, some of it JSON
/// held in a string like a job's request, and in a job's result reference,
/// so a copy follows them by value. They are UUIDs, which nothing else in
/// these rows spells.
fn remap_uuids(text: &str, map: &HashMap<String, String>) -> String {
    const UUID_LEN: usize = 36;
    let is_uuid = |window: &[u8]| {
        window.iter().enumerate().all(|(index, byte)| match index {
            8 | 13 | 18 | 23 => *byte == b'-',
            _ => byte.is_ascii_hexdigit(),
        })
    };
    let bytes = text.as_bytes();
    let mut out = String::with_capacity(text.len());
    let (mut copied, mut at) = (0, 0);
    while at + UUID_LEN <= bytes.len() {
        if !is_uuid(&bytes[at..at + UUID_LEN]) {
            at += 1;
            continue;
        }
        // An ASCII match, so both ends are character boundaries.
        if let Some(new) = map.get(&text[at..at + UUID_LEN]) {
            out.push_str(&text[copied..at]);
            out.push_str(new);
            copied = at + UUID_LEN;
        }
        at += UUID_LEN;
    }
    out.push_str(&text[copied..]);
    out
}

pub(super) struct ReviewTableRows;
pub(super) const REVIEW_TABLE_ROWS: ReviewTableRows = ReviewTableRows;

#[async_trait]
impl SectionRows for ReviewTableRows {
    fn section(&self) -> PackSection {
        PackSection::ReviewTables
    }
    async fn encode(
        &self,
        connection: &mut SqliteConnection,
        scope: &ExportScope<'_>,
    ) -> Result<Vec<LearningPackEntry>> {
        let mut entries = Vec::with_capacity(REVIEW_TABLES.len());
        for spec in REVIEW_TABLES {
            let mut table = snapshot_table(connection, spec, &scope.program.summary.id).await?;
            if table.rows.len() > MAX_TABLE_ROWS {
                return Err(invalid(format!(
                    "This course has {} records in {}, more than the {MAX_TABLE_ROWS} per table a course pack can carry, so no computer could import it.",
                    table.rows.len(),
                    table.table
                )));
            }
            if spec.table == "jobs" {
                // A published lesson's staged copy is the lesson itself.
                for row in &mut table.rows {
                    if let Some(object) = row.as_object_mut() {
                        object.insert("staged_result_json".into(), Value::Null);
                    }
                }
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
        validate(&decode_reviews(pack)?, program)?;
        Ok(Vec::new())
    }
    fn claims(
        &self,
        pack: &DecodedLearningPack,
        _program: &LearningProgramDto,
    ) -> Result<Vec<IdClaim>> {
        let tables = decode_reviews(pack)?;
        let mut claims = Vec::new();
        for (table, spec) in tables.iter().zip(REVIEW_TABLES) {
            for row in &table.rows {
                claims.push(match spec.table {
                    "jobs" => IdClaim::owned_by(
                        LearningPackConflictKind::Record,
                        spec.table,
                        "SELECT COALESCE((SELECT program_id FROM learning_jobs WHERE job_id=jobs.id),'') FROM jobs WHERE id=?",
                        text(row, "id")?,
                    ),
                    "learning_lesson_claims" => IdClaim::owned_by(
                        LearningPackConflictKind::Record,
                        spec.table,
                        "SELECT COALESCE((SELECT r.program_id FROM learning_lesson_reviews r WHERE r.lesson_id=c.lesson_id),'') FROM learning_lesson_claims c WHERE c.claim_id=?",
                        text(row, "claim_id")?,
                    ),
                    "learning_checker_examples" => {
                        IdClaim::column(spec.table, "id", text(row, "id")?, None)
                    }
                    _ => continue,
                });
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
        // A claim keeps one identity across its review, its history and the
        // examples it labelled; an example whose claim is gone gets a fresh one.
        for (table, spec) in decode_reviews(pack)?.iter().zip(REVIEW_TABLES) {
            let identities: &[&str] = match spec.table {
                "jobs" => &["id", "operation_id"],
                "learning_lesson_claims" => &["claim_id"],
                "learning_checker_examples" => &["id", "claim_id"],
                _ => continue,
            };
            for row in &table.rows {
                for column in identities {
                    fresh(ids, text(row, column)?);
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
        for (table, spec) in decode_reviews(scope.pack)?.iter().zip(REVIEW_TABLES) {
            for mut object in checked_rows(connection, spec, table).await? {
                rewrite(&mut object, program_id, scope.ids);
                match spec.table {
                    // The retry lineage and the course content the job was
                    // current with stay on the computer that ran it: here
                    // it records how a published lesson was checked, and is
                    // never work to resume.
                    "jobs" => {
                        object.insert("retry_of_job_id".into(), Value::Null);
                    }
                    "learning_jobs" => {
                        object.insert("content_revision".into(), Value::Null);
                    }
                    _ => {}
                }
                insert_row(connection, "INSERT", spec.table, &table.columns, &object).await?;
            }
        }
        Ok(())
    }
}

/// The row as it is written here: in this course, and for a copy with every
/// mapped identifier replaced.
fn rewrite(object: &mut Map<String, Value>, program_id: &str, ids: &HashMap<String, String>) {
    for (column, value) in object.iter_mut() {
        if column == "program_id" {
            *value = Value::String(program_id.to_owned());
        } else if let (false, Some(text)) = (ids.is_empty(), value.as_str()) {
            *value = Value::String(remap_uuids(text, ids));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::remap_uuids;
    use std::collections::HashMap;

    #[test]
    fn a_copy_follows_every_mapped_uuid_and_leaves_other_text_alone() {
        let lesson = "5f0b3c1e-8a2d-4c6f-9e1a-2b3c4d5e6f70";
        let kept = "0a1b2c3d-4e5f-4a6b-8c7d-9e0f1a2b3c4d";
        let ids = HashMap::from([(lesson.to_owned(), "new-lesson".to_owned())]);
        let request =
            format!(r#"{{"requestJson":"{{\"lessonIds\":[\"{lesson}\"]}}","note":"{kept} é"}}"#);
        assert_eq!(
            remap_uuids(&request, &ids),
            format!(r#"{{"requestJson":"{{\"lessonIds\":[\"new-lesson\"]}}","note":"{kept} é"}}"#)
        );
        assert_eq!(
            remap_uuids(&format!("needs_review:{lesson},{kept}"), &ids),
            format!("needs_review:new-lesson,{kept}")
        );
        let digest = "e8d08e1b98f336736c784978096ceee3330d1e600ee376668057d14687ea96fa";
        assert_eq!(remap_uuids(digest, &ids), digest);
    }
}
