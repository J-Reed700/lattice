//! Code review drills (docs/design/2026-11-04-code-review-drills.md): building a drill whose key is exact and
//! proven, the bank that keeps the next drill ready, a learner's attempt at a drill, and grading its review and its
//! fix against that key. The pure rules are lattice-core's `domain::learning::drills`.

mod attempt_grading;
pub mod attempts;
pub mod bank;
mod budget;
pub(in crate::features::learning) mod build;
pub mod dto;
pub(in crate::features::learning) mod fix_grading;
pub mod grader;
pub mod grading;
pub mod jobs;
pub(in crate::features::learning) mod material;
pub(in crate::features::learning) mod proof;
pub mod repository;
mod reveal;
pub mod review;
pub(in crate::features::learning) mod runner;
pub(in crate::features::learning) mod schedules;
pub mod spec;

#[cfg(test)]
mod tests;
