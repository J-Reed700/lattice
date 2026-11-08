//! Code review drills (docs/design/2026-11-04-code-review-drills.md): building a drill whose key is exact and
//! proven, the bank that keeps the next drill ready, a learner's attempt at a drill, and grading its review and its
//! fix against that key. The pure rules are lattice-core's `domain::learning::drills`.

mod attempt_grading;
pub mod attempts;
pub mod bank;
pub(crate) mod brief;
mod budget;
pub mod build;
pub mod dto;
pub(crate) mod fix_grading;
pub mod grader;
pub mod grading;
pub mod jobs;
pub(crate) mod lesson;
pub mod material;
pub(crate) mod proof;
pub mod repository;
mod reveal;
pub mod review;
pub mod runner;
pub(crate) mod schedules;
pub mod spec;
#[cfg(any(test, feature = "test-support"))]
#[path = "tests/support.rs"]
pub mod test_support;

#[cfg(test)]
mod tests;
