//! The blueprints' unit-of-work functions: they read the course and store blueprint revisions on the caller's
//! connection, and commit nothing.

use super::names::scope_kind_name;
use super::{
    blueprint_for, default_settings, parse_format, purpose_name, repository, slot_rows,
    validate_scope, CourseFacts, DesiredBlueprint, LessonFacts, ModuleFacts,
};
use crate::assessment_engine::{LearningBlueprintRequirement, LearningItemFormat};
use crate::dto::{LearningQuizScopeDto, LearningQuizScopeKind, LearningQuizSettingsDto};
use crate::persistence::{decode, encode, now};
use lattice_core::shared::error::{AppError, Result};
use sqlx::SqliteConnection;
use std::collections::{BTreeSet, HashSet};

/// The course's quiz settings, or the defaults when none were saved.
pub async fn settings_in(
    connection: &mut SqliteConnection,
    program_id: &str,
) -> Result<LearningQuizSettingsDto> {
    Ok(repository::quiz_settings_in(connection, program_id)
        .await?
        .unwrap_or_else(default_settings))
}

/// The lesson's outcome: the existing one, or a new one with `preferred_id`
/// (or a fresh ID) named after the lesson.
pub async fn ensure_lesson_outcome_in(
    connection: &mut SqliteConnection,
    program_id: &str,
    lesson_id: &str,
    preferred_id: Option<&str>,
) -> Result<String> {
    if let Some(existing) = repository::lesson_outcome_in(connection, program_id, lesson_id).await?
    {
        return Ok(existing);
    }
    let (title, objective, ordinal) =
        repository::lesson_heading_in(connection, program_id, lesson_id)
            .await?
            .ok_or_else(|| AppError::NotFound("Lesson not found in this course".into()))?;
    let id = preferred_id
        .map(str::to_owned)
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    repository::insert_lesson_outcome_in(
        connection,
        &repository::NewLessonOutcome {
            id: &id,
            program_id,
            lesson_id,
            title: &title,
            description: &objective,
            ordinal: 100 + ordinal,
            created_at: now(),
        },
    )
    .await?;
    Ok(id)
}

/// What the blueprints read about the course.
pub async fn course_facts_in(
    connection: &mut SqliteConnection,
    program_id: &str,
) -> Result<CourseFacts> {
    let program_title = repository::program_title_in(connection, program_id)
        .await?
        .ok_or_else(|| AppError::NotFound("Learning program not found".into()))?;
    let owned: HashSet<String> = repository::source_version_ids_in(connection, program_id)
        .await?
        .into_iter()
        .collect();
    let mut modules = Vec::new();
    for (id, title) in repository::modules_in(connection, program_id).await? {
        let outcome_ids = repository::module_outcome_ids_in(connection, program_id, &id).await?;
        modules.push(ModuleFacts {
            id,
            title,
            outcome_ids,
        });
    }
    let mut lessons = Vec::new();
    for row in repository::lessons_in(connection, program_id).await? {
        let id = row.id;
        let outcome_id = repository::lesson_outcome_in(connection, program_id, &id).await?;
        let mut sources = BTreeSet::new();
        for json in repository::block_source_ids_json_in(connection, &id).await? {
            for source in decode::<Vec<String>>(&json).unwrap_or_default() {
                if owned.contains(&source) {
                    sources.insert(source);
                }
            }
        }
        let prepared = row.preparation == "ready";
        let quiz_formats = if prepared {
            lesson_quiz_formats_in(connection, program_id, &id).await?
        } else {
            Vec::new()
        };
        lessons.push(LessonFacts {
            id,
            module_id: row.module_id,
            title: row.title,
            objective: row.objective,
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

/// The latest stored revision for a scope.
pub async fn latest_blueprint_in(
    connection: &mut SqliteConnection,
    program_id: &str,
    scope: &LearningQuizScopeDto,
) -> Result<Option<BlueprintRef>> {
    Ok(repository::latest_accepted_in(
        connection,
        program_id,
        scope_kind_name(scope.kind),
        &scope.id,
    )
    .await?
    .map(|(id, revision)| BlueprintRef { id, revision }))
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
        let row = repository::stored_blueprint_in(connection, &latest.id, latest.revision).await?;
        let slots = repository::stored_slots_in(connection, &latest.id, latest.revision).await?;
        let wanted = slot_rows(&desired);
        if row.title == desired.title
            && (row.passing_score - desired.passing_score).abs() < 1e-9
            && row.expected_minutes == i64::from(desired.expected_minutes)
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
    repository::insert_blueprint_in(
        connection,
        &repository::NewBlueprint {
            id: &id,
            program_id,
            revision,
            predecessor,
            purpose: purpose_name(desired.purpose),
            title: &desired.title,
            instructions: &desired.instructions,
            expected_minutes: i64::from(desired.expected_minutes),
            passing_score: desired.passing_score,
            source_version_ids_json: encode(&desired.source_version_ids)?,
            change_reason: if latest.is_some() {
                "The course or its quiz settings changed what this quiz covers."
            } else {
                "Derived from the course."
            },
            created_at: now(),
            scope_kind: scope_kind_name(scope.kind),
            scope_id: &scope.id,
        },
    )
    .await?;
    for (ordinal, (outcome, format, difficulty, difficulty_max, points)) in
        slot_rows(&desired).into_iter().enumerate()
    {
        repository::insert_slot_in(
            connection,
            &repository::NewSlot {
                blueprint_id: &id,
                revision,
                ordinal: ordinal as i64,
                outcome_id: &outcome,
                format: &format,
                difficulty,
                difficulty_max,
                points: points as f64 / 1000.0,
            },
        )
        .await?;
    }
    Ok(Some((BlueprintRef { id, revision }, desired)))
}

/// The blueprint's requirements, read back from its slots.
pub async fn requirements_in(
    connection: &mut SqliteConnection,
    blueprint: &BlueprintRef,
) -> Result<Vec<LearningBlueprintRequirement>> {
    let mut requirements: Vec<LearningBlueprintRequirement> = Vec::new();
    for (outcome, format, difficulty, difficulty_max) in
        repository::slot_requirements_in(connection, &blueprint.id, blueprint.revision).await?
    {
        let format = parse_format(&format)?;
        let min = difficulty.clamp(1, 5) as u8;
        let max = difficulty_max.clamp(1, 5) as u8;
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
