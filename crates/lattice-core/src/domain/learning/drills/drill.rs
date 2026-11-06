//! The types of a drill and its answer key: the files a learner reviews, the
//! defects planted in them as exact edits, and how each was proven.

use std::cmp::Ordering;

use serde::{Deserialize, Serialize};

/// How serious a known issue is. `Critical` is the greatest.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    Critical,
    Major,
    Minor,
    Nit,
}

impl Severity {
    /// Every severity, the most severe first.
    pub const ALL: [Severity; 4] = [
        Severity::Critical,
        Severity::Major,
        Severity::Minor,
        Severity::Nit,
    ];
}

impl Ord for Severity {
    fn cmp(&self, other: &Self) -> Ordering {
        weight(*self).cmp(&weight(*other))
    }
}

impl PartialOrd for Severity {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// What an issue of this severity counts for in a score: the more severe, the
/// more it weighs.
pub fn weight(severity: Severity) -> u32 {
    match severity {
        Severity::Critical => 4,
        Severity::Major => 3,
        Severity::Minor => 2,
        Severity::Nit => 1,
    }
}

/// One file of a drill, as the learner reads it: line endings are `\n`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DrillFile {
    pub path: String,
    pub text: String,
}

/// Whole lines of one file: 1-based, both ends included, `start <= end`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct LineSpan {
    pub path: String,
    pub start: u32,
    pub end: u32,
}

/// A text replacement in one file: the exact `before` text becomes `after`.
/// The edit that plants an issue is the clean code's `before` becoming the
/// flawed `after`; its fix is the reverse.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Edit {
    pub path: String,
    pub before: String,
    pub after: String,
}

/// What shows a planted issue is real.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Proof {
    /// A test that fails on the flawed code and passes on the clean code.
    Killed { test: String },
    /// An independent judge, shown both versions, confirmed that the edit
    /// introduces exactly this problem and that the clean code lacks it.
    Judged { reason: String },
}

/// Where a key issue came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Origin {
    /// Planted as an edit to the clean code.
    Planted,
    /// Reported by a blind review of the finished code and judged real
    /// against the clean code.
    FoundInReview,
}

/// One known issue of a drill.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeyIssue {
    pub id: String,
    /// The id of the catalogue pattern this issue is an instance of.
    pub pattern: String,
    pub severity: Severity,
    /// Where the issue is, in the drill's files: computed from the applied
    /// edit, never stated by a model.
    pub span: LineSpan,
    /// What goes wrong because of it.
    pub consequence: String,
    /// The edit that repairs it.
    pub fix: Edit,
    /// How to say it in an interview.
    pub explanation: String,
    pub proof: Proof,
    pub origin: Origin,
}

/// Code to review and what is known to be wrong with it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Drill {
    pub id: String,
    /// A language id of the catalogue.
    pub language: String,
    pub title: String,
    pub files: Vec<DrillFile>,
    pub key: Vec<KeyIssue>,
}
