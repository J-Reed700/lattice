//! A course index's status, for the course workspace: whether the course's
//! evidence can be read at once, is being indexed, or is searched by words
//! only, and why.
//!
//! The read is cheap and changes nothing it reads: the store's meta and
//! counts and its document keys through a short-lived connection (never the
//! open store, never a build), the course's active versions and its builds in
//! the job store. The one thing it may do is queue a build, when the index is
//! behind its course and none is queued, so a course is never shown as
//! indexing while nothing indexes it.

use super::super::job::{Activity, Phase};
use super::super::sync;
use super::{build_name, CourseIndexes, Rerun};
use crate::features::learning::source_library::ActiveVersion;
use crate::infrastructure::passage_index::registry;
use lattice_core::shared::runtime::jobs::{JobRecord, JobStatus};
use lattice_core::shared::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::Path;

/// Where a course's evidence index stands.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(tag = "state", rename_all = "camelCase")]
pub enum CourseIndexStatusDto {
    /// Every passage is embedded by the active model; `mode` is `hybrid`.
    Ready { mode: String },
    /// A build is queued or running, or the index is behind its course and
    /// one has just been queued. Lesson preparation waits for it; a search
    /// or attached evidence reads every passage by its words meanwhile.
    #[serde(rename_all = "camelCase")]
    Indexing {
        /// Passages of the whole index embedded so far, of every passage it
        /// holds: before a build starts, while it embeds, and after a resume,
        /// so the count only climbs.
        embedded: u32,
        total: u32,
        /// Active versions the index does not hold yet.
        references_new: u32,
        /// The running build's measured pace, once it is embedding.
        passages_per_second: Option<f64>,
        /// The build has not started: another course's build holds the one
        /// slot builds take turns in, and this one starts when it finishes.
        waiting_turn: bool,
    },
    /// No embedding model is active: the course is searched by its words.
    NoEmbeddingModel,
    /// The index cannot be read, or its last build could not finish; the
    /// course is searched by its words meanwhile.
    Unavailable { reason: String },
}

impl CourseIndexes {
    /// `program`'s index status. Never fails: what cannot be read is
    /// `Unavailable`, with the reason.
    pub async fn status(&self, program: &str) -> CourseIndexStatusDto {
        self.read_status(program)
            .await
            .unwrap_or_else(|error| CourseIndexStatusDto::Unavailable {
                reason: error.to_string(),
            })
    }

    async fn read_status(&self, program: &str) -> Result<CourseIndexStatusDto> {
        let dir = self.dir_of(program)?;
        let Some(identity) = self.embedders.identity().await else {
            return Ok(CourseIndexStatusDto::NoEmbeddingModel);
        };
        let versions = self.repository().active_versions(program).await?;
        let builds = self
            .jobs
            .store()
            .list_for_subject(super::super::COURSE_INDEX, program)
            .await?;
        let inspected = registry::inspect(&dir, "").await?;
        let counts = inspected
            .as_ref()
            .map(|inspected| inspected.counts)
            .unwrap_or_default();
        if let Some(live) = builds.iter().find(|job| !job.status.is_finished()) {
            let waiting_turn =
                live.status == JobStatus::Pending && self.another_course_builds(program).await?;
            return Ok(indexing(
                live,
                counts,
                missing(&dir, &versions).await?,
                waiting_turn,
            ));
        }
        let stamp = sync::stamp(&sync::revision(&versions), &identity);
        let recorded = inspected
            .as_ref()
            .and_then(|inspected| inspected.meta.get(sync::META_REVISION));
        if versions.is_empty() || recorded == Some(&stamp) {
            return Ok(CourseIndexStatusDto::Ready {
                mode: super::super::IndexMode::Hybrid.label().into(),
            });
        }
        let name = build_name(program, &stamp);
        if let Some(done) = builds.iter().find(|job| job.operation_id == name) {
            // This very build ran and left the index behind (the model
            // stopped part-way): it runs again when the course or the model
            // changes, or at the next start, and nothing waits for it.
            return Ok(CourseIndexStatusDto::Unavailable {
                reason: stopped(done),
            });
        }
        self.queue_stamp(program, &stamp, Rerun::No).await?;
        Ok(CourseIndexStatusDto::Indexing {
            embedded: counts.passages_embedded,
            total: counts.passages_total,
            references_new: u32::try_from(missing(&dir, &versions).await?).unwrap_or(u32::MAX),
            passages_per_second: None,
            waiting_turn: self.another_course_builds(program).await?,
        })
    }

    /// Whether another course's build is running: builds take turns one at
    /// a time, so a queued build for `program` waits for it.
    async fn another_course_builds(&self, program: &str) -> Result<bool> {
        Ok(self
            .jobs
            .store()
            .live(super::super::COURSE_INDEX)
            .await?
            .iter()
            .any(|job| {
                job.status == JobStatus::Running && job.subject_id.as_deref() != Some(program)
            }))
    }
}

/// How many of `versions` the index in `dir` does not hold yet.
async fn missing(dir: &Path, versions: &[ActiveVersion]) -> Result<usize> {
    let held: HashSet<String> = registry::document_keys(dir)
        .await?
        .unwrap_or_default()
        .into_iter()
        .collect();
    Ok(versions
        .iter()
        .filter(|version| !held.contains(&version.id))
        .count())
}

/// A live build's status: the whole index's counts, taken from the build's
/// activity once it is embedding (the index's saved counts move only when
/// the build saves) and from the index before. The pace is the build's own,
/// so the time left is its remaining passages at that pace. `waiting_turn`
/// when it is queued behind another course's build.
fn indexing(
    build: &JobRecord,
    counts: crate::infrastructure::passage_index::Counts,
    missing: usize,
    waiting_turn: bool,
) -> CourseIndexStatusDto {
    let activity = (build.status == JobStatus::Running)
        .then(|| build.activity.clone())
        .flatten()
        .and_then(|activity| serde_json::from_value::<Activity>(activity).ok());
    let as_u32 = |value: usize| u32::try_from(value).unwrap_or(u32::MAX);
    match activity {
        Some(activity) if activity.phase == Phase::Embedding => CourseIndexStatusDto::Indexing {
            embedded: as_u32(activity.index_embedded()),
            total: as_u32(activity.index_total()),
            references_new: as_u32(activity.references),
            passages_per_second: activity.per_second,
            waiting_turn: false,
        },
        _ => CourseIndexStatusDto::Indexing {
            embedded: counts.passages_embedded,
            total: counts.passages_total,
            references_new: as_u32(missing),
            passages_per_second: None,
            waiting_turn,
        },
    }
}

/// Why a finished build left the index behind, in its own words.
fn stopped(build: &JobRecord) -> String {
    let said = build
        .error_message
        .clone()
        .filter(|message| !message.trim().is_empty())
        .unwrap_or_else(|| build.progress_message.clone());
    format!("Indexing stopped before every passage was embedded: {said}")
}
