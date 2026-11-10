//! The lesson's shape (acceptance policy design §2.8): what the writer is asked
//! for, what the lesson is held to once verification has spent some of it, and
//! what a cut may take. One policy answers all of it, so the writer's schema,
//! the lesson's rules, the cuts and the save read the same decision.
//!
//! A lesson is held to one of two promises (`Phase`). The draft is held to the
//! margin: the writer is asked for more than a lesson needs (a mix of
//! sections, two questions of each kind) so that verification has something to
//! spend. A lesson verification has begun to cut is held to the floor: a first
//! explanation, a question of every kind, a third of the drafted sections at
//! most gone, and the order rules (each check after a worked example shown
//! since the previous check). The phase is the caller's to name; nothing infers
//! it from the lesson.
use crate::dto::LessonQuestionKind;
use crate::review_findings::{join_plain, FindingKind, FindingSite, ReviewFinding};
use serde_json::Value;
use std::collections::BTreeSet;

/// Which promise a lesson is held to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Phase {
    /// The writer's draft, held to the margin it was asked for.
    Draft,
    /// A lesson verification has cut or may cut, held to the floor.
    Kept,
}

/// The one reader of a lesson's shape.
pub(crate) trait ShapePolicy: Send + Sync {
    /// The lesson schema for `phase` and `source_count` saved sources. Both
    /// phases have the same item shapes; `Kept` has none of the margin's
    /// minimums.
    fn schema(&self, phase: Phase, source_count: usize) -> Value;

    /// The findings about the count and the order of `lesson`'s sections and
    /// questions in `phase`.
    fn findings(&self, lesson: &Value, phase: Phase) -> Vec<ReviewFinding>;

    /// Whether a section that took a cut, now `body`, is left with too little
    /// to teach; `answer_lost` says the cut took the answer of a check.
    fn too_little(&self, body: &str, answer_lost: bool) -> bool;

    /// The sections, beside those in `going`, that a cut takes with it: the
    /// checks that follow no worked example since the previous check once the
    /// sections that go are out, and did before. Units number the sections
    /// first, then the questions.
    fn dependants(&self, lesson: &Value, going: &BTreeSet<usize>) -> BTreeSet<usize>;

    /// Why `lesson` without the units in `going` could not be published, when
    /// it could not. `lesson` is the draft with every unit still in it, so
    /// "the drafted sections" are its own.
    fn floor(&self, lesson: &Value, going: &BTreeSet<usize>) -> Option<String>;

    /// The first question kind a lesson is saved with that `asked` lacks.
    fn missing_question_kind(&self, asked: &[LessonQuestionKind]) -> Option<LessonQuestionKind>;
}

/// The policy every lesson is held to.
pub(crate) fn policy() -> &'static dyn ShapePolicy {
    &StandardShape
}

pub(crate) struct StandardShape;

/// The sections a lesson is written with: at least this many of each kind, or
/// exactly this many when `exact`.
const SECTION_MIX: [(&str, usize, bool); 8] = [
    ("explanation", 2, false),
    ("worked_example", 2, false),
    ("contrast", 1, false),
    ("check", 2, false),
    ("guided_practice", 1, true),
    ("independent_practice", 1, true),
    ("reflection", 1, true),
    ("recap", 1, true),
];

/// Questions the writer is asked for of each kind, so that a cut can spend
/// one; a lesson is saved with one of each (`LessonQuestionKind::ALL`).
const QUESTIONS_PER_KIND: usize = 2;

/// The fewest sections the writer is asked for: the mix above at its least,
/// plus the ideas' own examples and checks. The writer's prompt asks for 10 to
/// 16.
const SECTIONS_ASKED: usize = 10;

/// The most sections a lesson has: what the writer's prompt allows. Cuts never
/// add, so a lesson never has more.
const MOST_SECTIONS: usize = 16;

/// A section that took a cut and has fewer teaching sentences than this has
/// nothing left to teach with.
const LEAST_TEACHING_SENTENCES: usize = 2;

impl ShapePolicy for StandardShape {
    fn schema(&self, phase: Phase, source_count: usize) -> Value {
        let kinds = LessonQuestionKind::ALL.len();
        let asked = QUESTIONS_PER_KIND * kinds;
        let (sections, questions) = match phase {
            Phase::Draft => ((SECTIONS_ASKED, MOST_SECTIONS), (asked, asked)),
            Phase::Kept => ((1, MOST_SECTIONS), (kinds, asked)),
        };
        crate::generation::lesson_schema_counting(source_count, sections, questions)
    }

    fn findings(&self, lesson: &Value, phase: Phase) -> Vec<ReviewFinding> {
        let blocks = items(lesson, "blocks");
        let questions = items(lesson, "questions");
        let mut found = Vec::new();
        found.extend(match phase {
            Phase::Draft => section_mix_finding(blocks),
            Phase::Kept => explanation_finding(blocks),
        });
        found.extend(order_findings(blocks));
        found.extend(match phase {
            Phase::Draft => question_count_finding(questions),
            Phase::Kept => question_kinds_finding(questions),
        });
        found
    }

    fn too_little(&self, body: &str, answer_lost: bool) -> bool {
        answer_lost || crate::generation::teaching_sentences(body).len() < LEAST_TEACHING_SENTENCES
    }

    fn dependants(&self, lesson: &Value, going: &BTreeSet<usize>) -> BTreeSet<usize> {
        let blocks = items(lesson, "blocks");
        let failing_before: BTreeSet<usize> = unshown_checks(blocks).into_iter().collect();
        let mut taken = BTreeSet::new();
        let mut shown = false;
        for (index, block) in blocks.iter().enumerate() {
            if going.contains(&index) {
                continue;
            }
            match text(block, "kind") {
                "worked_example" => shown = true,
                "check" => {
                    if !shown && !failing_before.contains(&index) {
                        taken.insert(index);
                    }
                    shown = false;
                }
                _ => {}
            }
        }
        taken
    }

    fn floor(&self, lesson: &Value, going: &BTreeSet<usize>) -> Option<String> {
        let blocks = lesson.get("blocks")?.as_array()?;
        let sections = blocks.len();
        let lost = (0..sections).filter(|unit| going.contains(unit)).count();
        let first = blocks
            .iter()
            .position(|block| text(block, "kind") == "explanation");
        let questions = items(lesson, "questions");
        let questions_lost = (sections..sections + questions.len())
            .filter(|unit| going.contains(unit))
            .count();
        let kind_lost = LessonQuestionKind::ALL.into_iter().find(|kind| {
            let asked: Vec<usize> = questions
                .iter()
                .enumerate()
                .filter(|(_, question)| text(question, "kind") == kind.as_str())
                .map(|(at, _)| sections + at)
                .collect();
            !asked.is_empty() && asked.iter().all(|unit| going.contains(unit))
        });
        if first.is_some_and(|unit| going.contains(&unit)) {
            Some("its first explanation would have to be taken out".to_owned())
        } else if 3 * lost > sections {
            Some(format!(
                "{lost} of its {sections} sections would have to be taken out"
            ))
        } else if !questions.is_empty() && questions_lost == questions.len() {
            Some("all of its questions would have to be taken out".to_owned())
        } else {
            kind_lost.map(|kind| format!("it would be left without a {} question", kind.as_str()))
        }
    }

    fn missing_question_kind(&self, asked: &[LessonQuestionKind]) -> Option<LessonQuestionKind> {
        LessonQuestionKind::ALL
            .into_iter()
            .find(|kind| !asked.contains(kind))
    }
}

fn items<'a>(lesson: &'a Value, list: &str) -> &'a [Value] {
    lesson
        .get(list)
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default()
}

fn text<'a>(item: &'a Value, field: &str) -> &'a str {
    item.get(field).and_then(Value::as_str).unwrap_or_default()
}

fn structure(site: FindingSite, summary: String, note: String) -> ReviewFinding {
    ReviewFinding::new(FindingKind::Structure, Some(site), note).with_summary(summary)
}

fn kind_count(items: &[Value], kind: &str) -> usize {
    items
        .iter()
        .filter(|item| text(item, "kind") == kind)
        .count()
}

/// "2 checks", "1 contrast": `count` of `noun`, in the plural unless it is one.
fn counted(count: usize, noun: &str) -> String {
    format!("{count} {noun}{}", if count == 1 { "" } else { "s" })
}

/// The sections that are checks and follow no worked example since the
/// previous check: two checks in a row, or a check before its idea's example,
/// test what the lesson has not yet worked through.
fn unshown_checks(blocks: &[Value]) -> Vec<usize> {
    let mut shown = false;
    let mut found = Vec::new();
    for (index, block) in blocks.iter().enumerate() {
        match text(block, "kind") {
            "worked_example" => shown = true,
            "check" => {
                if !shown {
                    found.push(index);
                }
                shown = false;
            }
            _ => {}
        }
    }
    found
}

fn order_findings(blocks: &[Value]) -> Vec<ReviewFinding> {
    let mut found = Vec::new();
    for index in unshown_checks(blocks) {
        let section = format!("Section {} (candidate blocks-{index})", index + 1);
        found.push(structure(
            FindingSite::Section(index),
            format!("Section {} is a check on an idea that no worked example has shown since the previous check.", index + 1),
            format!("{section} checks an idea no worked example has shown since the previous check; place each check after the example of the idea it asks the learner to apply."),
        ));
    }
    let first = |kind: &str| blocks.iter().position(|block| text(block, "kind") == kind);
    if let (Some(reflection), Some(recap)) = (first("reflection"), first("recap")) {
        if reflection > recap {
            found.push(structure(
                FindingSite::Section(recap),
                format!("Section {} recaps the lesson before the learner is asked to explain it from memory.", recap + 1),
                format!("Section {} (candidate blocks-{recap}) recaps before the reflection asks the learner to explain the lesson from memory; put the reflection first, so the recap is what they compare their explanation with.", recap + 1),
            ));
        }
    }
    found
}

fn section_mix_finding(blocks: &[Value]) -> Option<ReviewFinding> {
    let counts: Vec<_> = SECTION_MIX
        .iter()
        .map(|(kind, minimum, exact)| (*kind, kind_count(blocks, kind), *minimum, *exact))
        .collect();
    if !counts
        .iter()
        .any(|(_, count, minimum, exact)| *count < *minimum || (*exact && *count != *minimum))
    {
        return None;
    }
    let has: Vec<_> = counts
        .iter()
        .map(|(kind, count, _, _)| format!("{count} {}", kind.replace('_', " ")))
        .collect();
    let plain: Vec<_> = counts
        .iter()
        .map(|(kind, count, _, _)| counted(*count, &kind.replace('_', " ")))
        .collect();
    Some(structure(
        FindingSite::Lesson,
        format!(
            "The lesson should have at least two explanations, two worked examples, one contrast and two checks, and one each of guided practice, independent practice, reflection and recap. It has {}.",
            plain.join(", ")
        ),
        format!("The lesson needs at least two explanations, two worked examples, one contrast and two checks, and exactly one each of guided practice, independent practice, reflection and recap; it has {}.", has.join(", ")),
    ))
}

fn question_count_finding(questions: &[Value]) -> Option<ReviewFinding> {
    let per_kind: Vec<_> = LessonQuestionKind::ALL
        .iter()
        .map(|kind| (kind.as_str(), kind_count(questions, kind.as_str())))
        .collect();
    if !per_kind
        .iter()
        .any(|(_, count)| *count < QUESTIONS_PER_KIND)
    {
        return None;
    }
    let has: Vec<_> = per_kind
        .iter()
        .map(|(kind, count)| format!("{count} {kind}"))
        .collect();
    let plain: Vec<_> = per_kind
        .iter()
        .map(|(kind, count)| counted(*count, &format!("{kind} question")))
        .collect();
    Some(structure(
        FindingSite::Lesson,
        format!(
            "The lesson needs at least two questions each for practice, quiz and test. It has {}.",
            plain.join(", ")
        ),
        format!("The lesson needs at least two distinct questions each for practice, quiz and test; it has {}.", has.join(", ")),
    ))
}

fn explanation_finding(blocks: &[Value]) -> Option<ReviewFinding> {
    if kind_count(blocks, "explanation") > 0 {
        return None;
    }
    Some(structure(
        FindingSite::Lesson,
        "The lesson has no explanation.".to_owned(),
        "The lesson has no explanation section; it needs one, which teaches an idea before a check or a question asks about it.".to_owned(),
    ))
}

fn question_kinds_finding(questions: &[Value]) -> Option<ReviewFinding> {
    let missing: Vec<String> = LessonQuestionKind::ALL
        .iter()
        .filter(|kind| kind_count(questions, kind.as_str()) == 0)
        .map(|kind| kind.as_str().to_owned())
        .collect();
    if missing.is_empty() {
        return None;
    }
    Some(structure(
        FindingSite::Lesson,
        format!(
            "The lesson has no {} question, and it needs one of each kind.",
            join_plain(&missing)
        ),
        format!(
            "The lesson has no {} question; it needs at least one question of each kind.",
            join_plain(&missing)
        ),
    ))
}

#[cfg(test)]
#[path = "shape_tests.rs"]
mod tests;
