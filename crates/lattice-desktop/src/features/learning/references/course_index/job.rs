//! A course index build as a job: bring the course's index current in the
//! background, so the next lesson finds its references already embedded.
//!
//! The job's progress is the share of this run's passages embedded; its
//! activity says what the build is doing. A course deleted before its build
//! ran has its index removed instead.

use super::manager::CourseIndexes;
use super::sync::CourseProgress;
use super::IndexMode;
use crate::infrastructure::passage_index::job::ProgressSaver;
use async_trait::async_trait;
use lattice_core::shared::runtime::jobs::{JobContext, JobHandler, JobOutcome};
use lattice_core::shared::Result;
use serde::Serialize;
use std::sync::Weak;
use tokio::sync::watch;

pub(super) struct CourseIndexJob {
    courses: Weak<CourseIndexes>,
}

impl CourseIndexJob {
    pub fn new(courses: Weak<CourseIndexes>) -> Self {
        Self { courses }
    }
}

/// What a build is doing, saved as the job's activity.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
enum Phase {
    #[default]
    Starting,
    /// A reader is bringing the index current; the build waits for it.
    Waiting,
    /// New references are being chunked into the index.
    Indexing,
    Embedding,
}

#[derive(Debug, Clone, Copy, Default, Serialize)]
#[serde(rename_all = "camelCase")]
struct Activity {
    phase: Phase,
    references: usize,
    embedded: usize,
    total: usize,
}

impl Activity {
    fn percent(&self) -> u32 {
        if self.total == 0 {
            return 0;
        }
        u32::try_from(self.embedded.saturating_mul(100) / self.total).unwrap_or(100)
    }

    fn message(&self) -> String {
        match self.phase {
            Phase::Starting => "Checking the course's references".into(),
            Phase::Waiting => "Waiting for a lesson that is indexing this course".into(),
            Phase::Indexing => format!("Indexing {} new references", self.references),
            Phase::Embedding => {
                format!("Embedding passages: {} of {}", self.embedded, self.total)
            }
        }
    }
}

/// Sends each change to the job's progress loop.
struct Told(watch::Sender<Activity>);

impl CourseProgress for Told {
    fn waiting(&mut self) {
        self.0
            .send_modify(|activity| activity.phase = Phase::Waiting);
    }
    fn indexing(&mut self, references: usize) {
        self.0.send_modify(|activity| {
            activity.phase = Phase::Indexing;
            activity.references = references;
        });
    }
    fn embedded(&mut self, done: usize, total: usize) {
        self.0.send_modify(|activity| {
            activity.phase = Phase::Embedding;
            activity.embedded = done;
            activity.total = total;
        });
    }
}

#[async_trait]
impl JobHandler for CourseIndexJob {
    async fn run(&self, context: &JobContext) -> Result<JobOutcome> {
        let Some(courses) = self.courses.upgrade() else {
            return Ok(JobOutcome::Stopped);
        };
        let Some(program) = context.subject_id().map(str::to_string) else {
            return Ok(JobOutcome::Failed {
                code: "no_course".into(),
                message: "A course index build without its course".into(),
            });
        };
        if !courses.exists(&program).await? {
            courses.remove_index(&program);
            return Ok(JobOutcome::Completed {
                result_ref: program,
                message: "The course no longer exists".into(),
            });
        }
        let (sink, mut updates) = watch::channel(Activity::default());
        let mut told = Told(sink);
        let mut saver = ProgressSaver::new(context);
        // The run borrows the course; it ends with this block.
        let outcome = {
            let run = courses.ensure_current(&program, context.cancellation(), &mut told);
            tokio::pin!(run);
            let outcome = loop {
                tokio::select! {
                    outcome = &mut run => break outcome,
                    changed = updates.changed() => match changed {
                        Ok(()) => {
                            let activity = *updates.borrow_and_update();
                            saver
                                .save(
                                    activity.phase,
                                    activity.percent(),
                                    &activity.message(),
                                    &serde_json::to_value(activity)?,
                                    false,
                                )
                                .await?;
                        }
                        Err(_) => break (&mut run).await,
                    },
                }
            };
            outcome
        };
        if context.is_cancelled() {
            return Ok(JobOutcome::Stopped);
        }
        let mode = outcome?;
        tracing::info!(program = %program, mode = mode.label(), "Course index build finished");
        Ok(JobOutcome::Completed {
            result_ref: program,
            message: match mode {
                IndexMode::Hybrid => "Ready".into(),
                other => other.describe(),
            },
        })
    }
}
