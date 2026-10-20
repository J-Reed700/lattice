//! What Learning has running, for the activity view: lesson preparations,
//! course index builds and outline runs, named by their course and lesson.

use super::curriculum::{LearningGenerationActivity, LearningGenerationPhase};
use super::curriculum_repository::LESSON_PREPARATION;
use super::outline_progress::{LearningOutlineProgressDto, OutlineRunRegistry, OutlineStage};
use super::references::course_index::CourseIndexes;
use async_trait::async_trait;
use lattice_core::application::ports::{
    ActivityItem, ActivityProgress, ActivityResource, ActivitySource, ActivityState, ActivityTarget,
};
use lattice_core::shared::runtime::jobs::{JobRecord, JobRuntime, JobStatus};
use lattice_core::shared::{AppError, Result};
use sqlx::{Row, SqlitePool};
use std::collections::HashMap;
use std::sync::Arc;

/// Learning's live work, read from its jobs, its tables and its outline runs.
pub struct LearningActivity {
    pool: SqlitePool,
    jobs: Arc<JobRuntime>,
    courses: Option<Arc<CourseIndexes>>,
    outlines: OutlineRunRegistry,
}

impl LearningActivity {
    pub fn new(
        pool: SqlitePool,
        jobs: Arc<JobRuntime>,
        courses: Option<Arc<CourseIndexes>>,
        outlines: OutlineRunRegistry,
    ) -> Self {
        Self {
            pool,
            jobs,
            courses,
            outlines,
        }
    }
}

#[async_trait]
impl ActivitySource for LearningActivity {
    async fn activity(&self) -> Result<Vec<ActivityItem>> {
        let lessons = self.jobs.store().live(LESSON_PREPARATION).await?;
        let outlines = self.outlines.running();
        let mut programs: Vec<String> = lessons
            .iter()
            .filter_map(|job| job.subject_id.clone())
            .chain(
                outlines
                    .iter()
                    .filter_map(|(_, run)| run.program_id.clone()),
            )
            .collect();
        let courses = match &self.courses {
            Some(courses) => courses.activity().await?,
            None => Vec::new(),
        };
        programs.sort();
        programs.dedup();
        let titles = course_titles(&self.pool, &programs).await?;
        let lesson_ids: Vec<String> = lessons.iter().flat_map(requested_lessons).collect();
        let lesson_titles = lesson_titles(&self.pool, &lesson_ids).await?;

        let mut items: Vec<ActivityItem> = lessons
            .iter()
            .map(|job| lesson_item(job, &titles, &lesson_titles))
            .collect();
        items.extend(courses);
        items.extend(
            outlines
                .iter()
                .map(|(id, run)| outline_item(id, run, &titles)),
        );
        Ok(items)
    }
}

/// The titles of `programs` by id; a course deleted meanwhile is left out.
pub(crate) async fn course_titles(
    pool: &SqlitePool,
    programs: &[String],
) -> Result<HashMap<String, String>> {
    titles(
        pool,
        "SELECT id, title FROM learning_programs WHERE id IN (SELECT value FROM json_each(?))",
        programs,
    )
    .await
}

async fn lesson_titles(pool: &SqlitePool, lessons: &[String]) -> Result<HashMap<String, String>> {
    titles(
        pool,
        "SELECT id, title FROM learning_lessons WHERE id IN (SELECT value FROM json_each(?))",
        lessons,
    )
    .await
}

async fn titles(pool: &SqlitePool, sql: &str, ids: &[String]) -> Result<HashMap<String, String>> {
    if ids.is_empty() {
        return Ok(HashMap::new());
    }
    let ids = serde_json::to_string(ids).map_err(|e| AppError::Serialization(e.to_string()))?;
    let rows = sqlx::query(sql)
        .bind(ids)
        .fetch_all(pool)
        .await
        .map_err(|e| AppError::Database(e.to_string()))?;
    Ok(rows
        .iter()
        .map(|row| (row.get::<String, _>("id"), row.get::<String, _>("title")))
        .collect())
}

/// The lessons a preparation job was asked for, in order.
fn requested_lessons(job: &JobRecord) -> Vec<String> {
    job.requested
        .get("lessonIds")
        .and_then(serde_json::Value::as_array)
        .map(|ids| {
            ids.iter()
                .filter_map(|id| id.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

/// What a preparation phase is doing, as the lesson's own progress view
/// names it.
fn phase_words(phase: LearningGenerationPhase) -> &'static str {
    match phase {
        LearningGenerationPhase::References => "Preparing reference material",
        LearningGenerationPhase::Writing => "Building the lesson draft",
        LearningGenerationPhase::Review => "Reviewing teaching and assessments",
        LearningGenerationPhase::Inventory => "Identifying factual claims",
        LearningGenerationPhase::Coverage => "Checking every section is covered",
        LearningGenerationPhase::Examples => "Checking worked examples",
        LearningGenerationPhase::Evidence => "Verifying factual claims",
        LearningGenerationPhase::Research => "Researching unresolved claims",
        LearningGenerationPhase::Repair => "Repairing the draft",
        LearningGenerationPhase::Publishing => "Saving the verified lesson",
    }
}

fn lesson_item(
    job: &JobRecord,
    courses: &HashMap<String, String>,
    lessons: &HashMap<String, String>,
) -> ActivityItem {
    let program = job.subject_id.clone().unwrap_or_default();
    let course = courses.get(&program).cloned();
    let requested = requested_lessons(job);
    let activity: Option<LearningGenerationActivity> = job
        .activity
        .clone()
        .and_then(|value| serde_json::from_value(value).ok());
    let current = activity
        .as_ref()
        .and_then(|activity| activity.lesson_title.clone())
        .or_else(|| requested.first().and_then(|id| lessons.get(id).cloned()));
    let lesson = match (&current, requested.len()) {
        (Some(title), 0 | 1) => format!("“{title}”"),
        (Some(title), more) => format!("“{title}” and {} more", more - 1),
        (None, 0 | 1) => "a lesson".to_string(),
        (None, more) => format!("{more} lessons"),
    };
    let title = match &course {
        Some(course) => format!("Preparing {lesson} in {course}"),
        None => format!("Preparing {lesson}"),
    };
    let target = match requested.first() {
        Some(lesson) => ActivityTarget::Lesson {
            course: program.clone(),
            lesson: lesson.clone(),
        },
        None => ActivityTarget::Course {
            course: program.clone(),
        },
    };
    let mut item = ActivityItem::of_job(job, title)
        .subject(current.map_or(lesson, |title| format!("“{title}”")))
        .queue(format!("{LESSON_PREPARATION}:{program}"))
        .target(target);
    if job.status != JobStatus::Running {
        return item;
    }
    let Some(activity) = activity else {
        return item;
    };
    let checks = (activity.checks_total > 0).then(|| {
        ActivityProgress::of(
            u64::from(activity.checks_completed),
            u64::from(activity.checks_total),
            "claims checked",
        )
    });
    let lessons_done = (job.progress_total > 1).then(|| {
        ActivityProgress::of(
            u64::from(job.progress_current),
            u64::from(job.progress_total),
            "lessons",
        )
    });
    item = item
        .detail(phase_words(activity.phase))
        .progress(checks.or(lessons_done));
    match activity.model_name {
        Some(name) if activity.model_running => item.uses(ActivityResource::Model { name }),
        _ => item,
    }
}

/// What an outline stage is doing.
fn stage_words(stage: OutlineStage) -> &'static str {
    match stage {
        OutlineStage::ReadingSources => "Reading the sources",
        OutlineStage::LoadingModel => "Loading the model",
        OutlineStage::GatheringReferences => "Gathering references",
        OutlineStage::Drafting => "Drafting the outline",
        OutlineStage::CheckingCitations => "Checking citations",
        OutlineStage::Reviewing => "Reviewing the outline",
        OutlineStage::Researching => "Researching",
        OutlineStage::Repairing => "Repairing the outline",
        OutlineStage::CheckingRepair => "Checking the repair",
        OutlineStage::Saving => "Saving the outline",
        OutlineStage::Completed => "Done",
        OutlineStage::Cancelled => "Stopping",
        OutlineStage::Failed => "Stopping after a failure",
    }
}

fn outline_item(
    id: &str,
    run: &LearningOutlineProgressDto,
    courses: &HashMap<String, String>,
) -> ActivityItem {
    let course = run
        .program_id
        .as_ref()
        .and_then(|program| courses.get(program));
    let title = match course {
        Some(course) => format!("Outlining {course}"),
        None => "Outlining a new course".to_string(),
    };
    let detail = match &run.detail {
        Some(detail) => format!("{}: {detail}", stage_words(run.stage)),
        None => stage_words(run.stage).to_string(),
    };
    let started =
        chrono::Utc::now().timestamp_millis() - i64::from(run.elapsed_seconds).saturating_mul(1000);
    let mut item = ActivityItem::new(
        format!("learning.outline:{id}"),
        "learning.outline",
        title,
        ActivityState::Running,
    )
    .detail(detail)
    .since(Some(started));
    if let Some(program) = &run.program_id {
        item = item.target(ActivityTarget::Course {
            course: program.clone(),
        });
    }
    match &run.model_name {
        Some(name) if run.stage != OutlineStage::ReadingSources => {
            item.uses(ActivityResource::Model { name: name.clone() })
        }
        _ => item,
    }
}
