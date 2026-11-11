//! Where a kind's drafts come from: the author role, and what a build tells it.

use async_trait::async_trait;
use lattice_core::application::ports::LLMPort;
use lattice_core::domain::learning::practice::work::{Draft, Form};
use lattice_core::shared::error::Result;
use serde_json::Value;

/// The lesson an exercise is built to practise.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Lesson {
    pub id: String,
    pub title: String,
    pub objective: String,
}

/// A draft the proof refused: what the author wrote, as it is sent back, and why it cannot stand.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Failed {
    pub raw: String,
    pub reason: String,
}

/// What an author is asked for.
pub struct Commission<'a> {
    pub program_id: &'a str,
    pub lesson: Option<&'a Lesson>,
    pub form: Form,
    /// The kind's own parameters, as the request carried them.
    pub options: &'a Value,
    /// What the kind's store told the build besides the request: the learner's schedule, the practice profile.
    pub facts: &'a Value,
    pub llm: &'a dyn LLMPort,
    /// The drafts this build has had refused so far, in order. An author asked again answers them.
    pub failed: &'a [Failed],
}

/// What an author came back with.
#[derive(Debug, Clone, PartialEq)]
pub enum Authored {
    Draft {
        draft: Box<Draft>,
        /// The answer as written, handed back in `Failed` if the proof refuses it.
        raw: String,
    },
    /// The lesson has nothing of this kind to make, and why.
    NothingToMake(String),
}

/// Writes a draft exercise for a lesson, or says the lesson has nothing of this kind to make.
#[async_trait]
pub trait Author: Send + Sync {
    async fn draft(&self, commission: &Commission<'_>) -> Result<Authored>;
}
