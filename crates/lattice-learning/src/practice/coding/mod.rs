//! Coding sessions: a proven problem, worked through with a coach or an interviewer
//! (docs/design/2026-11-08-coding-sessions-and-one-practice-area.md).

pub mod builds;
pub mod checks;
pub mod debrief;
pub mod dto;
pub mod examples;
pub mod harness;
pub mod hints;
pub mod jobs;
pub mod observation;
pub mod patterns;
pub mod problem;
pub mod proof;
pub mod records;
pub mod session;
pub mod solution_check;
pub mod source;
pub mod store;

#[cfg(test)]
pub(crate) mod golden_support;
#[cfg(test)]
mod migration_tests;
#[cfg(test)]
mod session_tests;
