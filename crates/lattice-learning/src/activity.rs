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
use lattice_core::shared::Result;
pub(crate) use repository::course_titles;
use repository::lesson_titles;
use sqlx::SqlitePool;
use std::collections::HashMap;
use std::sync::Arc;

mod repository;

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
        LearningGenerationPhase::Inventory => "Identifying factual statements",
        LearningGenerationPhase::Coverage => "Checking every section is covered",
        LearningGenerationPhase::Examples => "Checking worked examples",
        LearningGenerationPhase::Evidence => "Verifying factual statements",
        LearningGenerationPhase::Research => "Researching unresolved statements",
        LearningGenerationPhase::Repair => "Repairing the draft",
        LearningGenerationPhase::Questions => "Writing quiz questions",
        LearningGenerationPhase::Publishing => "Saving the verified lesson",
    }
}

/// The step of the lesson's progress view a phase shows under, as
/// `lessonPreparationActivity.ts` groups them.
fn progress_step(phase: LearningGenerationPhase) -> u8 {
    match phase {
        LearningGenerationPhase::References => 0,
        LearningGenerationPhase::Writing => 1,
        LearningGenerationPhase::Review => 2,
        LearningGenerationPhase::Inventory
        | LearningGenerationPhase::Coverage
        | LearningGenerationPhase::Examples => 3,
        LearningGenerationPhase::Evidence
        | LearningGenerationPhase::Research
        | LearningGenerationPhase::Repair => 4,
        LearningGenerationPhase::Questions | LearningGenerationPhase::Publishing => 5,
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
            "statements checked",
        )
    });
    let lessons_done = (job.progress_total > 1).then(|| {
        ActivityProgress::of(
            u64::from(job.progress_current),
            u64::from(job.progress_total),
            "lessons",
        )
    });
    let detail = if progress_step(activity.phase) < progress_step(activity.reached) {
        "Checking the repaired draft again"
    } else {
        phase_words(activity.phase)
    };
    item = item.detail(detail).progress(checks.or(lessons_done));
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

#[cfg(test)]
mod tests {
    use super::*;

    fn running_preparation(activity: LearningGenerationActivity) -> JobRecord {
        JobRecord {
            id: "job-1".into(),
            kind: "learning.lesson_preparation".into(),
            subject_id: Some("program-1".into()),
            operation_id: "operation-1".into(),
            payload_hash: "hash".into(),
            status: JobStatus::Running,
            requested: serde_json::json!({}),
            progress_current: 0,
            progress_total: 1,
            progress_message: String::new(),
            activity: serde_json::to_value(activity).ok(),
            staged_result: None,
            result_ref: None,
            error_code: None,
            cause_code: None,
            error_message: None,
            retry_of_job_id: None,
            retry_count: 0,
            retry_not_before: None,
            created_at: 0,
            started_at: Some(0),
            finished_at: None,
            heartbeat_at: None,
            rank: None,
        }
    }

    #[test]
    fn a_running_preparation_counts_statements_checked_and_names_its_phase_in_statements() {
        let job = running_preparation(LearningGenerationActivity {
            phase: LearningGenerationPhase::Evidence,
            checks_completed: 2,
            checks_total: 4,
            ..Default::default()
        });
        let item = lesson_item(&job, &HashMap::new(), &HashMap::new());
        assert_eq!(item.detail.as_deref(), Some("Verifying factual statements"));
        assert_eq!(
            item.progress.map(|progress| progress.unit),
            Some("statements checked".to_string())
        );
        assert_eq!(
            phase_words(LearningGenerationPhase::Inventory),
            "Identifying factual statements"
        );
        assert_eq!(
            phase_words(LearningGenerationPhase::Research),
            "Researching unresolved statements"
        );
    }

    #[test]
    fn a_check_the_repaired_draft_goes_back_through_is_named_as_that() {
        use LearningGenerationPhase as Phase;
        let detail = |phase, reached| {
            let job = running_preparation(LearningGenerationActivity {
                phase,
                reached,
                ..Default::default()
            });
            lesson_item(&job, &HashMap::new(), &HashMap::new()).detail
        };
        assert_eq!(
            detail(Phase::Review, Phase::Repair).as_deref(),
            Some("Checking the repaired draft again")
        );
        assert_eq!(
            detail(Phase::Coverage, Phase::Repair).as_deref(),
            Some("Checking the repaired draft again")
        );
        assert_eq!(
            detail(Phase::Evidence, Phase::Repair).as_deref(),
            Some("Verifying factual statements")
        );
        assert_eq!(
            detail(Phase::Repair, Phase::Repair).as_deref(),
            Some("Repairing the draft")
        );
        assert_eq!(
            detail(Phase::Review, Phase::Review).as_deref(),
            Some("Reviewing teaching and assessments")
        );
    }
}
