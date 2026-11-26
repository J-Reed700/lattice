//! Shared content-kind names, reactor names, frontend invalidation names, and key helpers.

use lattice_core::shared::edits::{ContentRef, EditScope};

pub const SECTION: &str = "learning.section";
pub const QUESTION: &str = "learning.question";
pub const LESSON: &str = "learning.lesson";
pub const MODULE: &str = "learning.module";
pub const CARD: &str = "study.card";

pub const CLAIMS_REACTOR: &str = "learning.claims";
pub const QUIZ_BANK_REACTOR: &str = "learning.quiz_bank";
pub const OUTLINE_REACTOR: &str = "learning.outline";
pub const DUPLICATES_REACTOR: &str = "study.duplicates";

pub mod affected {
    pub const PROGRAM: &str = "learning.program";
    pub const PLAN: &str = "learning.plan";
    pub const CLAIMS: &str = "learning.claims";
    pub const QUIZZES: &str = "learning.quizzes";
    pub const RECALL: &str = "learning.recall";
    pub const DECKS: &str = "study.decks";
}

pub fn section_ref(lesson_id: &str, ordinal: usize) -> ContentRef {
    ContentRef {
        kind: SECTION.into(),
        key: format!("{lesson_id}/{ordinal}"),
    }
}

pub fn section_key(key: &str) -> Option<(&str, usize)> {
    let (lesson_id, ordinal) = key.rsplit_once('/')?;
    if lesson_id.is_empty() || ordinal.is_empty() || !ordinal.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let parsed = ordinal.parse::<usize>().ok()?;
    Some((lesson_id, parsed))
}

pub fn question_ref(candidate_id: &str) -> ContentRef {
    ContentRef {
        kind: QUESTION.into(),
        key: candidate_id.into(),
    }
}

pub fn lesson_ref(lesson_id: &str) -> ContentRef {
    ContentRef {
        kind: LESSON.into(),
        key: lesson_id.into(),
    }
}

pub fn module_ref(module_id: &str) -> ContentRef {
    ContentRef {
        kind: MODULE.into(),
        key: module_id.into(),
    }
}

pub fn card_ref(card_id: &str) -> ContentRef {
    ContentRef {
        kind: CARD.into(),
        key: card_id.into(),
    }
}

pub fn learning_scope(program_id: &str) -> EditScope {
    EditScope {
        feature: "learning".into(),
        container: program_id.into(),
    }
}

pub fn study_scope(deck_id: &str) -> EditScope {
    EditScope {
        feature: "study".into(),
        container: deck_id.into(),
    }
}
