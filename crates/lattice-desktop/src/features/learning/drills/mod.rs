//! Code review drills (docs/design/2026-11-04-code-review-drills.md): building a drill whose key is exact and
//! proven, grading a review against that key, and the bank that keeps the next drill ready. The pure rules are
//! lattice-core's `domain::learning::drills`.

pub mod grader;
pub mod grading;
