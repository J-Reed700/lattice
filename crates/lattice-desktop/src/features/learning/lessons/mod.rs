//! Lessons capabilities owned by Learning Studio.

pub(crate) mod answer_review;
pub mod claim_reviews;
pub(crate) mod content_verification;
#[cfg(test)]
pub(crate) mod course_generation_tests;
#[cfg(test)]
mod first_start_check;
pub mod generation;
pub(crate) mod generation_jobs;
pub(crate) mod lesson_drafts;
pub mod lesson_evidence;
pub(crate) mod lesson_progress;
pub mod reference_collection;
pub(crate) mod review_evidence;
pub(crate) mod review_findings;
pub(crate) mod structured_call;
pub mod teaching;
pub(crate) mod teaching_review;
mod text_edits;
/// The Verifier audit (lesson claim review §14). Its files live under
/// `content_verification/audit/`; declared here so that module's list stays
/// track L1's. Integration may move the declaration there.
#[path = "content_verification/audit/mod.rs"]
pub(crate) mod verifier_audit;
