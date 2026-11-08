//! The course's item bank: what it holds per quiz requirement, what each quiz
//! still needs written, and storing what was written.
use super::{
    candidate_from_row, candidate_outcomes_in, decode, format_name, json,
    LearningAssessmentRepository, StoredCandidate, CANDIDATE_COLUMNS,
};
use crate::features::learning::assessment_engine::{self as engine, LearningBlueprintRequirement};
use crate::features::learning::assessment_generation::{
    AuthoringNeed, MaterialPassage, StoredItem,
};
use crate::features::learning::dto::{LearningQuizScopeDto, LearningQuizScopeKind};
use crate::features::learning::persistence::{db, now};
use crate::features::learning::quiz_blueprints;
use lattice_core::shared::error::Result;
use sha2::{Digest, Sha256};
use sqlx::{Row, SqliteConnection};
use std::collections::HashSet;

/// An item on its way into the bank.
#[derive(Debug, Clone)]
pub struct BankItem {
    pub item: StoredItem,
    pub source_quote: String,
    pub lesson_id: Option<String>,
    /// `lesson` for the lesson's own reviewed questions, `bank` for items
    /// written for a quiz.
    pub origin: &'static str,
}

/// A stable content token: same format, stem and options, same item.
pub(crate) fn content_hash(item: &StoredItem) -> String {
    let mut hasher = Sha256::new();
    hasher.update(format_name(item.format));
    hasher.update([0]);
    hasher.update(engine::normalize_answer(&item.prompt));
    for option in item.left.iter().chain(&item.options) {
        hasher.update([0]);
        hasher.update(engine::normalize_answer(option));
    }
    format!("{:x}", hasher.finalize())
}

/// Store items, skipping any the bank already holds (same content, or a
/// near-duplicate stem for the same outcome). Returns how many were stored.
pub(crate) async fn insert_bank_items_in(
    connection: &mut SqliteConnection,
    program_id: &str,
    items: &[BankItem],
    author_model: Option<&str>,
) -> Result<usize> {
    let mut stored = 0;
    for entry in items {
        let item = &entry.item;
        let Some(outcome) = item.outcome_ids.first() else {
            continue;
        };
        let hash = content_hash(item);
        let exists: Option<i64> = sqlx::query_scalar(
            "SELECT 1 FROM learning_assessment_candidates WHERE program_id=? AND (content_sha256=? OR id=?)",
        )
        .bind(program_id)
        .bind(&hash)
        .bind(&item.id)
        .fetch_optional(&mut *connection)
        .await
        .map_err(db)?;
        if exists.is_some() {
            continue;
        }
        let near = sqlx::query_scalar::<_, String>(
            "SELECT prompt FROM learning_assessment_candidates WHERE program_id=? AND outcome_id=? AND review_status<>'dropped'",
        )
        .bind(program_id)
        .bind(outcome)
        .fetch_all(&mut *connection)
        .await
        .map_err(db)?
        .iter()
        .any(|prompt| engine::near_duplicate(prompt, &item.prompt));
        if near {
            continue;
        }
        sqlx::query("INSERT INTO learning_assessment_candidates(id,program_id,outcome_id,format,difficulty,prompt,options_json,artifact_kind,rubric_json,source_version_ids_json,authored_by,author_model,content_sha256,created_at,review_status,origin,lesson_id,source_quote,left_items_json) VALUES(?,?,?,?,?,?,?,NULL,?,?,'model',?,?,?,'agreed',?,?,?,?)")
            .bind(&item.id)
            .bind(program_id)
            .bind(outcome)
            .bind(format_name(item.format))
            .bind(i64::from(item.difficulty))
            .bind(&item.prompt)
            .bind(json(&item.options)?)
            .bind(json(&item.rubric)?)
            .bind(json(&item.source_version_ids)?)
            .bind(author_model)
            .bind(&hash)
            .bind(now())
            .bind(entry.origin)
            .bind(&entry.lesson_id)
            .bind(&entry.source_quote)
            .bind(json(&item.left)?)
            .execute(&mut *connection)
            .await
            .map_err(db)?;
        for outcome in &item.outcome_ids {
            sqlx::query("INSERT OR IGNORE INTO learning_assessment_candidate_outcomes(candidate_id,outcome_id) VALUES(?,?)")
                .bind(&item.id)
                .bind(outcome)
                .execute(&mut *connection)
                .await
                .map_err(db)?;
        }
        sqlx::query("INSERT INTO learning_assessment_answer_keys(candidate_id,explanation,key_json,distractor_rationales_json) VALUES(?,?,?,?)")
            .bind(&item.id)
            .bind(&item.explanation)
            .bind(json(&item.key)?)
            .bind(json(&item.distractor_rationales)?)
            .execute(&mut *connection)
            .await
            .map_err(db)?;
        stored += 1;
    }
    Ok(stored)
}

/// How a quiz's requirements stand in the bank.
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct BankState {
    /// Every requirement can be filled: a form can be built.
    pub ready: bool,
    pub size: usize,
    pub fresh: usize,
    /// Three forms' worth (section 5.2).
    pub target: usize,
    /// Per requirement: items that can fill it, and how many of those are unseen.
    pub per_requirement: Vec<(usize, usize)>,
}

pub(crate) fn bank_state(
    requirements: &[LearningBlueprintRequirement],
    bank: &[StoredCandidate],
    exposed: &HashSet<String>,
) -> BankState {
    let mut all = HashSet::new();
    let mut fresh = HashSet::new();
    let mut per_requirement = Vec::new();
    let mut ready = !requirements.is_empty();
    for requirement in requirements {
        let eligible: Vec<&StoredCandidate> = bank
            .iter()
            .filter(|c| engine::eligible_for(&c.candidate, requirement))
            .collect();
        let unseen = eligible
            .iter()
            .filter(|c| !exposed.contains(&c.candidate.id))
            .count();
        ready &= eligible.len() >= requirement.count;
        for c in &eligible {
            all.insert(c.candidate.id.clone());
            if !exposed.contains(&c.candidate.id) {
                fresh.insert(c.candidate.id.clone());
            }
        }
        per_requirement.push((eligible.len(), unseen));
    }
    BankState {
        ready,
        size: all.len(),
        fresh: fresh.len(),
        target: requirements.iter().map(|r| 3 * r.count).sum(),
        per_requirement,
    }
}

impl LearningAssessmentRepository {
    /// The bank's agreed items: what forms may draw from.
    pub(crate) async fn bank_in(
        connection: &mut SqliteConnection,
        program_id: &str,
    ) -> Result<Vec<StoredCandidate>> {
        let outcomes = candidate_outcomes_in(connection, program_id).await?;
        let rows = sqlx::query(&format!("SELECT {CANDIDATE_COLUMNS} FROM learning_assessment_candidates c JOIN learning_assessment_answer_keys k ON k.candidate_id=c.id WHERE c.program_id=? AND c.review_status='agreed' ORDER BY c.id"))
            .bind(program_id)
            .fetch_all(&mut *connection)
            .await
            .map_err(db)?;
        rows.iter()
            .map(|row| {
                let id: String = row.get("id");
                candidate_from_row(row, outcomes.get(&id).cloned().unwrap_or_default())
            })
            .collect()
    }

    /// Items the learner has already seen in a submitted or paused form.
    pub(crate) async fn exposed_in(
        connection: &mut SqliteConnection,
        program_id: &str,
    ) -> Result<HashSet<String>> {
        Ok(sqlx::query_scalar::<_, String>("SELECT i.candidate_id FROM learning_assessment_form_items i JOIN learning_assessment_forms f ON f.id=i.form_id WHERE f.program_id=? AND f.status IN ('submitted','interrupted')")
            .bind(program_id)
            .fetch_all(&mut *connection)
            .await
            .map_err(db)?
            .into_iter()
            .collect())
    }

    /// A lesson's blocks as authoring material.
    pub(crate) async fn lesson_material_in(
        connection: &mut SqliteConnection,
        lesson_id: &str,
    ) -> Result<Vec<MaterialPassage>> {
        sqlx::query("SELECT b.title,b.body,b.source_ids_json,l.title lesson_title FROM learning_blocks b JOIN learning_lessons l ON l.id=b.lesson_id WHERE b.lesson_id=? ORDER BY b.ordinal")
            .bind(lesson_id)
            .fetch_all(&mut *connection)
            .await
            .map_err(db)?
            .into_iter()
            .map(|row| {
                Ok(MaterialPassage {
                    lesson_title: row.get("lesson_title"),
                    title: row.get("title"),
                    text: row.get("body"),
                    source_version_ids: decode(row.get("source_ids_json"))?,
                })
            })
            .filter(|p: &Result<MaterialPassage>| p.as_ref().map_or(true, |p| !p.text.trim().is_empty()))
            .collect()
    }

    /// What a quiz still needs written: per requirement, up to three forms'
    /// worth, or with `top_up` one more form's worth wherever fewer unseen
    /// items remain than a form asks. Empty when the quiz has nothing to ask
    /// yet or its bank is full.
    pub(crate) async fn bank_needs_in(
        connection: &mut SqliteConnection,
        program_id: &str,
        scope: &LearningQuizScopeDto,
        top_up: bool,
    ) -> Result<Vec<AuthoringNeed>> {
        Self::ensure_outcomes(connection, program_id).await?;
        let Some((blueprint, _)) =
            quiz_blueprints::ensure_blueprint_in(connection, program_id, scope).await?
        else {
            return Ok(vec![]);
        };
        let requirements = quiz_blueprints::requirements_in(connection, &blueprint).await?;
        let bank = Self::bank_in(connection, program_id).await?;
        let exposed = Self::exposed_in(connection, program_id).await?;
        let state = bank_state(&requirements, &bank, &exposed);
        let mut needs = Vec::new();
        for (requirement, (eligible, unseen)) in requirements.iter().zip(state.per_requirement) {
            let count = if top_up {
                if unseen < requirement.count {
                    requirement.count
                } else {
                    0
                }
            } else {
                (3 * requirement.count).saturating_sub(eligible)
            };
            if count == 0 {
                continue;
            }
            let outcome = sqlx::query("SELECT title,description,module_id,lesson_id FROM learning_outcome_definitions WHERE id=? AND program_id=?")
                .bind(&requirement.outcome_id)
                .bind(program_id)
                .fetch_one(&mut *connection)
                .await
                .map_err(db)?;
            let lesson_id: Option<String> = outcome.get("lesson_id");
            let lessons: Vec<String> = match (&lesson_id, outcome.get::<Option<String>, _>("module_id")) {
                (Some(lesson), _) => vec![lesson.clone()],
                (None, Some(module)) => sqlx::query_scalar("SELECT id FROM learning_lessons WHERE program_id=? AND module_id=? AND preparation='ready' ORDER BY ordinal")
                    .bind(program_id)
                    .bind(module)
                    .fetch_all(&mut *connection)
                    .await
                    .map_err(db)?,
                (None, None) => vec![],
            };
            let mut material = Vec::new();
            for lesson in &lessons {
                material.extend(Self::lesson_material_in(connection, lesson).await?);
            }
            if material.is_empty() {
                continue;
            }
            let existing_prompts = bank
                .iter()
                .filter(|c| c.candidate.outcome_ids.contains(&requirement.outcome_id))
                .map(|c| c.candidate.prompt.clone())
                .collect();
            needs.push(AuthoringNeed {
                requirement: requirement.clone(),
                outcome_title: outcome.get("title"),
                outcome_description: outcome.get("description"),
                lesson_id,
                count,
                material,
                existing_prompts,
            });
        }
        // Module and course banks reuse lesson items; only what is missing is
        // written, and the lesson scope's own needs come first.
        if scope.kind != LearningQuizScopeKind::Lesson {
            needs.sort_by_key(|n| n.lesson_id.is_none());
        }
        Ok(needs)
    }
}
