//! Which questions each quiz asks: blueprints derived from the course, never
//! authored by the renderer (docs/design/2026-10-23-quizzes-and-tests.md,
//! sections 5.1 and 5.3).
//!
//! `blueprint_for` is a pure function of the scope's prepared lessons, their
//! outcomes and the course's quiz settings. `ensure_blueprint_in` stores its
//! result as a new revision only when it differs from the latest one, so old
//! forms keep the revision they were drawn from.
use crate::features::learning::assessment_engine::{
    LearningAssessmentPurpose, LearningBlueprintRequirement, LearningItemFormat,
};
use crate::features::learning::dto::{
    LearningQuizScopeDto, LearningQuizScopeKind, LearningQuizSettingsDto,
};
use crate::features::learning::persistence::{db, decode, encode, now};
use lattice_core::shared::error::{AppError, Result};
use sqlx::{Row, SqliteConnection};
use std::collections::{BTreeSet, HashSet};

/// A prepared or outline lesson, as a blueprint sees it.
#[derive(Debug, Clone, PartialEq)]
pub struct LessonFacts {
    pub id: String,
    pub module_id: String,
    pub title: String,
    pub objective: String,
    pub prepared: bool,
    /// The lesson-level outcome; every prepared lesson has one.
    pub outcome_id: Option<String>,
    pub source_version_ids: Vec<String>,
    /// The formats the lesson's latest quiz blueprint asks; empty before its
    /// first. A copied course carries its blueprints and banks under fresh
    /// lesson IDs, so the mix already stored is kept rather than picked again.
    pub quiz_formats: Vec<LearningItemFormat>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ModuleFacts {
    pub id: String,
    pub title: String,
    /// The module's own outcomes, in order.
    pub outcome_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CourseFacts {
    pub program_title: String,
    pub modules: Vec<ModuleFacts>,
    pub lessons: Vec<LessonFacts>,
}

/// One requirement and the points each of its items is worth.
#[derive(Debug, Clone, PartialEq)]
pub struct Slot {
    pub requirement: LearningBlueprintRequirement,
    /// 1.0 for keyed items. Written answers are worth their rubric's total,
    /// which is known only per item, so their slot records 1.0 and the form
    /// takes the item's rubric total.
    pub points: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DesiredBlueprint {
    pub purpose: LearningAssessmentPurpose,
    pub title: String,
    pub instructions: String,
    pub expected_minutes: u32,
    pub passing_score: f64,
    pub slots: Vec<Slot>,
    pub source_version_ids: Vec<String>,
    /// For module tests and the exam: prepared lessons covered, of all.
    pub coverage: Option<(usize, usize)>,
}

pub fn default_settings() -> LearningQuizSettingsDto {
    LearningQuizSettingsDto {
        quiz_pass: 0.7,
        test_pass: 0.75,
        exam_pass: 0.75,
        written_answers: true,
    }
}

/// A stable choice from the lesson ID, so a lesson's first quiz mix never
/// changes between preparations; once stored, the blueprint keeps it.
fn stable_pick(seed: &str, choices: usize) -> usize {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in seed.bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    (hash % choices.max(1) as u64) as usize
}

fn requirement(
    outcome_id: &str,
    format: LearningItemFormat,
    count: usize,
    min: u8,
    max: u8,
) -> Slot {
    Slot {
        requirement: LearningBlueprintRequirement {
            outcome_id: outcome_id.to_string(),
            format,
            count,
            difficulty_min: min,
            difficulty_max: max,
        },
        points: 1.0,
    }
}

fn minutes_for(format: LearningItemFormat) -> u32 {
    match format {
        LearningItemFormat::SingleChoice | LearningItemFormat::TrueFalse => 1,
        LearningItemFormat::Explanation | LearningItemFormat::Artifact => 6,
        _ => 2,
    }
}

fn bounded_title(prefix: &str, title: &str) -> String {
    format!("{prefix}: {}", title.trim())
        .chars()
        .take(160)
        .collect()
}

/// The blueprint a scope should have now, or None when it has nothing to ask
/// yet (an outline lesson; a module or course with no prepared lesson).
pub fn blueprint_for(
    scope: &LearningQuizScopeDto,
    facts: &CourseFacts,
    settings: &LearningQuizSettingsDto,
) -> Option<DesiredBlueprint> {
    let prepared = |lesson: &&LessonFacts| lesson.prepared && lesson.outcome_id.is_some();
    let mut slots = Vec::new();
    let (purpose, title, passing, scoped): (_, _, _, Vec<&LessonFacts>) = match scope.kind {
        LearningQuizScopeKind::Lesson => {
            let lesson = facts
                .lessons
                .iter()
                .find(|l| Some(&l.id) == scope.id.as_ref())
                .filter(prepared)?;
            let outcome = lesson.outcome_id.as_deref()?;
            let structured = [
                LearningItemFormat::MultiSelect,
                LearningItemFormat::Matching,
                LearningItemFormat::Ordering,
            ];
            let typed = [LearningItemFormat::ShortAnswer, LearningItemFormat::Cloze];
            let pick = |choices: &[LearningItemFormat], salt: &str| {
                choices
                    .iter()
                    .copied()
                    .find(|format| lesson.quiz_formats.contains(format))
                    .or_else(|| {
                        choices
                            .get(stable_pick(&format!("{}:{salt}", lesson.id), choices.len()))
                            .copied()
                    })
                    .unwrap_or(LearningItemFormat::SingleChoice)
            };
            if lesson.quiz_formats.contains(&LearningItemFormat::TrueFalse) {
                // A lesson whose blueprint already asks the earlier mix keeps
                // it, so its bank (or a copied course's) is not written again.
                slots.push(requirement(
                    outcome,
                    LearningItemFormat::SingleChoice,
                    4,
                    2,
                    3,
                ));
                slots.push(requirement(outcome, LearningItemFormat::TrueFalse, 1, 2, 2));
                slots.push(requirement(
                    outcome,
                    pick(&structured, "structured"),
                    1,
                    3,
                    3,
                ));
                slots.push(requirement(outcome, pick(&typed, "typed"), 1, 3, 3));
            } else {
                // Recall over recognition: an answer produced from memory, with
                // feedback after it, is retained better than one picked from
                // options (Kang, McDermott & Roediger 2007; McDaniel, Roediger
                // & McDermott 2007). Both typed formats replace a true/false
                // item, a coin-flip guess, and one of four single-choice items.
                slots.push(requirement(
                    outcome,
                    LearningItemFormat::SingleChoice,
                    3,
                    2,
                    3,
                ));
                slots.push(requirement(
                    outcome,
                    pick(&structured, "structured"),
                    1,
                    3,
                    3,
                ));
                for format in typed {
                    slots.push(requirement(outcome, format, 1, 3, 3));
                }
            }
            if settings.written_answers {
                slots.push(requirement(
                    outcome,
                    LearningItemFormat::Explanation,
                    1,
                    3,
                    4,
                ));
            }
            (
                LearningAssessmentPurpose::Checkpoint,
                bounded_title("Lesson quiz", &lesson.title),
                settings.quiz_pass,
                vec![lesson],
            )
        }
        LearningQuizScopeKind::Module => {
            let module = facts
                .modules
                .iter()
                .find(|m| Some(&m.id) == scope.id.as_ref())?;
            let lessons: Vec<&LessonFacts> = facts
                .lessons
                .iter()
                .filter(|l| l.module_id == module.id)
                .filter(prepared)
                .collect();
            if lessons.is_empty() {
                return None;
            }
            for lesson in &lessons {
                let outcome = lesson.outcome_id.as_deref()?;
                slots.push(requirement(
                    outcome,
                    LearningItemFormat::SingleChoice,
                    2,
                    3,
                    4,
                ));
                slots.push(requirement(
                    outcome,
                    LearningItemFormat::ShortAnswer,
                    1,
                    3,
                    3,
                ));
            }
            if settings.written_answers {
                for outcome in &module.outcome_ids {
                    slots.push(requirement(
                        outcome,
                        LearningItemFormat::Explanation,
                        1,
                        3,
                        4,
                    ));
                }
            }
            (
                LearningAssessmentPurpose::ModuleTest,
                bounded_title("Module test", &module.title),
                settings.test_pass,
                lessons,
            )
        }
        LearningQuizScopeKind::Course => {
            let lessons: Vec<&LessonFacts> = facts.lessons.iter().filter(prepared).collect();
            if lessons.is_empty() {
                return None;
            }
            for lesson in &lessons {
                let outcome = lesson.outcome_id.as_deref()?;
                slots.push(requirement(
                    outcome,
                    LearningItemFormat::SingleChoice,
                    1,
                    3,
                    4,
                ));
            }
            if settings.written_answers {
                for module in &facts.modules {
                    let covered = lessons.iter().any(|l| l.module_id == module.id);
                    if let (true, Some(outcome)) = (covered, module.outcome_ids.first()) {
                        slots.push(requirement(
                            outcome,
                            LearningItemFormat::Explanation,
                            1,
                            4,
                            4,
                        ));
                    }
                }
            }
            (
                LearningAssessmentPurpose::Cumulative,
                bounded_title("Course exam", &facts.program_title),
                settings.exam_pass,
                lessons,
            )
        }
    };
    let total = match scope.kind {
        LearningQuizScopeKind::Lesson => 1,
        LearningQuizScopeKind::Module => facts
            .lessons
            .iter()
            .filter(|l| Some(&l.module_id) == scope.id.as_ref())
            .count(),
        LearningQuizScopeKind::Course => facts.lessons.len(),
    };
    let expected_minutes = slots
        .iter()
        .map(|s| minutes_for(s.requirement.format) * s.requirement.count as u32)
        .sum::<u32>()
        .max(1);
    let sources: BTreeSet<String> = scoped
        .iter()
        .flat_map(|l| l.source_version_ids.iter().cloned())
        .collect();
    Some(DesiredBlueprint {
        purpose,
        title,
        instructions: if settings.written_answers {
            "Answer every question. Questions you leave blank score zero. Written answers are graded after you submit, and each lost point comes with the reason.".into()
        } else {
            "Answer every question. Questions you leave blank score zero.".into()
        },
        expected_minutes,
        passing_score: (passing * 10_000.0).round() / 10_000.0,
        slots,
        source_version_ids: sources.into_iter().take(32).collect(),
        coverage: (scope.kind != LearningQuizScopeKind::Lesson).then_some((scoped.len(), total)),
    })
}

/// The course's quiz settings, or the defaults when none were saved.
pub async fn settings_in(
    connection: &mut SqliteConnection,
    program_id: &str,
) -> Result<LearningQuizSettingsDto> {
    let row = sqlx::query("SELECT * FROM learning_quiz_settings WHERE program_id=?")
        .bind(program_id)
        .fetch_optional(&mut *connection)
        .await
        .map_err(db)?;
    Ok(match row {
        Some(row) => LearningQuizSettingsDto {
            quiz_pass: row.get("quiz_pass"),
            test_pass: row.get("test_pass"),
            exam_pass: row.get("exam_pass"),
            written_answers: row.get::<i64, _>("written_answers") != 0,
        },
        None => default_settings(),
    })
}

/// The lesson's outcome: the existing one, or a new one with `preferred_id`
/// (or a fresh ID) named after the lesson.
pub async fn ensure_lesson_outcome_in(
    connection: &mut SqliteConnection,
    program_id: &str,
    lesson_id: &str,
    preferred_id: Option<&str>,
) -> Result<String> {
    if let Some(existing) = sqlx::query_scalar::<_, String>(
        "SELECT id FROM learning_outcome_definitions WHERE program_id=? AND lesson_id=? ORDER BY created_at,id LIMIT 1",
    )
    .bind(program_id)
    .bind(lesson_id)
    .fetch_optional(&mut *connection)
    .await
    .map_err(db)?
    {
        return Ok(existing);
    }
    let lesson = sqlx::query(
        "SELECT title,objective,ordinal FROM learning_lessons WHERE id=? AND program_id=?",
    )
    .bind(lesson_id)
    .bind(program_id)
    .fetch_optional(&mut *connection)
    .await
    .map_err(db)?
    .ok_or_else(|| AppError::NotFound("Lesson not found in this course".into()))?;
    let id = preferred_id
        .map(str::to_owned)
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    sqlx::query("INSERT INTO learning_outcome_definitions(id,program_id,module_id,lesson_id,title,description,ordinal,created_at) VALUES(?,?,NULL,?,?,?,?,?)")
        .bind(&id)
        .bind(program_id)
        .bind(lesson_id)
        .bind(lesson.get::<String, _>("title"))
        .bind(lesson.get::<String, _>("objective"))
        .bind(100 + lesson.get::<i64, _>("ordinal"))
        .bind(now())
        .execute(&mut *connection)
        .await
        .map_err(db)?;
    Ok(id)
}

/// What the blueprints read about the course.
pub async fn course_facts_in(
    connection: &mut SqliteConnection,
    program_id: &str,
) -> Result<CourseFacts> {
    let program_title: String =
        sqlx::query_scalar("SELECT title FROM learning_programs WHERE id=?")
            .bind(program_id)
            .fetch_optional(&mut *connection)
            .await
            .map_err(db)?
            .ok_or_else(|| AppError::NotFound("Learning program not found".into()))?;
    let owned: HashSet<String> = sqlx::query_scalar::<_, String>(
        "SELECT id FROM learning_source_versions WHERE program_id=?",
    )
    .bind(program_id)
    .fetch_all(&mut *connection)
    .await
    .map_err(db)?
    .into_iter()
    .collect();
    let mut modules = Vec::new();
    for row in
        sqlx::query("SELECT id,title FROM learning_modules WHERE program_id=? ORDER BY ordinal")
            .bind(program_id)
            .fetch_all(&mut *connection)
            .await
            .map_err(db)?
    {
        let id: String = row.get("id");
        let outcome_ids = sqlx::query_scalar::<_, String>("SELECT id FROM learning_outcome_definitions WHERE program_id=? AND module_id=? AND lesson_id IS NULL ORDER BY ordinal,id")
            .bind(program_id)
            .bind(&id)
            .fetch_all(&mut *connection)
            .await
            .map_err(db)?;
        modules.push(ModuleFacts {
            id,
            title: row.get("title"),
            outcome_ids,
        });
    }
    let mut lessons = Vec::new();
    for row in sqlx::query("SELECT l.id,l.module_id,l.title,l.objective,l.preparation FROM learning_lessons l JOIN learning_modules m ON m.id=l.module_id WHERE l.program_id=? AND l.curriculum_state NOT IN ('replaced','skipped') ORDER BY m.ordinal,l.ordinal")
        .bind(program_id)
        .fetch_all(&mut *connection)
        .await
        .map_err(db)?
    {
        let id: String = row.get("id");
        let outcome_id = sqlx::query_scalar::<_, String>("SELECT id FROM learning_outcome_definitions WHERE program_id=? AND lesson_id=? ORDER BY created_at,id LIMIT 1")
            .bind(program_id)
            .bind(&id)
            .fetch_optional(&mut *connection)
            .await
            .map_err(db)?;
        let mut sources = BTreeSet::new();
        for json in sqlx::query_scalar::<_, String>("SELECT source_ids_json FROM learning_blocks WHERE lesson_id=?")
            .bind(&id)
            .fetch_all(&mut *connection)
            .await
            .map_err(db)?
        {
            for source in decode::<Vec<String>>(&json).unwrap_or_default() {
                if owned.contains(&source) {
                    sources.insert(source);
                }
            }
        }
        let prepared = row.get::<String, _>("preparation") == "ready";
        let quiz_formats = if prepared {
            lesson_quiz_formats_in(connection, program_id, &id).await?
        } else {
            Vec::new()
        };
        lessons.push(LessonFacts {
            id,
            module_id: row.get("module_id"),
            title: row.get("title"),
            objective: row.get("objective"),
            prepared,
            outcome_id,
            source_version_ids: sources.into_iter().collect(),
            quiz_formats,
        });
    }
    Ok(CourseFacts {
        program_title,
        modules,
        lessons,
    })
}

/// The formats a lesson's latest quiz blueprint asks; empty before its first.
pub async fn lesson_quiz_formats_in(
    connection: &mut SqliteConnection,
    program_id: &str,
    lesson_id: &str,
) -> Result<Vec<LearningItemFormat>> {
    let scope = LearningQuizScopeDto {
        kind: LearningQuizScopeKind::Lesson,
        id: Some(lesson_id.to_owned()),
    };
    let Some(blueprint) = latest_blueprint_in(connection, program_id, &scope).await? else {
        return Ok(Vec::new());
    };
    Ok(requirements_in(connection, &blueprint)
        .await?
        .into_iter()
        .map(|requirement| requirement.format)
        .collect())
}

/// A stored blueprint revision.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlueprintRef {
    pub id: String,
    pub revision: i64,
}

fn scope_kind_name(kind: LearningQuizScopeKind) -> &'static str {
    match kind {
        LearningQuizScopeKind::Lesson => "lesson",
        LearningQuizScopeKind::Module => "module",
        LearningQuizScopeKind::Course => "course",
    }
}

pub fn purpose_name(purpose: LearningAssessmentPurpose) -> &'static str {
    match purpose {
        LearningAssessmentPurpose::Practice => "practice",
        LearningAssessmentPurpose::Checkpoint => "checkpoint",
        LearningAssessmentPurpose::ModuleTest => "module_test",
        LearningAssessmentPurpose::Cumulative => "cumulative",
        LearningAssessmentPurpose::Transfer => "transfer",
    }
}

pub fn format_name(format: LearningItemFormat) -> &'static str {
    match format {
        LearningItemFormat::SingleChoice => "single_choice",
        LearningItemFormat::MultiSelect => "multi_select",
        LearningItemFormat::TrueFalse => "true_false",
        LearningItemFormat::ShortAnswer => "short_answer",
        LearningItemFormat::Cloze => "cloze",
        LearningItemFormat::Matching => "matching",
        LearningItemFormat::Ordering => "ordering",
        LearningItemFormat::Explanation => "explanation",
        LearningItemFormat::Artifact => "artifact",
    }
}

/// The latest stored revision for a scope.
pub async fn latest_blueprint_in(
    connection: &mut SqliteConnection,
    program_id: &str,
    scope: &LearningQuizScopeDto,
) -> Result<Option<BlueprintRef>> {
    let row = sqlx::query("SELECT id,revision FROM learning_assessment_blueprints WHERE program_id=? AND scope_kind=? AND scope_id IS ? AND status='accepted' ORDER BY revision DESC LIMIT 1")
        .bind(program_id)
        .bind(scope_kind_name(scope.kind))
        .bind(&scope.id)
        .fetch_optional(&mut *connection)
        .await
        .map_err(db)?;
    Ok(row.map(|r| BlueprintRef {
        id: r.get("id"),
        revision: r.get("revision"),
    }))
}

/// The scope's blueprint as stored, with a new revision written when the
/// course or its settings changed what it should ask. None when the scope has
/// nothing to ask yet.
pub async fn ensure_blueprint_in(
    connection: &mut SqliteConnection,
    program_id: &str,
    scope: &LearningQuizScopeDto,
) -> Result<Option<(BlueprintRef, DesiredBlueprint)>> {
    validate_scope(scope)?;
    let settings = settings_in(connection, program_id).await?;
    let facts = course_facts_in(connection, program_id).await?;
    ensure_blueprint_with(connection, program_id, scope, &facts, &settings).await
}

/// [`ensure_blueprint_in`] with the course already read, for callers that
/// derive many scopes at once.
pub async fn ensure_blueprint_with(
    connection: &mut SqliteConnection,
    program_id: &str,
    scope: &LearningQuizScopeDto,
    facts: &CourseFacts,
    settings: &LearningQuizSettingsDto,
) -> Result<Option<(BlueprintRef, DesiredBlueprint)>> {
    let Some(desired) = blueprint_for(scope, facts, settings) else {
        return Ok(None);
    };
    let latest = latest_blueprint_in(connection, program_id, scope).await?;
    if let Some(latest) = &latest {
        let row = sqlx::query("SELECT title,passing_score,expected_minutes FROM learning_assessment_blueprints WHERE id=? AND revision=?")
            .bind(&latest.id)
            .bind(latest.revision)
            .fetch_one(&mut *connection)
            .await
            .map_err(db)?;
        let slots = sqlx::query("SELECT outcome_id,format,difficulty,difficulty_max,points FROM learning_assessment_blueprint_slots WHERE blueprint_id=? AND blueprint_revision=? ORDER BY ordinal")
            .bind(&latest.id)
            .bind(latest.revision)
            .fetch_all(&mut *connection)
            .await
            .map_err(db)?
            .into_iter()
            .map(|r| {
                (
                    r.get::<String, _>("outcome_id"),
                    r.get::<String, _>("format"),
                    r.get::<i64, _>("difficulty"),
                    r.get::<i64, _>("difficulty_max"),
                    (r.get::<f64, _>("points") * 1000.0).round() as i64,
                )
            })
            .collect::<Vec<_>>();
        let wanted = slot_rows(&desired);
        if row.get::<String, _>("title") == desired.title
            && (row.get::<f64, _>("passing_score") - desired.passing_score).abs() < 1e-9
            && row.get::<i64, _>("expected_minutes") == i64::from(desired.expected_minutes)
            && slots == wanted
        {
            return Ok(Some((latest.clone(), desired)));
        }
    }
    let (id, revision, predecessor) = match &latest {
        Some(latest) => (
            latest.id.clone(),
            latest.revision + 1,
            Some(latest.revision),
        ),
        None => (uuid::Uuid::new_v4().to_string(), 1, None),
    };
    sqlx::query("INSERT INTO learning_assessment_blueprints(id,program_id,revision,predecessor_revision,purpose,title,instructions,expected_minutes,allowed_aids_json,passing_score,rubric_json,feedback_timing,source_version_ids_json,status,change_reason,created_at,scope_kind,scope_id) VALUES(?,?,?,?,?,?,?,?,'[]',?,'[]','after_submission',?,'accepted',?,?,?,?)")
        .bind(&id)
        .bind(program_id)
        .bind(revision)
        .bind(predecessor)
        .bind(purpose_name(desired.purpose))
        .bind(&desired.title)
        .bind(&desired.instructions)
        .bind(i64::from(desired.expected_minutes))
        .bind(desired.passing_score)
        .bind(encode(&desired.source_version_ids)?)
        .bind(if latest.is_some() {
            "The course or its quiz settings changed what this quiz covers."
        } else {
            "Derived from the course."
        })
        .bind(now())
        .bind(scope_kind_name(scope.kind))
        .bind(&scope.id)
        .execute(&mut *connection)
        .await
        .map_err(db)?;
    for (ordinal, (outcome, format, difficulty, difficulty_max, points)) in
        slot_rows(&desired).into_iter().enumerate()
    {
        sqlx::query("INSERT INTO learning_assessment_blueprint_slots(blueprint_id,blueprint_revision,ordinal,outcome_id,format,difficulty,difficulty_max,points) VALUES(?,?,?,?,?,?,?,?)")
            .bind(&id)
            .bind(revision)
            .bind(ordinal as i64)
            .bind(outcome)
            .bind(format)
            .bind(difficulty)
            .bind(difficulty_max)
            .bind(points as f64 / 1000.0)
            .execute(&mut *connection)
            .await
            .map_err(db)?;
    }
    Ok(Some((BlueprintRef { id, revision }, desired)))
}

/// One slot row per item, as stored.
fn slot_rows(desired: &DesiredBlueprint) -> Vec<(String, String, i64, i64, i64)> {
    desired
        .slots
        .iter()
        .flat_map(|slot| {
            let r = &slot.requirement;
            (0..r.count).map(move |_| {
                (
                    r.outcome_id.clone(),
                    format_name(r.format).to_string(),
                    i64::from(r.difficulty_min),
                    i64::from(r.difficulty_max),
                    (slot.points * 1000.0).round() as i64,
                )
            })
        })
        .collect()
}

/// The blueprint's requirements, read back from its slots.
pub async fn requirements_in(
    connection: &mut SqliteConnection,
    blueprint: &BlueprintRef,
) -> Result<Vec<LearningBlueprintRequirement>> {
    let mut requirements: Vec<LearningBlueprintRequirement> = Vec::new();
    for row in sqlx::query("SELECT outcome_id,format,difficulty,difficulty_max FROM learning_assessment_blueprint_slots WHERE blueprint_id=? AND blueprint_revision=? ORDER BY ordinal")
        .bind(&blueprint.id)
        .bind(blueprint.revision)
        .fetch_all(&mut *connection)
        .await
        .map_err(db)?
    {
        let outcome: String = row.get("outcome_id");
        let format = parse_format(row.get("format"))?;
        let min = row.get::<i64, _>("difficulty").clamp(1, 5) as u8;
        let max = row.get::<i64, _>("difficulty_max").clamp(1, 5) as u8;
        match requirements
            .iter_mut()
            .find(|r| r.outcome_id == outcome && r.format == format)
        {
            Some(existing) => {
                existing.count += 1;
                existing.difficulty_min = existing.difficulty_min.min(min);
                existing.difficulty_max = existing.difficulty_max.max(max);
            }
            None => requirements.push(LearningBlueprintRequirement {
                outcome_id: outcome,
                format,
                count: 1,
                difficulty_min: min,
                difficulty_max: max,
            }),
        }
    }
    Ok(requirements)
}

pub fn parse_format(value: &str) -> Result<LearningItemFormat> {
    serde_json::from_value(serde_json::Value::String(value.to_string()))
        .map_err(|_| AppError::Database("Invalid assessment item format".into()))
}

pub fn validate_scope(scope: &LearningQuizScopeDto) -> Result<()> {
    match (scope.kind, scope.id.as_deref()) {
        (LearningQuizScopeKind::Course, None) => Ok(()),
        (LearningQuizScopeKind::Lesson | LearningQuizScopeKind::Module, Some(id))
            if uuid::Uuid::parse_str(id).is_ok() =>
        {
            Ok(())
        }
        _ => Err(AppError::InvalidInput(
            "A lesson or module quiz names its lesson or module; the course exam names neither."
                .into(),
        )),
    }
}

#[cfg(test)]
#[path = "quiz_blueprints_tests.rs"]
mod tests;
