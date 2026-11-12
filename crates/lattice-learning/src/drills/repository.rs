//! The bank of built drills: what a finished build writes, and what the bank reads back. A drill comes back as the
//! core's `Drill` with its clean files and its evidence. The hidden suite and the killing tests inside the key's
//! proofs are what a learner's fix is run against; the learner never sees them. A drill on a passage keeps the
//! source passage its explanation was written from.

use super::bank::Shelf;
use super::build::{Evidence, SourcePassage};
use super::spec::DrillSpec;
use crate::persistence::{decode, encode, now};
use lattice_core::domain::learning::drills::{Drill, DrillFile, KeyIssue, Material};
use lattice_core::shared::error::{AppError, Result};
use lattice_core::shared::persistence::{Database, WriteTx};
use sqlx::{sqlite::SqliteRow, Row, SqliteConnection};
use std::collections::HashMap;

/// Whether a drill is waiting in the bank or a learner has it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DrillStatus {
    Ready,
    Taken,
}

impl DrillStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ready => "ready",
            Self::Taken => "taken",
        }
    }

    fn parse(value: &str) -> Result<Self> {
        match value {
            "ready" => Ok(Self::Ready),
            "taken" => Ok(Self::Taken),
            other => Err(AppError::Database(format!("Unknown drill status {other}"))),
        }
    }
}

/// A drill a finished build writes.
pub(crate) struct NewDrill<'a> {
    pub drill: &'a Drill,
    pub clean_files: &'a [DrillFile],
    pub spec: &'a DrillSpec,
    pub domain: &'a str,
    /// The hidden suite, in a runner language.
    pub suite: Option<&'a str>,
    /// The passage an explanation was written from: `None` for code.
    pub source: Option<&'a SourcePassage>,
    /// What the suite and the proofs ran on, such as `builtin-python`.
    pub runner: Option<&'a str>,
    /// The model that built the drill, as the server reports it.
    pub model: &'a str,
    pub evidence: &'a Evidence,
}

/// A drill as the bank keeps it.
#[derive(Debug, Clone, PartialEq)]
pub struct StoredDrill {
    pub drill: Drill,
    /// The material before any issue was planted.
    pub clean_files: Vec<DrillFile>,
    pub spec: DrillSpec,
    /// The title of the lesson the drill was built from, as it was then; for a drill from before lessons, the domain
    /// its code was set in.
    pub domain: String,
    /// The course the drill was built for, while it exists.
    pub program_id: Option<String>,
    /// The lesson the drill was built from, while it exists.
    pub lesson_id: Option<String>,
    /// The title the drill's lesson has now, while the lesson exists.
    pub lesson_title: Option<String>,
    pub suite: Option<String>,
    /// The passage an explanation was written from: `None` for code.
    pub source: Option<SourcePassage>,
    pub runner: Option<String>,
    pub model: String,
    pub evidence: Evidence,
    pub status: DrillStatus,
    pub created_at: i64,
    pub taken_at: Option<i64>,
}

/// Writes a drill, ready, with a row for each pattern its key holds. The course it names is kept only if it still
/// exists, and the lesson only if it is a lesson of that course. Call inside the transaction that completes the build.
pub(crate) async fn insert_in(tx: &mut WriteTx, new: &NewDrill<'_>) -> Result<()> {
    let program_id: Option<String> = match &new.spec.program_id {
        Some(id) => {
            sqlx::query_scalar("SELECT id FROM learning_programs WHERE id=?")
                .bind(id)
                .fetch_optional(&mut **tx)
                .await?
        }
        None => None,
    };
    let lesson_id: Option<String> = match (&program_id, &new.spec.lesson_id) {
        (Some(program_id), Some(lesson_id)) => {
            sqlx::query_scalar("SELECT id FROM learning_lessons WHERE id=? AND program_id=?")
                .bind(lesson_id)
                .bind(program_id)
                .fetch_optional(&mut **tx)
                .await?
        }
        _ => None,
    };
    let source = match (&new.drill.material, new.source) {
        (Material::Code { .. }, None) => None,
        (Material::Passage, Some(source)) => Some(encode(source)?),
        (Material::Code { .. }, Some(_)) => {
            return Err(AppError::InvalidData(
                "A drill of code has no source passage.".into(),
            ))
        }
        (Material::Passage, None) => {
            return Err(AppError::InvalidData(
                "A drill on a passage needs the source passage it was written from.".into(),
            ))
        }
    };
    sqlx::query("INSERT INTO learning_drills(id,material,language,source_json,title,domain,program_id,lesson_id,spec_json,clean_files_json,files_json,key_json,evidence_json,suite,runner,model,status,created_at,taken_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,'ready',?,NULL)")
        .bind(&new.drill.id)
        .bind(material_name(&new.drill.material))
        .bind(new.drill.material.language())
        .bind(source)
        .bind(&new.drill.title)
        .bind(new.domain)
        .bind(program_id)
        .bind(lesson_id)
        .bind(encode(new.spec)?)
        .bind(encode(new.clean_files)?)
        .bind(encode(&new.drill.files)?)
        .bind(encode(&new.drill.key)?)
        .bind(encode(new.evidence)?)
        .bind(new.suite)
        .bind(new.runner)
        .bind(new.model)
        .bind(now())
        .execute(&mut **tx)
        .await?;
    let mut patterns: Vec<&str> = Vec::new();
    for issue in &new.drill.key {
        if patterns.contains(&issue.pattern.as_str()) {
            continue;
        }
        patterns.push(&issue.pattern);
        sqlx::query("INSERT INTO learning_drill_patterns(drill_id,pattern_id) VALUES(?,?)")
            .bind(&new.drill.id)
            .bind(&issue.pattern)
            .execute(&mut **tx)
            .await?;
    }
    Ok(())
}

/// The columns `read` reads.
const COLUMNS: &str = "id,material,language,source_json,title,domain,program_id,lesson_id,spec_json,clean_files_json,files_json,key_json,evidence_json,suite,runner,model,status,created_at,taken_at,(SELECT l.title FROM learning_lessons l WHERE l.id=learning_drills.lesson_id AND l.program_id=learning_drills.program_id) AS lesson_title";

/// What follows `SELECT <columns>` in `oldest_ready_on`: ?1 is the course, ?2 the material, ?3 the language and ?4 the
/// lesson, or null for any.
pub(crate) const OLDEST_READY_ON: &str = "FROM learning_drills WHERE program_id=?1 AND material=?2 AND language IS ?3 AND status='ready' AND lesson_id IS NOT NULL AND (?4 IS NULL OR lesson_id=?4) ORDER BY created_at,rowid LIMIT 1";

/// The query of `any_ready_in`: ?1 is the course, ?2 the material and ?3 the language.
pub(crate) const ANY_READY_IN: &str = "SELECT EXISTS(SELECT 1 FROM learning_drills WHERE program_id=?1 AND material=?2 AND language IS ?3 AND status='ready' AND lesson_id IS NOT NULL)";

/// The name a material is stored under.
fn material_name(material: &Material) -> &'static str {
    match material {
        Material::Code { .. } => "code",
        Material::Passage => "passage",
    }
}

/// The oldest ready drill of `shelf` built from a lesson: from `lesson`, when one is given.
pub(crate) async fn oldest_ready_on(
    connection: &mut SqliteConnection,
    shelf: Shelf<'_>,
    lesson: Option<&str>,
) -> Result<Option<StoredDrill>> {
    sqlx::query(sqlx::AssertSqlSafe(format!(
        "SELECT {COLUMNS} {OLDEST_READY_ON}"
    )))
    .bind(shelf.program_id)
    .bind(material_name(shelf.material))
    .bind(shelf.material.language())
    .bind(lesson)
    .fetch_optional(connection)
    .await?
    .as_ref()
    .map(read)
    .transpose()
}

/// Whether a drill of `shelf` built from a lesson is ready.
pub(crate) async fn any_ready_in(
    connection: &mut SqliteConnection,
    shelf: Shelf<'_>,
) -> Result<bool> {
    sqlx::query_scalar(ANY_READY_IN)
        .bind(shelf.program_id)
        .bind(material_name(shelf.material))
        .bind(shelf.material.language())
        .fetch_one(connection)
        .await
        .map_err(AppError::from)
}

/// The source passages the drills of the course `program_id` were built on, ready or taken.
pub(crate) async fn drilled_passages(
    database: &Database,
    program_id: &str,
) -> Result<Vec<SourcePassage>> {
    let mut read = database.begin_read().await?;
    let stored: Vec<String> = sqlx::query_scalar(
        "SELECT source_json FROM learning_drills WHERE material='passage' AND program_id=? AND source_json IS NOT NULL ORDER BY created_at,rowid",
    )
    .bind(program_id)
    .fetch_all(&mut *read)
    .await?;
    read.commit().await?;
    stored.iter().map(|json| decode(json)).collect()
}

/// For each version of the course's sources, the source it is a version of.
pub(crate) async fn source_owners(
    database: &Database,
    program_id: &str,
) -> Result<HashMap<String, String>> {
    let mut read = database.begin_read().await?;
    let rows: Vec<(String, String)> =
        sqlx::query_as("SELECT id,source_id FROM learning_source_versions WHERE program_id=?")
            .bind(program_id)
            .fetch_all(&mut *read)
            .await?;
    read.commit().await?;
    Ok(rows.into_iter().collect())
}

/// A drill by its id, ready or taken.
pub async fn find_in(connection: &mut SqliteConnection, id: &str) -> Result<Option<StoredDrill>> {
    sqlx::query(sqlx::AssertSqlSafe(format!(
        "SELECT {COLUMNS} FROM learning_drills WHERE id=?"
    )))
    .bind(id)
    .fetch_optional(connection)
    .await?
    .as_ref()
    .map(read)
    .transpose()
}

/// Marks a ready drill taken. False when it was not ready, so a drill is never given to two learners.
pub(crate) async fn mark_taken_in(tx: &mut WriteTx, id: &str, at: i64) -> Result<bool> {
    Ok(sqlx::query(
        "UPDATE learning_drills SET status='taken',taken_at=? WHERE id=? AND status='ready'",
    )
    .bind(at)
    .bind(id)
    .execute(&mut **tx)
    .await?
    .rows_affected()
        == 1)
}

fn read(row: &SqliteRow) -> Result<StoredDrill> {
    let files: Vec<DrillFile> = decode(&row.get::<String, _>("files_json"))?;
    let key: Vec<KeyIssue> = decode(&row.get::<String, _>("key_json"))?;
    let material = match (
        row.get::<String, _>("material").as_str(),
        row.get::<Option<String>, _>("language"),
    ) {
        ("code", Some(language)) => Material::Code { language },
        ("passage", None) => Material::Passage,
        (other, _) => {
            return Err(AppError::Database(format!(
                "A drill's material {other} does not agree with its language"
            )))
        }
    };
    let source = row
        .get::<Option<String>, _>("source_json")
        .map(|json| decode(&json))
        .transpose()?;
    Ok(StoredDrill {
        drill: Drill {
            id: row.get("id"),
            material,
            title: row.get("title"),
            files,
            key,
        },
        clean_files: decode(&row.get::<String, _>("clean_files_json"))?,
        spec: decode(&row.get::<String, _>("spec_json"))?,
        domain: row.get("domain"),
        program_id: row.get("program_id"),
        lesson_id: row.get("lesson_id"),
        lesson_title: row.get("lesson_title"),
        suite: row.get("suite"),
        source,
        runner: row.get("runner"),
        model: row.get("model"),
        evidence: decode(&row.get::<String, _>("evidence_json"))?,
        status: DrillStatus::parse(&row.get::<String, _>("status"))?,
        created_at: row.get("created_at"),
        taken_at: row.get("taken_at"),
    })
}
