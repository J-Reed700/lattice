//! Lessons capabilities owned by Learning Studio.

pub(crate) mod call_plan;
pub mod claim_reviews;
pub mod content_verification;
mod copied_wording;
#[cfg(test)]
pub(crate) mod course_generation_tests;
pub mod course_plan;
pub mod course_setup;
#[cfg(test)]
mod excerpt_copy_tests;
pub mod generation;
pub mod generation_jobs;
pub mod lesson_drafts;
pub mod lesson_evidence;
pub(crate) mod lesson_progress;
pub mod reference_collection;
pub(crate) mod repeats;
pub mod review_evidence;
pub mod review_findings;
pub(crate) mod same_text;
#[cfg(test)]
mod sentence_identity_tests;
pub(crate) mod structured_call;
pub mod teaching;
pub mod teaching_review;
mod text_edits;
/// The Verifier audit (lesson claim review §14). Its files live under
/// `content_verification/audit/`; declared here so that module's list stays
/// track L1's. Integration may move the declaration there.
#[path = "content_verification/audit/mod.rs"]
pub mod verifier_audit;
mod writer_excerpts;
