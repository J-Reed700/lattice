//! # Tutor
//!
//! A chat that teaches from a course (`docs/design/2026-10-28-tutor.md`): a
//! conversation bound to one course (`conversations.program_id`) and, when
//! opened from a lesson, to that lesson (`conversations.lesson_id`).
//!
//! [`CourseTurn`] is the course as a turn source: what a turn reads beside the
//! library, the tools it adds, and the tutor's prompt section ([`prompt`]), the
//! only place the study methods are spelled out. Deleting a course leaves its
//! tutor chats as ordinary chats; the foreign keys do that.

mod binding;
mod cards;
mod course_turn;
mod facts;
mod notes;
mod passages;
pub(crate) mod prompt;
mod repository;
mod tools;

#[cfg(test)]
mod binding_tests;
#[cfg(test)]
mod tests;

pub use binding::{bind_conversation, SetConversationCourseRequestDto};
pub use course_turn::CourseTurn;
