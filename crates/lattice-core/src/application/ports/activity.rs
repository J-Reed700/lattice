//! What the app is doing in the background, told by each part that does it.
//!
//! Long work and held resources are spread over many features: jobs of many
//! kinds, each backend's model requests, the embedding model, downloads, the
//! library's indexer. Each feature describes its own live items through an
//! [`ActivitySource`], in its owner's words (the course, the folder, the
//! lesson), and the activity view lists them together. A source states a
//! pace and a time left only when it measured one; it never estimates.

use crate::shared::error::Result;
use crate::shared::runtime::jobs::{JobRecord, JobStatus};
use async_trait::async_trait;
use serde::Serialize;
use std::path::PathBuf;

/// Where an item stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum ActivityState {
    Running,
    /// Not started yet, or between attempts.
    Waiting,
    /// Started, and holding while something else goes first.
    Paused,
}

/// A shared resource an item holds or uses while it runs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ActivityResource {
    /// The one embedding model every index and search embeds through.
    EmbeddingModel,
    /// A language model, by the name the user knows it by.
    Model { name: String },
}

/// Where in the app an item can be opened.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ActivityTarget {
    Course {
        course: String,
    },
    Lesson {
        course: String,
        lesson: String,
    },
    /// An Explorer folder, by its root as the folder list names it.
    Folder {
        root: String,
        name: String,
    },
    Conversation {
        conversation: String,
    },
}

/// How far an item has come, in its own unit ("passages", "files").
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ActivityProgress {
    pub done: u64,
    pub total: u64,
    pub unit: String,
}

impl ActivityProgress {
    pub fn of(done: u64, total: u64, unit: impl Into<String>) -> Self {
        Self {
            done: done.min(total),
            total,
            unit: unit.into(),
        }
    }
}

/// One thing running, waiting or paused, as its owner describes it.
#[derive(Debug, Clone, PartialEq)]
pub struct ActivityItem {
    /// Stable while the item lives: a job's id, or the source's own key.
    pub id: String,
    /// The sort of work: a job kind (`learning.course_index`), or the
    /// source's own (`model.requests`).
    pub kind: String,
    /// What it is, in the owner's words: "Preparing “Eigenvalues”".
    pub title: String,
    /// What it works on, by the name another item would call it when it
    /// waits for this one: the course, the folder, the model. `None` to use
    /// the title.
    pub subject: Option<String>,
    pub state: ActivityState,
    /// What it is doing, or what it waits for, in a few words.
    pub detail: Option<String>,
    pub progress: Option<ActivityProgress>,
    /// Units of `progress` per second, measured; never a guess.
    pub per_second: Option<f64>,
    /// Seconds left at the measured pace; `None` without one.
    pub seconds_left: Option<u64>,
    /// What it holds or uses while it runs.
    pub uses: Vec<ActivityResource>,
    pub target: Option<ActivityTarget>,
    /// When it started, or was asked for while it waits (Unix ms).
    pub since: Option<i64>,
    /// When a job put off by an outage tries again (Unix ms).
    pub retry_at: Option<i64>,
    /// The line it waits in, when it waits behind others of its sort: a
    /// waiting item queues behind the running one with the same key.
    pub queue: Option<String>,
    /// For a shared resource (a model's backend): the requests waiting for
    /// it, which the items using it are told.
    pub queued: Option<usize>,
    /// The passage index it embeds, named by its directory as the index
    /// turn order names it.
    pub index_dir: Option<PathBuf>,
}

impl ActivityItem {
    /// An item with nothing but its identity, title and state.
    pub fn new(
        id: impl Into<String>,
        kind: impl Into<String>,
        title: impl Into<String>,
        state: ActivityState,
    ) -> Self {
        Self {
            id: id.into(),
            kind: kind.into(),
            title: title.into(),
            subject: None,
            state,
            detail: None,
            progress: None,
            per_second: None,
            seconds_left: None,
            uses: Vec::new(),
            target: None,
            since: None,
            retry_at: None,
            queue: None,
            queued: None,
            index_dir: None,
        }
    }

    /// The name another item calls it by when it waits for this one.
    pub fn subject_name(&self) -> &str {
        self.subject.as_deref().unwrap_or(&self.title)
    }

    /// A live job: running or waiting, its saved message as the detail,
    /// since it started (or was asked for), and when an outage put it off,
    /// when it tries again. Its owner adds the rest.
    pub fn of_job(job: &JobRecord, title: impl Into<String>) -> Self {
        let state = match job.status {
            JobStatus::Running => ActivityState::Running,
            _ => ActivityState::Waiting,
        };
        let waiting = state == ActivityState::Waiting;
        let mut item = Self::new(job.id.clone(), job.kind.clone(), title, state)
            .detail(job.progress_message.clone())
            .since(job.started_at.filter(|_| !waiting).or(Some(job.created_at)));
        item.retry_at = job.retry_not_before.filter(|_| waiting);
        item
    }

    pub fn detail(mut self, detail: impl Into<String>) -> Self {
        let detail = detail.into();
        self.detail = (!detail.trim().is_empty()).then_some(detail);
        self
    }

    pub fn progress(mut self, progress: Option<ActivityProgress>) -> Self {
        self.progress = progress.filter(|progress| progress.total > 0);
        self
    }

    /// A measured pace, with the time it leaves for what remains of
    /// `progress`. A pace that is not positive is no pace.
    pub fn pace(mut self, per_second: Option<f64>) -> Self {
        let per_second = per_second.filter(|rate| rate.is_finite() && *rate > 0.0);
        self.per_second = per_second;
        self.seconds_left = match (&self.progress, per_second) {
            (Some(progress), Some(rate)) => {
                let left = progress.total.saturating_sub(progress.done) as f64;
                Some((left / rate).ceil() as u64)
            }
            _ => None,
        };
        self
    }

    pub fn uses(mut self, resource: ActivityResource) -> Self {
        if !self.uses.contains(&resource) {
            self.uses.push(resource);
        }
        self
    }

    pub fn target(mut self, target: ActivityTarget) -> Self {
        self.target = Some(target);
        self
    }

    pub fn since(mut self, since: Option<i64>) -> Self {
        self.since = since;
        self
    }

    pub fn subject(mut self, subject: impl Into<String>) -> Self {
        self.subject = Some(subject.into());
        self
    }

    pub fn queue(mut self, queue: impl Into<String>) -> Self {
        self.queue = Some(queue.into());
        self
    }

    pub fn index_dir(mut self, dir: Option<PathBuf>) -> Self {
        self.index_dir = dir;
        self
    }
}

/// A part of the app that runs long work or holds a shared resource, telling
/// what it has live now. A read only: it starts, stops and changes nothing.
#[async_trait]
pub trait ActivitySource: Send + Sync {
    /// Its items running, waiting or paused now. Finished work is left out.
    async fn activity(&self) -> Result<Vec<ActivityItem>>;
}
