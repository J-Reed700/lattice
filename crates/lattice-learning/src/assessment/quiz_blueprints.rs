//! Which questions each quiz asks: blueprints derived from the course, never
//! authored by the renderer (docs/design/2026-10-23-quizzes-and-tests.md,
//! sections 5.1 and 5.3).
//!
//! `blueprint_for` is a pure function of the scope's prepared lessons, their
//! outcomes and the course's quiz settings. `ensure_blueprint_in` stores its
//! result as a new revision only when it differs from the latest one, so old
//! forms keep the revision they were drawn from.
use crate::assessment_engine::{
    LearningAssessmentPurpose, LearningBlueprintRequirement, LearningItemFormat,
};
use crate::dto::{LearningQuizScopeDto, LearningQuizScopeKind, LearningQuizSettingsDto};
use std::collections::BTreeSet;

mod names;
pub(crate) mod repository;
mod store;
pub use names::{format_name, parse_format, purpose_name, validate_scope};
pub use store::{
    course_facts_in, ensure_blueprint_in, ensure_blueprint_with, ensure_lesson_outcome_in,
    latest_blueprint_in, lesson_quiz_formats_in, requirements_in, settings_in, BlueprintRef,
};

/// A prepared or outline lesson, as a blueprint sees it.
#[derive(Debug, Clone, PartialEq)]
pub struct LessonFacts {
    pub id: String,
    pub module_id: String,
    pub title: String,
    pub objective: String,
    pub prepared: bool,
    /// The lesson-level outcome; every prepared lesson has one.
    pub outcome_id: Option<String>,
    pub source_version_ids: Vec<String>,
    /// The formats the lesson's latest quiz blueprint asks; empty before its
    /// first. A copied course carries its blueprints and banks under fresh
    /// lesson IDs, so the mix already stored is kept rather than picked again.
    pub quiz_formats: Vec<LearningItemFormat>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ModuleFacts {
    pub id: String,
    pub title: String,
    /// The module's own outcomes, in order.
    pub outcome_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CourseFacts {
    pub program_title: String,
    pub modules: Vec<ModuleFacts>,
    pub lessons: Vec<LessonFacts>,
}

/// One requirement and the points each of its items is worth.
#[derive(Debug, Clone, PartialEq)]
pub struct Slot {
    pub requirement: LearningBlueprintRequirement,
    /// 1.0 for keyed items. Written answers are worth their rubric's total,
    /// which is known only per item, so their slot records 1.0 and the form
    /// takes the item's rubric total.
    pub points: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DesiredBlueprint {
    pub purpose: LearningAssessmentPurpose,
    pub title: String,
    pub instructions: String,
    pub expected_minutes: u32,
    pub passing_score: f64,
    pub slots: Vec<Slot>,
    pub source_version_ids: Vec<String>,
    /// For module tests and the exam: prepared lessons covered, of all.
    pub coverage: Option<(usize, usize)>,
}

pub fn default_settings() -> LearningQuizSettingsDto {
    LearningQuizSettingsDto {
        quiz_pass: 0.7,
        test_pass: 0.75,
        exam_pass: 0.75,
        written_answers: true,
    }
}

/// A stable choice from the lesson ID, so a lesson's first quiz mix never
/// changes between preparations; once stored, the blueprint keeps it.
fn stable_pick(seed: &str, choices: usize) -> usize {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in seed.bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    (hash % choices.max(1) as u64) as usize
}

fn requirement(
    outcome_id: &str,
    format: LearningItemFormat,
    count: usize,
    min: u8,
    max: u8,
) -> Slot {
    Slot {
        requirement: LearningBlueprintRequirement {
            outcome_id: outcome_id.to_string(),
            format,
            count,
            difficulty_min: min,
            difficulty_max: max,
        },
        points: 1.0,
    }
}

fn minutes_for(format: LearningItemFormat) -> u32 {
    match format {
        LearningItemFormat::SingleChoice | LearningItemFormat::TrueFalse => 1,
        LearningItemFormat::Explanation | LearningItemFormat::Artifact => 6,
        _ => 2,
    }
}

fn scope_title(prefix: &str, title: &str) -> String {
    format!("{prefix}: {}", title.trim())
}

/// The blueprint a scope should have now, or None when it has nothing to ask
/// yet (an outline lesson; a module or course with no prepared lesson).
pub fn blueprint_for(
    scope: &LearningQuizScopeDto,
    facts: &CourseFacts,
    settings: &LearningQuizSettingsDto,
) -> Option<DesiredBlueprint> {
    let prepared = |lesson: &&LessonFacts| lesson.prepared && lesson.outcome_id.is_some();
    let mut slots = Vec::new();
    // Earlier lessons a lesson quiz asks again.
    let mut review: Vec<&LessonFacts> = Vec::new();
    let (purpose, title, passing, scoped): (_, _, _, Vec<&LessonFacts>) = match scope.kind {
        LearningQuizScopeKind::Lesson => {
            let lesson = facts
                .lessons
                .iter()
                .find(|l| Some(&l.id) == scope.id.as_ref())
                .filter(prepared)?;
            let outcome = lesson.outcome_id.as_deref()?;
            let structured = [
                LearningItemFormat::MultiSelect,
                LearningItemFormat::Matching,
                LearningItemFormat::Ordering,
            ];
            let typed = [LearningItemFormat::ShortAnswer, LearningItemFormat::Cloze];
            let pick = |choices: &[LearningItemFormat], salt: &str| {
                choices
                    .iter()
                    .copied()
                    .find(|format| lesson.quiz_formats.contains(format))
                    .or_else(|| {
                        choices
                            .get(stable_pick(&format!("{}:{salt}", lesson.id), choices.len()))
                            .copied()
                    })
                    .unwrap_or(LearningItemFormat::SingleChoice)
            };
            if lesson.quiz_formats.contains(&LearningItemFormat::TrueFalse) {
                // A lesson whose blueprint already asks the earlier mix keeps
                // it, so its bank (or a copied course's) is not written again.
                slots.push(requirement(
                    outcome,
                    LearningItemFormat::SingleChoice,
                    4,
                    2,
                    3,
                ));
                slots.push(requirement(outcome, LearningItemFormat::TrueFalse, 1, 2, 2));
                slots.push(requirement(
                    outcome,
                    pick(&structured, "structured"),
                    1,
                    3,
                    3,
                ));
                slots.push(requirement(outcome, pick(&typed, "typed"), 1, 3, 3));
            } else {
                // A mix with more answers the learner produces: short-answer
                // practice with feedback outlasted multiple choice (Kang,
                // McDermott & Roediger 2007; McDaniel et al. 2007), while choice
                // items with competitive distractors also drive retrieval
                // (Little et al. 2012). Both typed formats replace a true/false
                // item, a coin-flip guess, and one of four single-choice items.
                slots.push(requirement(
                    outcome,
                    LearningItemFormat::SingleChoice,
                    3,
                    2,
                    3,
                ));
                slots.push(requirement(
                    outcome,
                    pick(&structured, "structured"),
                    1,
                    3,
                    3,
                ));
                for format in typed {
                    slots.push(requirement(outcome, format, 1, 3, 3));
                }
            }
            // Earlier lessons come back: one item from the lesson just before
            // and one from a lesson further back, drawn from their own banks
            // at the difficulty those were written for, so nothing new is
            // authored. Retrieval spaced across sessions and mixed with new
            // material is what keeps it (Rawson & Dunlosky 2011; Rohrer et al.
            // 2020; Rosenshine 2012's weekly and monthly review).
            let earlier: Vec<&LessonFacts> = facts
                .lessons
                .iter()
                .take_while(|l| l.id != lesson.id)
                .filter(prepared)
                .collect();
            if let Some((last, further)) = earlier.split_last() {
                review.push(*last);
                if !further.is_empty() {
                    if let Some(back) =
                        further.get(stable_pick(&format!("{}:review", lesson.id), further.len()))
                    {
                        review.push(*back);
                    }
                }
            }
            for revisited in &review {
                if let Some(earlier_outcome) = revisited.outcome_id.as_deref() {
                    slots.push(requirement(
                        earlier_outcome,
                        LearningItemFormat::SingleChoice,
                        1,
                        2,
                        3,
                    ));
                }
            }
            if settings.written_answers {
                slots.push(requirement(
                    outcome,
                    LearningItemFormat::Explanation,
                    1,
                    3,
                    4,
                ));
            }
            (
                LearningAssessmentPurpose::Checkpoint,
                scope_title("Lesson quiz", &lesson.title),
                settings.quiz_pass,
                vec![lesson],
            )
        }
        LearningQuizScopeKind::Module => {
            let module = facts
                .modules
                .iter()
                .find(|m| Some(&m.id) == scope.id.as_ref())?;
            let lessons: Vec<&LessonFacts> = facts
                .lessons
                .iter()
                .filter(|l| l.module_id == module.id)
                .filter(prepared)
                .collect();
            if lessons.is_empty() {
                return None;
            }
            for lesson in &lessons {
                let outcome = lesson.outcome_id.as_deref()?;
                slots.push(requirement(
                    outcome,
                    LearningItemFormat::SingleChoice,
                    2,
                    3,
                    4,
                ));
                slots.push(requirement(
                    outcome,
                    LearningItemFormat::ShortAnswer,
                    1,
                    3,
                    3,
                ));
            }
            if settings.written_answers {
                for outcome in &module.outcome_ids {
                    slots.push(requirement(
                        outcome,
                        LearningItemFormat::Explanation,
                        1,
                        3,
                        4,
                    ));
                }
            }
            (
                LearningAssessmentPurpose::ModuleTest,
                scope_title("Module test", &module.title),
                settings.test_pass,
                lessons,
            )
        }
        LearningQuizScopeKind::Course => {
            let lessons: Vec<&LessonFacts> = facts.lessons.iter().filter(prepared).collect();
            if lessons.is_empty() {
                return None;
            }
            for lesson in &lessons {
                let outcome = lesson.outcome_id.as_deref()?;
                slots.push(requirement(
                    outcome,
                    LearningItemFormat::SingleChoice,
                    1,
                    3,
                    4,
                ));
            }
            if settings.written_answers {
                for module in &facts.modules {
                    let covered = lessons.iter().any(|l| l.module_id == module.id);
                    if let (true, Some(outcome)) = (covered, module.outcome_ids.first()) {
                        slots.push(requirement(
                            outcome,
                            LearningItemFormat::Explanation,
                            1,
                            4,
                            4,
                        ));
                    }
                }
            }
            (
                LearningAssessmentPurpose::Cumulative,
                scope_title("Course exam", &facts.program_title),
                settings.exam_pass,
                lessons,
            )
        }
    };
    let total = match scope.kind {
        LearningQuizScopeKind::Lesson => 1,
        LearningQuizScopeKind::Module => facts
            .lessons
            .iter()
            .filter(|l| Some(&l.module_id) == scope.id.as_ref())
            .count(),
        LearningQuizScopeKind::Course => facts.lessons.len(),
    };
    let expected_minutes = slots
        .iter()
        .map(|s| minutes_for(s.requirement.format) * s.requirement.count as u32)
        .sum::<u32>()
        .max(1);
    let sources: BTreeSet<String> = scoped
        .iter()
        .flat_map(|l| l.source_version_ids.iter().cloned())
        .collect();
    Some(DesiredBlueprint {
        purpose,
        title,
        instructions: {
            let mut text = if settings.written_answers {
                "Answer every question. Questions you leave blank score zero. Written answers are graded after you submit, and each lost point comes with the reason.".to_string()
            } else {
                "Answer every question. Questions you leave blank score zero.".to_string()
            };
            if !review.is_empty() {
                text.push_str(" A few questions come from earlier lessons: recalling them again, spread out over time, is what makes them last.");
            }
            text
        },
        expected_minutes,
        passing_score: (passing * 10_000.0).round() / 10_000.0,
        slots,
        source_version_ids: sources.into_iter().collect(),
        coverage: (scope.kind != LearningQuizScopeKind::Lesson).then_some((scoped.len(), total)),
    })
}

/// One slot row per item, as stored.
fn slot_rows(desired: &DesiredBlueprint) -> Vec<(String, String, i64, i64, i64)> {
    desired
        .slots
        .iter()
        .flat_map(|slot| {
            let r = &slot.requirement;
            (0..r.count).map(move |_| {
                (
                    r.outcome_id.clone(),
                    format_name(r.format).to_string(),
                    i64::from(r.difficulty_min),
                    i64::from(r.difficulty_max),
                    (slot.points * 1000.0).round() as i64,
                )
            })
        })
        .collect()
}

#[cfg(test)]
#[path = "quiz_blueprints_tests.rs"]
mod tests;
