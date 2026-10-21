//! Teaching-review findings the repairs did not settle, in
//! `learning_lesson_teaching_findings`. The lesson publishes, and each finding
//! waits in the lesson's review panel, with the reviewer's note and the part
//! of the lesson it is about, until the user keeps the text as written.
//!
//! A finding is identified by its kind and place (`FindingKey`), never its
//! wording. It is bound to the text of that part (`unit_sha256`): while the
//! text is unchanged the machine does not repair it again and a "keep as
//! written" stands; once the text changes, both lapse.
use std::collections::BTreeSet;

use lattice_core::shared::error::{AppError, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::{Row, SqlitePool};

use crate::features::learning::persistence::{db, now};
use crate::features::learning::review_findings::{
    site_sha256, unit, FindingKey, FindingKind, FindingSite, SetAside,
};

/// What raised a teaching finding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum TeachingFindingKindDto {
    /// The teaching review of a section or of the lesson.
    Teaching,
    /// An independent solution disagreed with the answer key.
    AnswerKey,
    /// The lesson's shape: its schema, a section's length or rubric, its mix
    /// of sections and questions.
    Structure,
    /// A source quotation that matches no saved reference.
    Quotation,
}

/// The part of the lesson a teaching finding is about.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum TeachingFindingSiteDto {
    Lesson,
    Section,
    Question,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum TeachingFindingStateDto {
    /// Waiting for the user.
    Waiting,
    /// The user kept the text as written.
    Kept,
}

/// A teaching-review finding the repairs did not settle.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct TeachingFindingDto {
    /// Names the finding in `keep_learning_teaching_finding`.
    pub finding_key: String,
    pub kind: TeachingFindingKindDto,
    pub site: TeachingFindingSiteDto,
    /// Zero-based section or question index; null for the whole lesson.
    pub unit_index: Option<u32>,
    /// "Section 3: Loops", "Question 2" or "Whole lesson".
    pub unit_title: String,
    /// The reviewer's note.
    pub note: String,
    /// The passage the finding is about, when one was chosen.
    pub passage: Option<String>,
    /// Repairs that tried to settle it.
    pub repairs: u32,
    pub state: TeachingFindingStateDto,
}

fn kind_dto(kind: FindingKind) -> TeachingFindingKindDto {
    match kind {
        FindingKind::Teaching => TeachingFindingKindDto::Teaching,
        FindingKind::AnswerKey => TeachingFindingKindDto::AnswerKey,
        FindingKind::Structure => TeachingFindingKindDto::Structure,
        FindingKind::Quotation => TeachingFindingKindDto::Quotation,
    }
}

fn site_columns(site: FindingSite) -> (&'static str, Option<i64>) {
    match site {
        FindingSite::Lesson => ("lesson", None),
        FindingSite::Section(index) => ("section", i64::try_from(index).ok()),
        FindingSite::Question(index) => ("question", i64::try_from(index).ok()),
    }
}

fn site_of(site: &str, index: Option<i64>) -> Option<FindingSite> {
    let index = index.and_then(|index| usize::try_from(index).ok());
    match (site, index) {
        ("lesson", None) => Some(FindingSite::Lesson),
        ("section", Some(index)) => Some(FindingSite::Section(index)),
        ("question", Some(index)) => Some(FindingSite::Question(index)),
        _ => None,
    }
}

#[derive(Clone)]
pub struct TeachingFindingsRepository {
    pool: SqlitePool,
}

impl TeachingFindingsRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// The lesson's findings whose part still has the text they were found
    /// in: the machine does not repair these again.
    pub(in crate::features::learning) async fn held(
        &self,
        lesson_id: &str,
        candidate: &Value,
    ) -> Result<BTreeSet<FindingKey>> {
        let rows = sqlx::query(
            "SELECT kind, site, unit_index, unit_sha256 FROM learning_lesson_teaching_findings WHERE lesson_id=?",
        )
        .bind(lesson_id)
        .fetch_all(&self.pool)
        .await
        .map_err(db)?;
        Ok(rows
            .iter()
            .filter_map(|row| {
                let key = FindingKey {
                    kind: FindingKind::from_column(&row.get::<String, _>("kind"))?,
                    site: site_of(&row.get::<String, _>("site"), row.get("unit_index"))?,
                };
                (site_sha256(candidate, key.site) == row.get::<String, _>("unit_sha256"))
                    .then_some(key)
            })
            .collect())
    }

    /// Record findings for `candidate`. A finding kept as written stays kept
    /// while its part's text is unchanged; otherwise it waits again with the
    /// latest note. With `replace`, the findings not raised again were
    /// fixed and go.
    pub(in crate::features::learning) async fn record(
        &self,
        lesson_id: &str,
        job_id: &str,
        candidate: &Value,
        findings: &[SetAside],
        replace: bool,
    ) -> Result<()> {
        let mut tx = self.pool.begin().await.map_err(db)?;
        let at = now();
        let keys: Vec<String> = findings.iter().map(|f| f.finding.key().id()).collect();
        if replace {
            sqlx::query(
                "DELETE FROM learning_lesson_teaching_findings WHERE lesson_id=? AND finding_key NOT IN (SELECT value FROM json_each(?))",
            )
            .bind(lesson_id)
            .bind(serde_json::to_string(&keys)?)
            .execute(&mut *tx)
            .await
            .map_err(db)?;
        }
        for (SetAside { finding, repairs }, key) in findings.iter().zip(&keys) {
            let shown = unit(candidate, finding);
            let (site, index) = site_columns(finding.key().site);
            sqlx::query(
                "INSERT INTO learning_lesson_teaching_findings \
                 (lesson_id, finding_key, kind, site, unit_index, unit_title, unit_sha256, note, passage, repairs, state, job_id, found_at, kept_at) \
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 'waiting', ?, ?, NULL) \
                 ON CONFLICT(lesson_id, finding_key) DO UPDATE SET \
                 unit_title=excluded.unit_title, note=excluded.note, passage=excluded.passage, \
                 repairs=max(repairs, excluded.repairs), job_id=excluded.job_id, found_at=excluded.found_at, \
                 state=CASE WHEN state='kept' AND unit_sha256=excluded.unit_sha256 THEN 'kept' ELSE 'waiting' END, \
                 kept_at=CASE WHEN state='kept' AND unit_sha256=excluded.unit_sha256 THEN kept_at ELSE NULL END, \
                 unit_sha256=excluded.unit_sha256",
            )
            .bind(lesson_id)
            .bind(key.as_str())
            .bind(finding.kind.as_str())
            .bind(site)
            .bind(index)
            .bind(shown.title.as_str())
            .bind(shown.sha256.as_str())
            .bind(finding.note.as_str())
            .bind(finding.passage.as_deref().or(shown.passage.as_deref()))
            .bind(i64::from(*repairs))
            .bind(job_id)
            .bind(at)
            .execute(&mut *tx)
            .await
            .map_err(db)?;
        }
        tx.commit().await.map_err(db)
    }

    /// The lesson's findings: by place, then kind.
    pub async fn list(&self, lesson_id: &str) -> Result<Vec<TeachingFindingDto>> {
        let rows = sqlx::query(
            "SELECT finding_key, kind, site, unit_index, unit_title, note, passage, repairs, state \
             FROM learning_lesson_teaching_findings WHERE lesson_id=? \
             ORDER BY CASE site WHEN 'lesson' THEN 0 WHEN 'section' THEN 1 ELSE 2 END, unit_index, kind",
        )
        .bind(lesson_id)
        .fetch_all(&self.pool)
        .await
        .map_err(db)?;
        rows.iter()
            .map(|row| {
                let corrupt = || AppError::InternalError("Invalid stored teaching finding".into());
                let site = site_of(&row.get::<String, _>("site"), row.get("unit_index"))
                    .ok_or_else(corrupt)?;
                Ok(TeachingFindingDto {
                    finding_key: row.get("finding_key"),
                    kind: kind_dto(
                        FindingKind::from_column(&row.get::<String, _>("kind"))
                            .ok_or_else(corrupt)?,
                    ),
                    site: match site {
                        FindingSite::Lesson => TeachingFindingSiteDto::Lesson,
                        FindingSite::Section(_) => TeachingFindingSiteDto::Section,
                        FindingSite::Question(_) => TeachingFindingSiteDto::Question,
                    },
                    unit_index: row
                        .get::<Option<i64>, _>("unit_index")
                        .and_then(|index| u32::try_from(index).ok()),
                    unit_title: row.get("unit_title"),
                    note: row.get("note"),
                    passage: row.get("passage"),
                    repairs: u32::try_from(row.get::<i64, _>("repairs")).unwrap_or_default(),
                    state: if row.get::<String, _>("state") == "kept" {
                        TeachingFindingStateDto::Kept
                    } else {
                        TeachingFindingStateDto::Waiting
                    },
                })
            })
            .collect()
    }

    /// "Keep as written": the user resolves a waiting finding without a
    /// change. Refused when the finding is not waiting in this course.
    pub async fn keep(&self, program_id: &str, lesson_id: &str, finding_key: &str) -> Result<()> {
        let kept = sqlx::query(
            "UPDATE learning_lesson_teaching_findings SET state='kept', kept_at=? \
             WHERE lesson_id=? AND finding_key=? AND state='waiting' \
             AND EXISTS (SELECT 1 FROM learning_lessons WHERE id=? AND program_id=?)",
        )
        .bind(now())
        .bind(lesson_id)
        .bind(finding_key)
        .bind(lesson_id)
        .bind(program_id)
        .execute(&self.pool)
        .await
        .map_err(db)?;
        if kept.rows_affected() == 0 {
            return Err(AppError::NotFound(
                "This teaching finding is no longer waiting for you. Reload the lesson.".into(),
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "teaching_findings_tests.rs"]
mod tests;
