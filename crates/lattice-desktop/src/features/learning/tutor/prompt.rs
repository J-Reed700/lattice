//! What a tutor turn tells the model: the template its evidence travels in,
//! which holds the study methods, and the block of facts each turn carries.
//! The harness supplies the facts; the model decides how to say them.

use lattice_core::application::ports::turn_source::TurnSourcePolicy;

/// The tutor's prompt section. Unlike a folder's it asks for citations: a
/// tutor teaches from the course's numbered passages, and the chat's citation
/// checks read them as they read library passages.
pub(crate) const TUTOR_TEMPLATE: &str =
    "You are tutoring the learner through the course below, one stretch at a time. \
     Teach from the course's numbered passages and cite them: right after every claim about the \
     subject, put the bracket number of the passage that supports it, like [1]. Never cite a \
     passage for what it does not say. When nothing here supports what you are about to say, read \
     more of the course first; if the course does not cover it, say so before you go on from what \
     you know. Your questions, your feedback on the learner's words and the running of the session \
     need no citation.\n\n\
     Work these into the conversation as each moment calls for it. Never list them or lecture \
     about them.\n\
     - When a newcomer would likely guess wrong, ask for a committed answer before you explain.\n\
     - After teaching a point, ask one question the learner answers before you move on, pitched so \
     that a learner who followed usually succeeds. Give feedback that names the misconception, not \
     only the right answer. A missed point becomes a card with save_card.\n\
     - When cards are due, open the session with them, unless the learner's message asks for \
     something else: then do that first and bring the cards in after. Ask them one at a time, as \
     written, mixed into the conversation; the facts below show the next ones and due_reviews \
     shows the rest. An answer to a card is a review: as soon as the learner answers a card you \
     asked, rate it with record_recall and its card_id, before you say anything else. The rating \
     comes from the learner's answer, not from your guess about it: when the answer alone does \
     not settle it, ask how it went.\n\
     - Sometimes ask the learner to explain a point as to a beginner, compare it with the passages \
     you taught it from, and name what was missing. Gaps become cards.\n\
     - Set a new idea beside the earlier one it is easily confused with, naming the feature that \
     differs. course_outline shows what came before.\n\
     - Draw a diagram in a mermaid code block when an idea has structure, never as decoration, and \
     use find_image when a real picture helps.\n\
     - At the end of a stretch, ask the learner to write the summary, point out what it left out, \
     and save their words with save_note.\n\n\
     Context:\n{context}\n\nThe learner says: {question}\n\nAnswer:";

/// How a turn treats the course. The question is about the course, so the
/// router does not start a library search of its own and the web comes only
/// when the learner turns it on, but the library's tools stay offered, as in
/// any chat. Every claim is checked, as in a chat, because a tutor cites. A
/// session reads, reviews and saves a few things per round, so it gets a
/// folder's rounds. It does not read its passages into notes first: the round
/// after the notes answers in the passages' words whatever the learner said,
/// which restated a card's answer and left the card unrated.
pub(crate) const TUTOR_POLICY: TurnSourcePolicy = TurnSourcePolicy {
    routes_to_library: false,
    offers_library_tools: true,
    infers_intent: false,
    verifies_cited_only: false,
    reads_passages: false,
    drops_session_talk: true,
    tool_rounds: 10,
    evidence_template: Some(TUTOR_TEMPLATE),
};

/// What a turn knows about the course and the learner before it starts.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct TurnFacts {
    pub title: String,
    pub goal: String,
    pub lessons_finished: usize,
    pub lessons: usize,
    /// The lesson the learner is on, as module › lesson.
    pub current_lesson: Option<String>,
    /// The lesson this chat was opened from, and its objective.
    pub focus: Option<(String, String)>,
    /// `None` when the schedule could not be read.
    pub cards_due: Option<i64>,
    /// The next due cards, as the model reads them.
    pub next_cards: Vec<String>,
    /// Tutor replies since the last that asked the learner something.
    pub replies_since_check: Option<i64>,
}

/// The block of facts one turn carries, cut to `max_chars`.
pub(super) fn render_context(facts: &TurnFacts, tools_offered: bool, max_chars: usize) -> String {
    let mut lines = vec![format!("Course: {}.", facts.title)];
    if !facts.goal.trim().is_empty() {
        lines.push(format!("The learner's goal: {}", facts.goal.trim()));
    }
    lines.push(format!(
        "Lessons finished: {} of {}.",
        facts.lessons_finished, facts.lessons
    ));
    if let Some(lesson) = &facts.current_lesson {
        lines.push(format!("Current lesson: {lesson}."));
    }
    if let Some((lesson, objective)) = &facts.focus {
        lines.push(format!(
            "This chat was opened from the lesson {lesson}. Teach from it first; its objective: {objective}"
        ));
    }
    if let Some(due) = facts.cards_due {
        lines.push(format!("Cards due now: {due}."));
    }
    if tools_offered && !facts.next_cards.is_empty() {
        lines.push(format!(
            "The next ones, each with the card_id to rate it by:\n{}",
            facts.next_cards.concat().trim_end()
        ));
    }
    if let Some(replies) = facts.replies_since_check {
        lines.push(format!(
            "Your replies since you last asked the learner a question: {replies}."
        ));
    }
    lines.push("A numbered passage labelled with a source_id is from this course.".to_string());
    lines.push(if tools_offered {
        "Read the course with search_course and read_course_source; each passage comes back with \
         the number to cite it by."
            .to_string()
    } else {
        "You cannot call tools this turn. Teach from what is shown here, and say so when the \
         answer needs more of the course. Cards and notes cannot be saved or rated this turn."
            .to_string()
    });
    lattice_core::shared::text::safe_truncate(&lines.join("\n"), max_chars)
}
