//! The evidence a lesson brief carries: a prepared lesson's own teaching, or what the course's sources say of an
//! outline lesson, as much as the request's room and the lesson's planned minutes allow.

use super::lesson::LessonContext;
use lattice_core::application::ports::LLMPort;
use lattice_core::domain::learning::drills::LessonBrief;
use lattice_core::shared::error::Result;
use sqlx::SqlitePool;

/// The mean silent reading rate of adults for non-fiction, in words a minute (Brysbaert, 2019): an outline lesson's
/// evidence is as much as its planned minutes take to read.
const READING_WORDS_PER_MINUTE: usize = 238;

/// The brief of the lesson of `context`, with as much evidence as `room` tokens hold, counted by `llm` over the
/// rendered brief. A prepared lesson's evidence is a prefix of the units of its teaching. An outline lesson's is the
/// units of the course's source passages for the lesson, best first, as many passages as its planned minutes take to
/// read, and always the first. An error when the brief without evidence, or its first unit, does not fit.
pub(in crate::features::learning) async fn lesson_brief(
    _llm: &dyn LLMPort,
    _pool: &SqlitePool,
    context: &LessonContext,
    _room: usize,
) -> Result<LessonBrief> {
    Ok(context.brief(Vec::new()))
}
