//! The brief of the lesson a drill is built from: what a model is told of the course, the module and the lesson, and
//! the evidence of what the lesson teaches that the issues of a drill cite by number.

use super::drill::{LessonLink, LessonSentence};
use serde::{Deserialize, Serialize};

/// What a drill is told of the lesson it is built from: the course, the module, the lesson, and evidence of what the
/// lesson teaches. A build makes it once and keeps it in its first checkpoint, so every later stage sees the same.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LessonBrief {
    pub lesson_id: String,
    /// The course's title.
    pub course: String,
    pub goal: String,
    pub prior_knowledge: String,
    /// The module's title.
    pub module: String,
    pub module_summary: String,
    pub outcomes: Vec<String>,
    /// The lesson's title.
    pub lesson: String,
    pub objective: String,
    /// Numbered from 1 when shown: L1, L2, ...
    pub evidence: Vec<LessonSentence>,
}

impl LessonBrief {
    /// The brief as a model is shown it.
    pub fn render(&self) -> String {
        String::new()
    }

    /// The link of an issue that cites the evidence `numbers` (1-based): the cited units, cloned, in the order of
    /// `numbers`. `None` when `numbers` is empty or any number is 0 or beyond the evidence.
    pub fn link(&self, _numbers: &[usize]) -> Option<LessonLink> {
        None
    }
}

/// The units a lesson's text is cut into for evidence: each fenced block of code whole, and the prose around the
/// blocks cut into sentences.
pub fn evidence_units(_text: &str) -> Vec<String> {
    Vec::new()
}
