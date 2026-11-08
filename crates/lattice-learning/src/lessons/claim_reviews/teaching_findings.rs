//! Teaching-review findings the repairs did not settle, in
//! `learning_lesson_teaching_findings`. The lesson publishes, and each finding
//! waits in the lesson's review panel, with the reviewer's note and the part
//! of the lesson it is about, until the user keeps the text as written.
//!
//! A finding is identified by its kind and place (`FindingKey`), never its
//! wording. It is bound to the text of that part (`unit_sha256`): while the
//! text is unchanged the machine does not repair it again and a "keep as
//! written" stands; once the text changes, both lapse.
use std::collections::{BTreeSet, HashMap};

use lattice_core::shared::error::{AppError, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::{Row, SqlitePool};

use super::teaching_summary::{derive, passage_shown, Stored};
use crate::persistence::{db, now};
use crate::review_findings::{site_sha256, unit, FindingKey, FindingKind, FindingSite, SetAside};

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

/// What the user does with waiting or kept teaching findings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum TeachingSettleDto {
    /// Keep the text as written: waiting becomes kept.
    Keep,
    /// Take a kept finding back: kept becomes waiting.
    Reopen,
}

/// What the passage under a finding is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum TeachingPassageRoleDto {
    /// Text of the lesson, or a question's prompt.
    Lesson,
    /// The quotation of a source that a sentence or a part cites.
    Quotation,
}

/// A teaching-review finding the repairs did not settle.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct TeachingFindingDto {
    /// Names the finding in `settle_learning_teaching_findings`.
    pub finding_key: String,
    pub kind: TeachingFindingKindDto,
    pub site: TeachingFindingSiteDto,
    /// Zero-based section or question index; null for the whole lesson.
    pub unit_index: Option<u32>,
    /// "Section 3: Loops", "Question 2" or "Whole lesson".
    pub unit_title: String,
    /// What is wrong, in plain words.
    pub summary: String,
    /// The passage the finding is about, when one was chosen.
    pub passage: Option<String>,
    /// Whether the passage is the lesson's own text or a quotation it cites.
    pub passage_role: TeachingPassageRoleDto,
    /// Repairs that tried to settle it.
    pub repairs: u32,
    pub state: TeachingFindingStateDto,
    /// Whether the user changed this section after the note was written.
    pub part_changed: bool,
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
    pub(crate) async fn held(
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
    pub(crate) async fn record(
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
                 (lesson_id, finding_key, kind, site, unit_index, unit_title, unit_sha256, note, summary, passage, repairs, state, job_id, found_at, kept_at) \
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 'waiting', ?, ?, NULL) \
                 ON CONFLICT(lesson_id, finding_key) DO UPDATE SET \
                 unit_title=excluded.unit_title, note=excluded.note, summary=excluded.summary, passage=excluded.passage, \
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
            .bind((!finding.summary.is_empty()).then_some(finding.summary.as_str()))
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
            "SELECT finding_key, kind, site, unit_index, unit_title, note, summary, passage, repairs, state, \
             part_changed_at IS NOT NULL AND part_changed_at >= found_at AS part_changed \
             FROM learning_lesson_teaching_findings WHERE lesson_id=? \
             ORDER BY CASE site WHEN 'lesson' THEN 0 WHEN 'section' THEN 1 ELSE 2 END, unit_index, kind",
        )
        .bind(lesson_id)
        .fetch_all(&self.pool)
        .await
        .map_err(db)?;
        let sections = self.sections_for_old_rows(lesson_id, &rows).await?;
        rows.iter()
            .map(|row| {
                let corrupt = || AppError::InternalError("Invalid stored teaching finding".into());
                let site = site_of(&row.get::<String, _>("site"), row.get("unit_index"))
                    .ok_or_else(corrupt)?;
                let kind =
                    FindingKind::from_column(&row.get::<String, _>("kind")).ok_or_else(corrupt)?;
                let mut passage: Option<String> = row.get("passage");
                let summary = match row
                    .get::<Option<String>, _>("summary")
                    .filter(|summary| !summary.is_empty())
                {
                    Some(summary) => summary,
                    None => {
                        let note = row.get::<String, _>("note");
                        passage = passage_shown(kind, &note, passage);
                        derive(&Stored {
                            kind,
                            site,
                            note: &note,
                            section: match site {
                                FindingSite::Section(index) => {
                                    sections.get(&index).map(String::as_str)
                                }
                                _ => None,
                            },
                        })
                    }
                };
                Ok(TeachingFindingDto {
                    finding_key: row.get("finding_key"),
                    kind: kind_dto(kind),
                    site: match site {
                        FindingSite::Lesson => TeachingFindingSiteDto::Lesson,
                        FindingSite::Section(_) => TeachingFindingSiteDto::Section,
                        FindingSite::Question(_) => TeachingFindingSiteDto::Question,
                    },
                    unit_index: row
                        .get::<Option<i64>, _>("unit_index")
                        .and_then(|index| u32::try_from(index).ok()),
                    unit_title: row.get("unit_title"),
                    summary,
                    passage,
                    passage_role: if kind == FindingKind::Quotation {
                        TeachingPassageRoleDto::Quotation
                    } else {
                        TeachingPassageRoleDto::Lesson
                    },
                    repairs: u32::try_from(row.get::<i64, _>("repairs")).unwrap_or_default(),
                    state: if row.get::<String, _>("state") == "kept" {
                        TeachingFindingStateDto::Kept
                    } else {
                        TeachingFindingStateDto::Waiting
                    },
                    part_changed: row.get::<i64, _>("part_changed") != 0,
                })
            })
            .collect()
    }

    /// The published sections a finding with no summary may be about, by
    /// index; read only when such a finding quotes a sentence.
    async fn sections_for_old_rows(
        &self,
        lesson_id: &str,
        rows: &[sqlx::sqlite::SqliteRow],
    ) -> Result<HashMap<usize, String>> {
        let wanted = rows.iter().any(|row| {
            row.get::<Option<String>, _>("summary")
                .is_none_or(|summary| summary.is_empty())
                && row.get::<String, _>("kind") == FindingKind::Quotation.as_str()
                && row.get::<String, _>("site") == "section"
        });
        if !wanted {
            return Ok(HashMap::new());
        }
        let blocks = sqlx::query("SELECT ordinal, body FROM learning_blocks WHERE lesson_id=?")
            .bind(lesson_id)
            .fetch_all(&self.pool)
            .await
            .map_err(db)?;
        Ok(blocks
            .iter()
            .filter_map(|row| {
                Some((
                    usize::try_from(row.get::<i64, _>("ordinal")).ok()?,
                    row.get::<String, _>("body"),
                ))
            })
            .collect())
    }

    /// Keeps or reopens every finding in `finding_keys` in one transaction:
    /// all or nothing. Refused when the list is empty, the lesson is not in
    /// the course, or a finding is not in the state the change needs.
    pub async fn settle(
        &self,
        program_id: &str,
        lesson_id: &str,
        finding_keys: &[String],
        settle: TeachingSettleDto,
    ) -> Result<()> {
        let distinct: BTreeSet<&str> = finding_keys.iter().map(String::as_str).collect();
        if distinct.is_empty() {
            return Err(AppError::InvalidInput(
                "Select at least one teaching note.".into(),
            ));
        }
        let (from, to, kept_at) = match settle {
            TeachingSettleDto::Keep => ("waiting", "kept", Some(now())),
            TeachingSettleDto::Reopen => ("kept", "waiting", None),
        };
        let mut tx = self.pool.begin_with("BEGIN IMMEDIATE").await.map_err(db)?;
        let changed = sqlx::query(
            "UPDATE learning_lesson_teaching_findings SET state=?, kept_at=? \
             WHERE lesson_id=? AND state=? AND finding_key IN (SELECT value FROM json_each(?)) \
             AND EXISTS (SELECT 1 FROM learning_lessons WHERE id=? AND program_id=?)",
        )
        .bind(to)
        .bind(kept_at)
        .bind(lesson_id)
        .bind(from)
        .bind(serde_json::to_string(&distinct)?)
        .bind(lesson_id)
        .bind(program_id)
        .execute(&mut *tx)
        .await
        .map_err(db)?;
        if usize::try_from(changed.rows_affected()).ok() != Some(distinct.len()) {
            return Err(AppError::NotFound(
                "Some of these notes changed since you opened them. Reload the lesson.".into(),
            ));
        }
        tx.commit().await.map_err(db)
    }
}

#[cfg(test)]
#[path = "teaching_findings_tests.rs"]
mod tests;
