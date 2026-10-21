//! The course index builds as the activity view lists them: named by their
//! course, with the passages embedded and the pace each build measured.

use super::super::job::{Activity, Phase};
use super::super::COURSE_INDEX;
use super::CourseIndexes;
use crate::features::learning::activity::course_titles;
use lattice_core::application::ports::{
    ActivityItem, ActivityProgress, ActivityResource, ActivityState, ActivityTarget,
};
use lattice_core::shared::runtime::jobs::JobStatus;
use lattice_core::shared::Result;

impl CourseIndexes {
    /// Each live build, running or queued. Builds take this kind's one slot
    /// in turn, so a queued one waits behind the running one.
    pub async fn activity(&self) -> Result<Vec<ActivityItem>> {
        let jobs = self.jobs.store().live(COURSE_INDEX).await?;
        let programs: Vec<String> = jobs
            .iter()
            .filter_map(|job| job.subject_id.clone())
            .collect();
        let titles = course_titles(&self.pool, &programs).await?;
        Ok(jobs
            .iter()
            .map(|job| {
                let program = job.subject_id.clone().unwrap_or_default();
                let course = titles
                    .get(&program)
                    .cloned()
                    .unwrap_or_else(|| "a course".to_string());
                let item = ActivityItem::of_job(job, format!("Indexing evidence for {course}"))
                    .subject(course)
                    .queue(COURSE_INDEX)
                    .index_dir(self.dir_of(&program).ok())
                    .target(ActivityTarget::Course { course: program });
                let activity = job
                    .activity
                    .clone()
                    .and_then(|value| serde_json::from_value::<Activity>(value).ok());
                match activity {
                    Some(activity) if job.status == JobStatus::Running => running(item, &activity),
                    _ => item,
                }
            })
            .collect())
    }
}

/// A running build's words, its passages and its measured pace.
fn running(item: ActivityItem, activity: &Activity) -> ActivityItem {
    let mut item = item.detail(activity.message());
    match activity.phase {
        // A lesson is bringing this index current; the build holds for it.
        Phase::Waiting => {
            item.state = ActivityState::Paused;
            item
        }
        Phase::Embedding => item
            .progress(Some(ActivityProgress::of(
                activity.embedded as u64,
                activity.total as u64,
                "passages",
            )))
            .pace(activity.per_second)
            .uses(ActivityResource::EmbeddingModel),
        Phase::Starting | Phase::Indexing => item,
    }
}
