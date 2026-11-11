//! Learning Studio concepts that hold rules of their own.
//!
//! `claim_review` is the ledger of a lesson's factual claims: what the
//! checker and the user did about each one, and the rules that bound the
//! machine's work and keep the user's verdicts.
//!
//! `drills` is code review practice: code with proven defects planted as
//! exact edits, the grading of a learner's findings against that key, and the
//! schedule of the patterns the defects are instances of.

pub mod acceptance;
pub mod claim_review;
pub mod drills;
