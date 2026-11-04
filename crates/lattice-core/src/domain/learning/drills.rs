//! Code review drills: code with known, proven defects planted in it, the
//! catalogue of patterns those defects are instances of, the edits that plant
//! them and give the key its line spans, the grading of a learner's findings,
//! and what the outcomes mean for each pattern's schedule. Pure types and
//! rules, no I/O.
//!
//! Design: docs/design/2026-11-04-code-review-drills.md §2–§4.

mod catalogue;
mod drill;
mod edits;
mod grading;
mod schedule;
#[cfg(test)]
mod tests;

pub use catalogue::{pattern, patterns_for, Family, Languages, Pattern, CATALOGUE, LANGUAGES};
pub use drill::{weight, Drill, DrillFile, Edit, KeyIssue, LineSpan, Origin, Proof, Severity};
pub use edits::{apply, overlaps, EditFault};
pub use grading::{
    overlapping, score, validate, Assessment, AssessmentFault, Finding, FindingOutcome,
    IssueOutcome, Score, SeverityTally,
};
pub use schedule::{rating, targets, PatternDue, PatternRating};
