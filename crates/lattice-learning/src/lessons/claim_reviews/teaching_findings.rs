//! Teaching-review findings the repairs did not settle, in
//! `learning_lesson_teaching_findings`. The lesson publishes, and each finding
//! waits in the lesson's review panel, with the reviewer's note and the part
//! of the lesson it is about, until the user keeps the text as written.
//!
//! A finding is stored by its id (`FindingId`), never its wording. It is bound
//! to the text of that part (`unit_sha256`): while the text is unchanged the
//! machine does not repair it again and a "keep as written" stands; once the
//! text changes, both lapse. A reviewer's finding is stored with its number,
//! note, summary and passage: while its part is unchanged the machine does not
//! repair it again, a later job's first review asks the reviewer about it by
//! its number, and the number is not given to another finding. A row from
//! before finding ids is stored by its kind and place (`FindingKey`) and
//! stands for every finding of that kind at that place until the text
//! changes.
use std::collections::{BTreeSet, HashMap};

use lattice_core::shared::error::{AppError, Result};
use lattice_core::shared::persistence::Database;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::repository::blocks;
use super::repository::teaching_findings::{self as table, FindingWrite, Settle, StoredFinding};
use super::teaching_summary::{derive, passage_shown, sentence_of, Stored};
use crate::persistence::now;
use crate::review_findings::{
    site_sha256, unit, FindingId, FindingKey, FindingKind, FindingSite, Held, ReviewFinding,
    SetAside,
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
    /// The user took the sentence out or added a source for it.
    Resolved,
}

/// How a resolved teaching finding was resolved.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum TeachingResolutionDto {
    /// The user's removal of the sentence stands: worked out when the review is
    /// read, never stored.
    Removed,
    /// A source that supports the sentence was added.
    SourceAdded,
}

/// What the user does with waiting or kept teaching findings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum TeachingSettleDto {
    /// Keep the text as written: waiting becomes kept.
    Keep,
    /// Take a kept or resolved finding back: it becomes waiting.
    Reopen,
    /// A source was added for the sentence: waiting becomes resolved.
    SourceAdded,
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
    /// "Section 3 · Loops", "Question 2" or "The whole lesson".
    pub unit_title: String,
    /// What is wrong, in plain words.
    pub summary: String,
    /// The passage the finding is about, when one was chosen.
    pub passage: Option<String>,
    /// Whether the passage is the lesson's own text or a quotation it cites.
    pub passage_role: TeachingPassageRoleDto,
    /// The whole sentence a quotation finding is about, when it is about one.
    pub sentence: Option<String>,
    /// The live claims on exactly that sentence in the published section; for a
    /// note shown removed, the claims an undo of the user's removal takes back,
    /// none when that removal came with an edit of the whole section.
    pub claim_ids: Vec<String>,
    /// Whether "Remove this sentence" can take the note's sentence out of its
    /// published section.
    pub removable: bool,
    /// How a resolved finding was resolved.
    pub resolution: Option<TeachingResolutionDto>,
    /// When it was resolved.
    pub resolved_at: Option<i64>,
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
    database: Database,
}

impl TeachingFindingsRepository {
    pub fn new(database: Database) -> Self {
        Self { database }
    }

    /// The lesson's findings whose part still has the text they were found
    /// in: the machine does not repair these again.
    pub(crate) async fn held(&self, lesson_id: &str, candidate: &Value) -> Result<Held> {
        let rows = table::held_rows(&self.database, lesson_id).await?;
        let mut held = Held::default();
        for row in &rows {
            let id = FindingId::stored(row.finding_key.clone());
            let number = (row.identity == "finding")
                .then(|| id.reviewer_number())
                .flatten();
            if let Some(number) = number {
                held.next_number = held.next_number.max(number.saturating_add(1));
            }
            let Some(site) = site_of(&row.site, row.unit_index) else {
                continue;
            };
            if site_sha256(candidate, site) != row.unit_sha256 {
                continue;
            }
            if row.identity == "finding" {
                if let Some(number) = number {
                    let finding = ReviewFinding::reviewer(number, Some(site), row.note.clone())
                        .with_summary(row.summary.clone().unwrap_or_default())
                        .with_passage(row.passage.as_deref().unwrap_or_default())
                        .with_sentence(row.sentence.as_deref().unwrap_or_default());
                    held.reviewer.push((finding, row.unit_sha256.clone()));
                }
                held.ids.insert(id);
            } else if let Some(kind) = FindingKind::from_column(&row.kind) {
                held.places.insert(FindingKey { kind, site });
            }
        }
        held.reviewer
            .sort_by_key(|(finding, _)| finding.id().reviewer_number());
        Ok(held)
    }

    /// The text hashes of the lesson's parts whose findings wait for the user
    /// and end with their part, not those kept as written.
    pub(crate) async fn waiting_part_texts(&self, lesson_id: &str) -> Result<BTreeSet<String>> {
        Ok(table::waiting_parts(&self.database, lesson_id)
            .await?
            .into_iter()
            .filter(|row| {
                site_of(&row.site, row.unit_index).is_some_and(|site| {
                    FindingId::stored(row.finding_key.clone()).ends_with_its_part(site)
                })
            })
            .map(|row| row.unit_sha256)
            .collect())
    }

    /// Record findings for `candidate`. A finding kept as written stays kept
    /// while its part's text is unchanged; otherwise it waits again with the
    /// latest note. With `replace`, the findings not raised again were
    /// fixed and go. A row stored by place whose part's text is unchanged
    /// covers the findings of its kind there; once the text changes it gives
    /// way to the findings' own rows.
    pub(crate) async fn record(
        &self,
        lesson_id: &str,
        job_id: &str,
        candidate: &Value,
        findings: &[SetAside],
        replace: bool,
    ) -> Result<()> {
        let mut tx = self.database.begin_write().await?;
        let at = now();
        let places = table::place_rows(&mut tx, lesson_id).await?;
        let mut stay: Vec<String> = Vec::new();
        let mut lapsed: Vec<String> = Vec::new();
        for SetAside { finding, repairs } in findings {
            let shown = unit(candidate, finding);
            let (site, index) = site_columns(finding.place().site);
            let at_place = places.iter().find(|row| {
                row.kind == finding.kind.as_str() && row.site == site && row.unit_index == index
            });
            if let Some(row) = at_place {
                if row.unit_sha256 == shown.sha256 {
                    stay.push(row.finding_key.clone());
                    continue;
                }
                lapsed.push(row.finding_key.clone());
            }
            let id = finding.id();
            stay.push(id.as_str().to_owned());
            let write = FindingWrite {
                key: id.as_str(),
                kind: finding.kind.as_str(),
                site,
                unit_index: index,
                unit_title: shown.title.as_str(),
                unit_sha256: shown.sha256.as_str(),
                note: finding.note.as_str(),
                summary: (!finding.summary.is_empty()).then_some(finding.summary.as_str()),
                passage: finding.passage.as_deref().or(shown.passage.as_deref()),
                sentence: finding.sentence.as_deref(),
                repairs: i64::from(*repairs),
                job_id,
                found_at: at,
            };
            table::upsert(&mut tx, lesson_id, &write).await?;
        }
        if replace {
            table::delete_not_raised(&mut tx, lesson_id, &serde_json::to_string(&stay)?).await?;
        } else if !lapsed.is_empty() {
            table::delete_keys(&mut tx, lesson_id, &serde_json::to_string(&lapsed)?).await?;
        }
        tx.commit().await.map_err(AppError::from)
    }

    /// The lesson's findings: by place, then kind.
    pub async fn list(&self, lesson_id: &str) -> Result<Vec<TeachingFindingDto>> {
        let rows = table::stored_findings(&self.database, lesson_id).await?;
        let sections = self.sections_for_old_rows(lesson_id, &rows).await?;
        rows.iter()
            .map(|row| {
                let corrupt = || AppError::InternalError("Invalid stored teaching finding".into());
                let site = site_of(&row.site, row.unit_index).ok_or_else(corrupt)?;
                let kind = FindingKind::from_column(&row.kind).ok_or_else(corrupt)?;
                let mut passage = row.passage.clone();
                let mut sentence = row.sentence.clone();
                let summary = match row.summary.clone().filter(|summary| !summary.is_empty()) {
                    Some(summary) => summary,
                    None => {
                        let note = row.note.clone();
                        passage = passage_shown(kind, &note, passage);
                        let stored = Stored {
                            kind,
                            site,
                            note: &note,
                            section: match site {
                                FindingSite::Section(index) => {
                                    sections.get(&index).map(String::as_str)
                                }
                                _ => None,
                            },
                        };
                        sentence = sentence.or_else(|| sentence_of(&stored));
                        derive(&stored)
                    }
                };
                Ok(TeachingFindingDto {
                    finding_key: row.finding_key.clone(),
                    kind: kind_dto(kind),
                    site: match site {
                        FindingSite::Lesson => TeachingFindingSiteDto::Lesson,
                        FindingSite::Section(_) => TeachingFindingSiteDto::Section,
                        FindingSite::Question(_) => TeachingFindingSiteDto::Question,
                    },
                    unit_index: row.unit_index.and_then(|index| u32::try_from(index).ok()),
                    unit_title: row.unit_title.clone(),
                    sentence,
                    claim_ids: Vec::new(),
                    removable: false,
                    resolution: match row.resolution.as_deref() {
                        Some("source_added") => Some(TeachingResolutionDto::SourceAdded),
                        _ => None,
                    },
                    resolved_at: row.resolved_at,
                    summary,
                    passage,
                    passage_role: if kind == FindingKind::Quotation {
                        TeachingPassageRoleDto::Quotation
                    } else {
                        TeachingPassageRoleDto::Lesson
                    },
                    repairs: u32::try_from(row.repairs).unwrap_or_default(),
                    state: match row.state.as_str() {
                        "kept" => TeachingFindingStateDto::Kept,
                        "resolved" => TeachingFindingStateDto::Resolved,
                        _ => TeachingFindingStateDto::Waiting,
                    },
                    part_changed: row.part_changed,
                })
            })
            .collect()
    }

    /// The published sections a finding with no summary may be about, by
    /// index; read only when such a finding quotes a sentence.
    async fn sections_for_old_rows(
        &self,
        lesson_id: &str,
        rows: &[StoredFinding],
    ) -> Result<HashMap<usize, String>> {
        let wanted = rows.iter().any(|row| {
            row.summary
                .as_ref()
                .is_none_or(|summary| summary.is_empty())
                && row.kind == FindingKind::Quotation.as_str()
                && row.site == "section"
        });
        if !wanted {
            return Ok(HashMap::new());
        }
        blocks::section_bodies(&self.database, lesson_id).await
    }

    /// Keeps, resolves or reopens every finding in `finding_keys` in one
    /// transaction: all or nothing. Refused when the list is empty, the lesson is not in
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
        let at = now();
        let (from, to, kept_at, resolution, resolved_at) = match settle {
            TeachingSettleDto::Keep => (&["waiting"][..], "kept", Some(at), None, None),
            TeachingSettleDto::SourceAdded => (
                &["waiting"][..],
                "resolved",
                None,
                Some("source_added"),
                Some(at),
            ),
            TeachingSettleDto::Reopen => (&["kept", "resolved"][..], "waiting", None, None, None),
        };
        let mut tx = self.database.begin_write().await?;
        let change = Settle {
            from,
            to,
            kept_at,
            resolution,
            resolved_at,
        };
        let changed = table::settle(&mut tx, program_id, lesson_id, &distinct, &change).await?;
        if usize::try_from(changed).ok() != Some(distinct.len()) {
            return Err(AppError::NotFound(
                "Some of these notes changed since you opened them. Reload the lesson.".into(),
            ));
        }
        tx.commit().await.map_err(AppError::from)
    }
}

#[cfg(test)]
#[path = "teaching_findings_tests.rs"]
mod tests;
