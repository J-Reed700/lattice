//! The one judgement of a review against a drill's key, whoever wrote the review: a learner in practice or an
//! interview, the Tutor's chat, or the blind reviewer that completes a key while a drill is built.

use super::build::SourcePassage;
use async_trait::async_trait;
use lattice_core::domain::learning::drills::{Assessment, Drill, DrillFile, Finding};
use lattice_core::shared::error::Result;

/// A review to judge: the drill as the reviewer saw it, its material before any issue was planted, the passage an
/// explanation was written from, and the review's findings in the order they were made.
pub struct Review<'a> {
    pub drill: &'a Drill,
    pub clean: &'a [DrillFile],
    /// `None` for code.
    pub source: Option<&'a SourcePassage>,
    pub findings: &'a [Finding],
}

/// Decides which findings name which of the key's issues, which name real problems the key does not hold, and which
/// are not problems.
#[async_trait]
pub trait ReviewGrader: Send + Sync {
    /// An assessment that passes `drills::validate` for this review, or an error when none could be had: a learner
    /// then reads "Couldn't grade yet", never a guess.
    async fn assess(&self, review: &Review<'_>) -> Result<Assessment>;
}
