//! The bank of built drills: what a finished build writes, and what the bank reads back. A drill comes back as the
//! core's `Drill` with its clean files and its evidence. The hidden suite and the killing tests inside the key's
//! proofs are what a learner's fix is run against; the learner never sees them.

use super::build::Evidence;
use super::spec::DrillSpec;
use crate::features::learning::persistence::{db, decode, encode, now};
use lattice_core::domain::learning::drills::{Drill, DrillFile, KeyIssue};
use lattice_core::shared::error::{AppError, Result};
use sqlx::{sqlite::SqliteRow, Row, SqliteConnection, SqlitePool};

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
pub(in crate::features::learning) struct NewDrill<'a> {
    pub drill: &'a Drill,
    pub clean_files: &'a [DrillFile],
    pub spec: &'a DrillSpec,
    pub domain: &'a str,
    /// The hidden suite, in a runner language.
    pub suite: Option<&'a str>,
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
    /// The code before any issue was planted.
    pub clean_files: Vec<DrillFile>,
    pub spec: DrillSpec,
    pub domain: String,
    /// The course whose topic gave the domain, while the course exists.
    pub program_id: Option<String>,
    pub suite: Option<String>,
    pub runner: Option<String>,
    pub model: String,
    pub evidence: Evidence,
    pub status: DrillStatus,
    pub created_at: i64,
    pub taken_at: Option<i64>,
}

/// Writes a drill, ready, with a row for each pattern its key holds. The course it names is kept only if it still
/// exists. Call inside the transaction that completes the build.
pub(in crate::features::learning) async fn insert_in(
    connection: &mut SqliteConnection,
    new: &NewDrill<'_>,
) -> Result<()> {
    let program_id: Option<String> = match &new.spec.program_id {
        Some(id) => sqlx::query_scalar("SELECT id FROM learning_programs WHERE id=?")
            .bind(id)
            .fetch_optional(&mut *connection)
            .await
            .map_err(db)?,
        None => None,
    };
    sqlx::query("INSERT INTO learning_drills(id,language,title,domain,program_id,spec_json,clean_files_json,files_json,key_json,evidence_json,suite,runner,model,status,created_at,taken_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,'ready',?,NULL)")
        .bind(&new.drill.id)
        .bind(&new.drill.language)
        .bind(&new.drill.title)
        .bind(new.domain)
        .bind(program_id)
        .bind(encode(new.spec)?)
        .bind(encode(new.clean_files)?)
        .bind(encode(&new.drill.files)?)
        .bind(encode(&new.drill.key)?)
        .bind(encode(new.evidence)?)
        .bind(new.suite)
        .bind(new.runner)
        .bind(new.model)
        .bind(now())
        .execute(&mut *connection)
        .await
        .map_err(db)?;
    let mut patterns: Vec<&str> = Vec::new();
    for issue in &new.drill.key {
        if patterns.contains(&issue.pattern.as_str()) {
            continue;
        }
        patterns.push(&issue.pattern);
        sqlx::query("INSERT INTO learning_drill_patterns(drill_id,pattern_id) VALUES(?,?)")
            .bind(&new.drill.id)
            .bind(&issue.pattern)
            .execute(&mut *connection)
            .await
            .map_err(db)?;
    }
    Ok(())
}

/// The oldest ready drill of `language`.
pub(in crate::features::learning) async fn oldest_ready_in(
    connection: &mut SqliteConnection,
    language: &str,
) -> Result<Option<StoredDrill>> {
    sqlx::query("SELECT id,language,title,domain,program_id,spec_json,clean_files_json,files_json,key_json,evidence_json,suite,runner,model,status,created_at,taken_at FROM learning_drills WHERE language=? AND status='ready' ORDER BY created_at,rowid LIMIT 1")
        .bind(language)
        .fetch_optional(connection)
        .await
        .map_err(db)?
        .as_ref()
        .map(read)
        .transpose()
}

/// Marks a ready drill taken. False when it was not ready, so a drill is never given to two learners.
pub(in crate::features::learning) async fn mark_taken_in(
    connection: &mut SqliteConnection,
    id: &str,
    at: i64,
) -> Result<bool> {
    Ok(sqlx::query(
        "UPDATE learning_drills SET status='taken',taken_at=? WHERE id=? AND status='ready'",
    )
    .bind(at)
    .bind(id)
    .execute(connection)
    .await
    .map_err(db)?
    .rows_affected()
        == 1)
}

/// For each domain used for `language`, when a drill of it was last built.
pub(in crate::features::learning) async fn domain_uses(
    pool: &SqlitePool,
    language: &str,
) -> Result<Vec<(String, i64)>> {
    sqlx::query_as::<_, (String, i64)>(
        "SELECT domain,max(created_at) FROM learning_drills WHERE language=? GROUP BY domain",
    )
    .bind(language)
    .fetch_all(pool)
    .await
    .map_err(db)
}

fn read(row: &SqliteRow) -> Result<StoredDrill> {
    let files: Vec<DrillFile> = decode(&row.get::<String, _>("files_json"))?;
    let key: Vec<KeyIssue> = decode(&row.get::<String, _>("key_json"))?;
    Ok(StoredDrill {
        drill: Drill {
            id: row.get("id"),
            language: row.get("language"),
            title: row.get("title"),
            files,
            key,
        },
        clean_files: decode(&row.get::<String, _>("clean_files_json"))?,
        spec: decode(&row.get::<String, _>("spec_json"))?,
        domain: row.get("domain"),
        program_id: row.get("program_id"),
        suite: row.get("suite"),
        runner: row.get("runner"),
        model: row.get("model"),
        evidence: decode(&row.get::<String, _>("evidence_json"))?,
        status: DrillStatus::parse(&row.get::<String, _>("status"))?,
        created_at: row.get("created_at"),
        taken_at: row.get("taken_at"),
    })
}
