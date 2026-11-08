//! Review drills: code, or an explanation of a passage, with known, proven
//! defects planted in it, the catalogue of patterns those defects are
//! instances of, the edits that plant them and give the key its line spans, the
//! check a key passes before its drill is stored, the grading of a learner's
//! findings, what the outcomes mean for each pattern's schedule, and what came
//! of a learner's fix. Pure types and rules, no I/O.
//!
//! Design: docs/design/2026-11-04-code-review-drills.md §2–§4.

mod brief;
mod catalogue;
mod diff;
mod drill;
mod edits;
mod fix;
mod grading;
mod key;
mod lessons;
mod schedule;
#[cfg(test)]
mod tests;

pub use brief::{evidence_units, LessonBrief};
pub use catalogue::{
    language_of_code, pattern, patterns_for, Family, Languages, Pattern, CATALOGUE, LANGUAGES,
};
pub use diff::{line_diff, DiffKind, DiffLine};
pub use drill::{
    weight, Drill, DrillFile, Edit, KeyIssue, LessonLink, LessonSentence, LineSpan, Material,
    Origin, Proof, Severity, SourceVerdict,
};
pub use edits::{apply, overlaps, EditFault};
pub use fix::{DecidedBy, FixOutcome, IssueCheck, IssueFix, SuiteVerdict};
pub use grading::{
    display_order, overlapping, score, validate, Assessment, AssessmentFault, Finding,
    FindingOutcome, IssueOutcome, Score, SeverityTally,
};
pub use key::{check_key, severity_floor, KeyFault};
pub use lessons::{next_lessons, LessonStanding};
pub use schedule::{
    lesson_rating, per_pattern, rating, ratings, targets, IssueRating, PatternDue, PatternRating,
};
