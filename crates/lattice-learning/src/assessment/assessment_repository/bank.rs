//! The course's item bank: what it holds per quiz requirement, what each quiz
//! still needs written, and storing what was written.
use super::{
    candidate_from_row, candidate_outcomes_in, decode, format_name, json,
    LearningAssessmentRepository, StoredCandidate, CANDIDATE_COLUMNS,
};
use crate::assessment_engine::{self as engine, LearningBlueprintRequirement, Shortfall};
use crate::assessment_generation::{AuthoringNeed, BankPass, MaterialPassage, StoredItem};
use crate::dto::{LearningQuizScopeDto, LearningQuizScopeKind};
use crate::lessons::claim_reviews::repository::waiting_questions_in;
use crate::persistence::now;
use crate::quiz_blueprints;
use lattice_core::shared::error::Result;
use lattice_core::shared::hash::sha256_hex;
use lattice_core::shared::persistence::{Database, WriteTx};
use sha2::{Digest, Sha256};
use sqlx::{Row, SqliteConnection};
use std::collections::{HashMap, HashSet};

/// An item on its way into the bank.
#[derive(Debug, Clone)]
pub struct BankItem {
    pub item: StoredItem,
    pub source_quote: String,
    pub lesson_id: Option<String>,
    /// `lesson` for the lesson's own reviewed questions, `bank` for items
    /// written for a quiz.
    pub origin: &'static str,
    /// `agreed` for items forms may draw, `dropped` for a lesson question the
    /// bank's checks reject, kept so the lesson's questions keep their places.
    pub review_status: &'static str,
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

/// Store items, skipping any generated item the bank already holds (same
/// content). A lesson's own
/// question is always stored in its place: one whose content the program
/// already holds is stored `dropped`, its token salted by its ID. Returns how
/// many were stored.
pub(crate) async fn insert_bank_items_in(
    tx: &mut WriteTx,
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
        let mut hash = content_hash(item);
        let mut review_status = entry.review_status;
        let id_taken: Option<i64> = sqlx::query_scalar(
            "SELECT 1 FROM learning_assessment_candidates WHERE program_id=? AND id=?",
        )
        .bind(program_id)
        .bind(&item.id)
        .fetch_optional(&mut **tx)
        .await?;
        if id_taken.is_some() {
            continue;
        }
        let repeated: Option<i64> = sqlx::query_scalar(
            "SELECT 1 FROM learning_assessment_candidates WHERE program_id=? AND content_sha256=?",
        )
        .bind(program_id)
        .bind(&hash)
        .fetch_optional(&mut **tx)
        .await?;
        if repeated.is_some() {
            if entry.origin != "lesson" {
                continue;
            }
            hash = sha256_hex(format!("{hash}\0{}", item.id));
            review_status = "dropped";
        }
        sqlx::query("INSERT INTO learning_assessment_candidates(id,program_id,outcome_id,format,difficulty,prompt,options_json,artifact_kind,rubric_json,source_version_ids_json,authored_by,author_model,content_sha256,created_at,review_status,origin,lesson_id,source_quote,left_items_json) VALUES(?,?,?,?,?,?,?,NULL,?,?,'model',?,?,?,?,?,?,?,?)")
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
            .bind(review_status)
            .bind(entry.origin)
            .bind(&entry.lesson_id)
            .bind(&entry.source_quote)
            .bind(json(&item.left)?)
            .execute(&mut **tx)
            .await?;
        for outcome in &item.outcome_ids {
            sqlx::query("INSERT OR IGNORE INTO learning_assessment_candidate_outcomes(candidate_id,outcome_id) VALUES(?,?)")
                .bind(&item.id)
                .bind(outcome)
                .execute(&mut **tx)
                .await?;
        }
        sqlx::query("INSERT INTO learning_assessment_answer_keys(candidate_id,explanation,key_json,distractor_rationales_json) VALUES(?,?,?,?)")
            .bind(&item.id)
            .bind(&item.explanation)
            .bind(json(&item.key)?)
            .bind(json(&item.distractor_rationales)?)
            .execute(&mut **tx)
            .await?;
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
    /// The first requirement a form cannot fill; None when a form can be built or there are no requirements.
    pub shortfall: Option<Shortfall>,
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
    let shortfall = engine::form_items(
        requirements,
        &bank.iter().map(|c| &c.candidate).collect::<Vec<_>>(),
        exposed,
    )
    .err();
    BankState {
        ready,
        shortfall,
        size: all.len(),
        fresh: fresh.len(),
        target: requirements.iter().map(|r| r.bank_target()).sum(),
        per_requirement,
    }
}

impl LearningAssessmentRepository {
    /// The bank's agreed items: what forms may draw from, less a lesson question
    /// whose check finding still waits for the user.
    pub(crate) async fn bank_in(
        connection: &mut SqliteConnection,
        program_id: &str,
    ) -> Result<Vec<StoredCandidate>> {
        let outcomes = candidate_outcomes_in(connection, program_id).await?;
        let rows = sqlx::query(sqlx::AssertSqlSafe(format!("SELECT {CANDIDATE_COLUMNS} FROM learning_assessment_candidates c JOIN learning_assessment_answer_keys k ON k.candidate_id=c.id WHERE c.program_id=? AND c.review_status='agreed' ORDER BY c.id")))
            .bind(program_id)
            .fetch_all(&mut *connection)
            .await?;
        let waiting = waiting_questions_in(connection, program_id).await?;
        let mut held_back = HashSet::new();
        if !waiting.is_empty() {
            let own: Vec<(String, String)> = sqlx::query_as("SELECT id, lesson_id FROM learning_assessment_candidates WHERE program_id=? AND origin IN ('lesson','migrated') AND lesson_id IS NOT NULL ORDER BY rowid")
                .bind(program_id)
                .fetch_all(&mut *connection)
                .await?;
            let mut seen: HashMap<String, usize> = HashMap::new();
            for (id, lesson_id) in own {
                let index = seen.entry(lesson_id.clone()).or_insert(0);
                if waiting.contains(&(lesson_id, *index)) {
                    held_back.insert(id);
                }
                *index += 1;
            }
        }
        rows.iter()
            .filter(|row| !held_back.contains(&row.get::<String, _>("id")))
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
            .await?
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
            .await?
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

    /// A lesson's own reviewed questions that are in the bank.
    pub(crate) async fn lesson_seeds_in(
        connection: &mut SqliteConnection,
        program_id: &str,
        lesson_id: &str,
    ) -> Result<Vec<StoredItem>> {
        let rows = sqlx::query(sqlx::AssertSqlSafe(format!("SELECT {CANDIDATE_COLUMNS} FROM learning_assessment_candidates c JOIN learning_assessment_answer_keys k ON k.candidate_id=c.id WHERE c.program_id=? AND c.lesson_id=? AND c.origin='lesson' AND c.review_status='agreed' ORDER BY c.id")))
            .bind(program_id)
            .bind(lesson_id)
            .fetch_all(&mut *connection)
            .await?;
        rows.iter()
            .map(|row| candidate_from_row(row, vec![]).map(|c| c.candidate.into()))
            .collect()
    }

    /// [`Self::bank_needs_in`] as a unit of work of its own: working out a
    /// need stores the outcomes and the blueprint revision it reads.
    pub(crate) async fn bank_needs(
        database: &Database,
        program_id: &str,
        scope: &LearningQuizScopeDto,
        top_up: bool,
    ) -> Result<Vec<AuthoringNeed>> {
        let mut tx = database.begin_write().await?;
        let needs = Self::bank_needs_in(&mut tx, program_id, scope, top_up).await?;
        tx.commit().await?;
        Ok(needs)
    }

    /// The first requirement a quiz's first form cannot fill from the bank, with the items that could fill it;
    /// None when a form can be built or the quiz has no requirements yet.
    pub async fn first_form_shortfall(
        database: &Database,
        program_id: &str,
        scope: &LearningQuizScopeDto,
    ) -> Result<Option<Shortfall>> {
        let mut tx = database.begin_write().await?;
        Self::ensure_outcomes(&mut tx, program_id).await?;
        let Some((blueprint, _)) =
            quiz_blueprints::ensure_blueprint_in(&mut tx, program_id, scope).await?
        else {
            tx.commit().await?;
            return Ok(None);
        };
        let requirements = quiz_blueprints::requirements_in(&mut tx, &blueprint).await?;
        let bank = Self::bank_in(&mut tx, program_id).await?;
        tx.commit().await?;
        Ok(bank_state(&requirements, &bank, &HashSet::new()).shortfall)
    }

    /// [`insert_bank_items_in`] as a unit of work of its own.
    pub(crate) async fn insert_bank_items(
        database: &Database,
        program_id: &str,
        items: &[BankItem],
        author_model: Option<&str>,
    ) -> Result<usize> {
        let mut tx = database.begin_write().await?;
        let stored = insert_bank_items_in(&mut tx, program_id, items, author_model).await?;
        tx.commit().await?;
        Ok(stored)
    }

    /// What a quiz still needs written: per requirement, up to three forms'
    /// worth, or with `top_up` one more form's worth wherever fewer unseen
    /// items remain than a form asks. Empty when the quiz has nothing to ask
    /// yet or its bank is full.
    pub(crate) async fn bank_needs_in(
        tx: &mut WriteTx,
        program_id: &str,
        scope: &LearningQuizScopeDto,
        top_up: bool,
    ) -> Result<Vec<AuthoringNeed>> {
        Self::ensure_outcomes(tx, program_id).await?;
        let Some((blueprint, _)) =
            quiz_blueprints::ensure_blueprint_in(tx, program_id, scope).await?
        else {
            return Ok(vec![]);
        };
        let requirements = quiz_blueprints::requirements_in(tx, &blueprint).await?;
        let bank = Self::bank_in(tx, program_id).await?;
        let exposed = Self::exposed_in(tx, program_id).await?;
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
                requirement.bank_target().saturating_sub(eligible)
            };
            if count == 0 {
                continue;
            }
            let outcome = sqlx::query("SELECT title,description,module_id,lesson_id FROM learning_outcome_definitions WHERE id=? AND program_id=?")
                .bind(&requirement.outcome_id)
                .bind(program_id)
                .fetch_one(&mut **tx)
                .await?;
            let lesson_id: Option<String> = outcome.get("lesson_id");
            let lessons: Vec<String> = match (&lesson_id, outcome.get::<Option<String>, _>("module_id")) {
                (Some(lesson), _) => vec![lesson.clone()],
                (None, Some(module)) => sqlx::query_scalar("SELECT id FROM learning_lessons WHERE program_id=? AND module_id=? AND preparation='ready' ORDER BY ordinal")
                    .bind(program_id)
                    .bind(module)
                    .fetch_all(&mut **tx)
                    .await?,
                (None, None) => vec![],
            };
            let mut material = Vec::new();
            for lesson in &lessons {
                material.extend(Self::lesson_material_in(tx, lesson).await?);
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
                pass: BankPass::Retakes,
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

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::indexing_slicing)]
    use super::*;
    use crate::assessment_engine::{
        build_assessment_form, LearningAssessmentBlueprint, LearningAssessmentCandidate,
        LearningAssessmentKey, LearningAssessmentPurpose, LearningFeedbackTiming,
        LearningItemFormat,
    };
    use lattice_core::shared::error::AppError;

    fn requirement(outcome: u128, count: usize) -> LearningBlueprintRequirement {
        LearningBlueprintRequirement {
            outcome_id: uuid::Uuid::from_u128(outcome).to_string(),
            format: LearningItemFormat::SingleChoice,
            count,
            difficulty_min: 2,
            difficulty_max: 4,
        }
    }

    fn stored(seed: u128) -> StoredCandidate {
        StoredCandidate {
            candidate: LearningAssessmentCandidate {
                id: uuid::Uuid::from_u128(seed).to_string(),
                outcome_ids: vec![
                    uuid::Uuid::from_u128(2).to_string(),
                    uuid::Uuid::from_u128(3).to_string(),
                ],
                format: LearningItemFormat::SingleChoice,
                difficulty: 3,
                prompt: format!("Which shelf takes parcel {seed}?"),
                options: vec!["Left".into(), "Middle".into(), "Right".into()],
                left: vec![],
                source_version_ids: vec![],
                rubric: vec![],
                key: LearningAssessmentKey::Choice(1),
                explanation: "The middle shelf.".into(),
                distractor_rationales: vec![],
            },
            artifact_kind: None,
            source_quote: String::new(),
        }
    }

    #[test]
    fn a_bank_is_ready_only_when_a_form_can_be_built_from_it() {
        let requirements = vec![requirement(2, 2), requirement(3, 1)];
        let bank = vec![stored(10), stored(11)];
        let blueprint = LearningAssessmentBlueprint {
            id: uuid::Uuid::from_u128(1).to_string(),
            purpose: LearningAssessmentPurpose::Checkpoint,
            title: "Parcels".into(),
            instructions: "Answer each question.".into(),
            expected_minutes: 5,
            allowed_aids: vec![],
            feedback_timing: LearningFeedbackTiming::AfterSubmission,
            requirements: requirements.clone(),
        };
        let candidates: Vec<LearningAssessmentCandidate> =
            bank.iter().map(|c| c.candidate.clone()).collect();

        let state = bank_state(&requirements, &bank, &HashSet::new());
        let form = build_assessment_form(
            &blueprint,
            &candidates,
            &HashSet::new(),
            uuid::Uuid::new_v4().to_string(),
            0,
        );

        assert!(
            matches!(form, Err(AppError::InvalidState(_))),
            "the form was built or refused for another reason: {form:?}"
        );
        assert!(!state.ready, "{state:?}");
    }
}
