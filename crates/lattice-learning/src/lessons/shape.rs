//! The lesson's shape (acceptance policy design §2.8): what the writer is asked
//! for, what the lesson is held to once verification has spent some of it, and
//! what a cut may take. One policy answers all of it, so the writer's schema,
//! the lesson's rules, the cuts, the save and what a model is told of the
//! shape read the same decision.
//!
//! A lesson is held to one of two promises (`Phase`). The draft is held to the
//! margin: the writer is asked for more than a lesson needs (a mix of
//! sections, two questions of each kind) so that verification has something to
//! spend. A lesson verification has begun to cut is held to the floor: a first
//! explanation, at least one question, a third of the drafted sections at
//! most gone, and the order rules (each check after a worked example shown
//! since the previous check, but the last of two or more checks). The phase
//! is the caller's to name; nothing infers it from the lesson.
use crate::dto::{LearningBlockKind, LessonQuestionKind};
use crate::review_findings::{join_plain, FindingKind, FindingSite, ReviewFinding, Rule};
use serde_json::Value;
use std::collections::BTreeSet;

#[path = "block_kinds.rs"]
mod block_kinds;

/// Which promise a lesson is held to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Phase {
    /// The writer's draft, held to the margin it was asked for.
    Draft,
    /// A lesson verification has cut or may cut, held to the floor.
    Kept,
}

/// The key a request states a lesson's shape under.
pub(crate) const WORDS: &str = "lessonShape";

/// A lesson schema and the promise it holds a lesson to. Only the shape
/// policy builds one, so the two cannot disagree.
#[derive(Clone, Debug)]
pub(crate) struct LessonSchema {
    phase: Phase,
    value: Value,
}

impl LessonSchema {
    pub(crate) fn phase(&self) -> Phase {
        self.phase
    }

    pub(crate) fn value(&self) -> &Value {
        &self.value
    }

    pub(crate) fn into_value(self) -> Value {
        self.value
    }

    /// The same promise for `source_count` sources, once verification has
    /// numbered more than the writer was given.
    pub(crate) fn numbered(&self, shape: &dyn ShapePolicy, source_count: usize) -> Self {
        shape.schema(self.phase, source_count)
    }
}

/// A lesson schema with the policy that built it: what a review or a repair
/// of a lesson is given.
#[derive(Clone, Copy)]
pub(crate) struct LessonShape<'a> {
    pub(crate) schema: &'a LessonSchema,
    pub(crate) policy: &'a dyn ShapePolicy,
}

impl<'a> LessonShape<'a> {
    pub(crate) fn new(schema: &'a LessonSchema, policy: &'a dyn ShapePolicy) -> Self {
        Self { schema, policy }
    }

    pub(crate) fn value(&self) -> &'a Value {
        self.schema.value()
    }

    pub(crate) fn phase(&self) -> Phase {
        self.schema.phase()
    }

    /// States in `request`, the writer's request, the shape this lesson is held
    /// to now: what a model that reviews or repairs the lesson reads.
    pub(crate) fn state_in(&self, request: &mut Value) {
        if let Some(request) = request.as_object_mut() {
            request.insert(
                WORDS.to_owned(),
                serde_json::json!(self.policy.asked(self.phase())),
            );
        }
    }
}

/// The one reader of a lesson's shape.
pub(crate) trait ShapePolicy: Send + Sync {
    /// The lesson schema for `phase` and `source_count` saved sources. Both
    /// phases have the same item shapes; `Kept` has none of the margin's
    /// minimums.
    fn schema(&self, phase: Phase, source_count: usize) -> LessonSchema;

    /// What a lesson in `phase` is held to, in the words a model is given: the
    /// writer is asked for the draft's, and a review or a repair is told the
    /// words of the phase the lesson is in.
    fn asked(&self, phase: Phase) -> Vec<String>;

    /// Every rule finding of `lesson` in `phase`: the shape's about the count
    /// and the order of its sections and questions, and the rules of its parts.
    fn rules(&self, lesson: &Value, phase: Phase) -> Vec<ReviewFinding>;

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
}

/// The policy every lesson is held to.
pub(crate) fn policy() -> &'static dyn ShapePolicy {
    &StandardShape
}

pub(crate) struct StandardShape;

/// The core ideas a lesson teaches: two or three.
const IDEAS: (usize, usize) = (2, 3);

/// The sections a lesson is written with: at least this many of each kind, or
/// exactly this many when `exact`. Each idea has one section of each kind that
/// is not exact; the exact kinds close the lesson.
const SECTION_MIX: [(LearningBlockKind, usize, bool); 8] = [
    (LearningBlockKind::Explanation, IDEAS.0, false),
    (LearningBlockKind::WorkedExample, IDEAS.0, false),
    (LearningBlockKind::Contrast, 1, false),
    (LearningBlockKind::Check, IDEAS.0, false),
    (LearningBlockKind::GuidedPractice, 1, true),
    (LearningBlockKind::IndependentPractice, 1, true),
    (LearningBlockKind::Reflection, 1, true),
    (LearningBlockKind::Recap, 1, true),
];

/// The answer choices every lesson question offers; the learner picks one.
pub(crate) const CHOICES_PER_QUESTION: usize = 4;

/// Questions the writer is asked for of each kind (`LessonQuestionKind::ALL`),
/// so that a cut can spend one; a kept lesson needs one question of any kind.
const QUESTIONS_PER_KIND: usize = 2;

/// The fewest sections the writer is asked for: the mix at its least.
const fn least_sections(mix: &[(LearningBlockKind, usize, bool)]) -> usize {
    match mix {
        [] => 0,
        [(_, least, _), rest @ ..] => *least + least_sections(rest),
    }
}

/// The most sections a lesson has: the most ideas, each with a section of
/// every kind that is not exact, then the exact closing sections. Cuts never
/// add, so a lesson never has more.
const fn most_sections(mix: &[(LearningBlockKind, usize, bool)]) -> usize {
    match mix {
        [] => 0,
        [(_, least, exact), rest @ ..] => {
            (if *exact { *least } else { IDEAS.1 }) + most_sections(rest)
        }
    }
}

const SECTIONS: (usize, usize) = (least_sections(&SECTION_MIX), most_sections(&SECTION_MIX));

/// A section that took a cut and has fewer teaching sentences than this has
/// nothing left to teach with.
const LEAST_TEACHING_SENTENCES: usize = 2;

/// What a kept lesson is held to, each rule with its words.
#[derive(Clone, Copy)]
enum KeptRule {
    Explanation,
    ShownBeforeCheck,
    RecapAfterReflection,
    Question,
}

const KEPT_RULES: [KeptRule; 4] = [
    KeptRule::Explanation,
    KeptRule::ShownBeforeCheck,
    KeptRule::RecapAfterReflection,
    KeptRule::Question,
];

impl KeptRule {
    fn words(self) -> &'static str {
        match self {
            Self::Explanation => "at least one explanation",
            Self::ShownBeforeCheck => "each check after a worked example shown since the previous check, except the last of two or more checks, which mixes the lesson's ideas",
            Self::RecapAfterReflection => "the recap after the reflection when it has both",
            Self::Question => "at least one question",
        }
    }

    fn findings(self, lesson: &Value) -> Vec<ReviewFinding> {
        let blocks = items(lesson, "blocks");
        match self {
            Self::Explanation => explanation_finding(blocks).into_iter().collect(),
            Self::ShownBeforeCheck => unshown_check_findings(blocks),
            Self::RecapAfterReflection => recap_findings(blocks),
            Self::Question => question_finding(items(lesson, "questions"))
                .into_iter()
                .collect(),
        }
    }
}

/// The words a kept lesson is held to, joined as a sentence's list.
fn kept_words() -> String {
    let words: Vec<&str> = KEPT_RULES.iter().map(|rule| rule.words()).collect();
    match words.split_last() {
        Some((last, rest)) => format!("{}; and {last}", rest.join("; ")),
        None => String::new(),
    }
}

impl StandardShape {
    /// The findings about the count and the order of `lesson`'s sections and
    /// questions in `phase`.
    pub(crate) fn shape_findings(&self, lesson: &Value, phase: Phase) -> Vec<ReviewFinding> {
        match phase {
            Phase::Draft => {
                let blocks = items(lesson, "blocks");
                section_mix_finding(blocks)
                    .into_iter()
                    .chain(order_findings(blocks))
                    .chain(question_count_finding(items(lesson, "questions")))
                    .collect()
            }
            Phase::Kept => KEPT_RULES
                .into_iter()
                .flat_map(|rule| rule.findings(lesson))
                .collect(),
        }
    }
}

impl ShapePolicy for StandardShape {
    fn schema(&self, phase: Phase, source_count: usize) -> LessonSchema {
        let kinds = LessonQuestionKind::ALL.len();
        let asked = QUESTIONS_PER_KIND * kinds;
        let (sections, questions) = match phase {
            Phase::Draft => (SECTIONS, (asked, asked)),
            Phase::Kept => ((1, SECTIONS.1), (1, asked)),
        };
        LessonSchema {
            phase,
            value: crate::generation::lesson_schema_counting(source_count, sections, questions),
        }
    }

    fn asked(&self, phase: Phase) -> Vec<String> {
        let kinds = question_kinds();
        match phase {
            Phase::Draft => vec![
                format!("Teach through the {} or {} core ideas the objective depends on, the ones coursePlan lists for this lesson: fewer ideas understood deeply, not a survey. For each idea write an explanation, then a worked_example, then a check. Where learners plausibly mistake an idea for something similar, put a contrast between its example and its check; the lesson has at least one. After the ideas come one guided_practice, one independent_practice, one reflection and one recap, in that order: {} to {} blocks.", number(IDEAS.0), number(IDEAS.1), SECTIONS.0, SECTIONS.1),
                format!(
                    "Create exactly {} distinct multiple-choice items of each kind: {kinds}.",
                    number(QUESTIONS_PER_KIND)
                ),
            ],
            Phase::Kept => vec![
                "Verification takes out of a lesson what no source confirms, so this lesson is no longer held to the number of sections and questions it was drafted with.".to_owned(),
                format!("It is held to this instead: {}.", kept_words()),
            ],
        }
    }

    fn rules(&self, lesson: &Value, phase: Phase) -> Vec<ReviewFinding> {
        crate::generation::lesson_rules(lesson, self.shape_findings(lesson, phase))
    }

    fn too_little(&self, body: &str, answer_lost: bool) -> bool {
        answer_lost || crate::generation::teaching_sentences(body).len() < LEAST_TEACHING_SENTENCES
    }

    fn dependants(&self, lesson: &Value, going: &BTreeSet<usize>) -> BTreeSet<usize> {
        let blocks = items(lesson, "blocks");
        let failing_before = unshown_checks(blocks, &BTreeSet::new());
        unshown_checks(blocks, going)
            .difference(&failing_before)
            .copied()
            .collect()
    }

    fn floor(&self, lesson: &Value, going: &BTreeSet<usize>) -> Option<String> {
        let blocks = lesson.get("blocks")?.as_array()?;
        let sections = blocks.len();
        let lost = (0..sections).filter(|unit| going.contains(unit)).count();
        let first = blocks
            .iter()
            .position(|block| text(block, "kind") == LearningBlockKind::Explanation.as_str());
        let questions = items(lesson, "questions");
        let questions_lost = (sections..sections + questions.len())
            .filter(|unit| going.contains(unit))
            .count();
        if first.is_some_and(|unit| going.contains(&unit)) {
            Some("its first explanation would have to be taken out".to_owned())
        } else if 3 * lost > sections {
            Some(format!(
                "{lost} of its {sections} sections would have to be taken out"
            ))
        } else if !questions.is_empty() && questions_lost == questions.len() {
            Some("all of its questions would have to be taken out".to_owned())
        } else {
            None
        }
    }
}

/// The standard policy with a schema of the test's own, for tests of what the
/// schema is not about.
#[cfg(test)]
pub(crate) struct FixedSchema(pub(crate) Value);

#[cfg(test)]
impl ShapePolicy for FixedSchema {
    fn schema(&self, phase: Phase, _source_count: usize) -> LessonSchema {
        LessonSchema {
            phase,
            value: self.0.clone(),
        }
    }

    fn asked(&self, phase: Phase) -> Vec<String> {
        StandardShape.asked(phase)
    }

    fn rules(&self, _lesson: &Value, _phase: Phase) -> Vec<ReviewFinding> {
        Vec::new()
    }

    fn too_little(&self, body: &str, answer_lost: bool) -> bool {
        StandardShape.too_little(body, answer_lost)
    }

    fn dependants(&self, lesson: &Value, going: &BTreeSet<usize>) -> BTreeSet<usize> {
        StandardShape.dependants(lesson, going)
    }

    fn floor(&self, lesson: &Value, going: &BTreeSet<usize>) -> Option<String> {
        StandardShape.floor(lesson, going)
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

fn structure(rule: Rule, site: FindingSite, summary: String, note: String) -> ReviewFinding {
    ReviewFinding::rule(rule, FindingKind::Structure, site, note).with_summary(summary)
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

const NUMBER_WORDS: [&str; 11] = [
    "zero", "one", "two", "three", "four", "five", "six", "seven", "eight", "nine", "ten",
];

/// `count` as a word, or in digits past ten.
fn number(count: usize) -> String {
    NUMBER_WORDS
        .get(count)
        .map_or_else(|| count.to_string(), |word| (*word).to_owned())
}

fn kind_name(kind: &str) -> String {
    kind.replace('_', " ")
}

/// What the writer is asked for of the sections, in words, from `SECTION_MIX`:
/// "at least two explanations, ... and one each of guided practice, ...";
/// `exactly` goes before "one each" when it is not empty.
fn mix_asked(exactly: &str) -> String {
    let mix = &SECTION_MIX;
    let wanted = |exact: bool| mix.iter().filter(move |(_, _, is)| *is == exact);
    let ideas: Vec<String> = wanted(false)
        .map(|(kind, least, _)| {
            format!(
                "{} {}{}",
                number(*least),
                kind_name(kind.as_str()),
                if *least == 1 { "" } else { "s" }
            )
        })
        .collect();
    let closing: Vec<String> = wanted(true)
        .map(|(kind, _, _)| kind_name(kind.as_str()))
        .collect();
    let counts: Vec<usize> = wanted(true).map(|(_, count, _)| *count).collect();
    let same = counts.iter().all(|count| Some(count) == counts.first());
    let closing = match counts.first() {
        Some(each) if same => format!("{} each of {}", number(*each), join_plain(&closing)),
        _ => join_plain(
            &wanted(true)
                .map(|(kind, count, _)| format!("{} {}", number(*count), kind_name(kind.as_str())))
                .collect::<Vec<_>>(),
        ),
    };
    format!("at least {}, and {exactly}{closing}", join_plain(&ideas))
}

/// What the writer is asked for of the questions, in words: "at least two
/// questions each for practice, quiz and test"; `adjective` goes before
/// "questions" when it is not empty.
fn questions_asked(adjective: &str) -> String {
    format!(
        "at least {} {adjective}questions each for {}",
        number(QUESTIONS_PER_KIND),
        question_kinds()
    )
}

/// The question kinds, as the words join them: "practice, quiz and test".
fn question_kinds() -> String {
    let kinds: Vec<String> = LessonQuestionKind::ALL
        .iter()
        .map(|kind| kind.as_str().to_owned())
        .collect();
    join_plain(&kinds)
}

/// The sections, other than those in `going`, that are checks and follow no
/// worked example since the previous check: two checks in a row, or a check
/// before its idea's example, test what the lesson has not yet worked through.
fn unshown_checks(blocks: &[Value], going: &BTreeSet<usize>) -> BTreeSet<usize> {
    let mut shown = false;
    let mut found = BTreeSet::new();
    for (index, block) in blocks.iter().enumerate() {
        if going.contains(&index) {
            continue;
        }
        match LearningBlockKind::parse(text(block, "kind")) {
            Some(LearningBlockKind::WorkedExample) => shown = true,
            Some(LearningBlockKind::Check) => {
                if !shown {
                    found.insert(index);
                }
                shown = false;
            }
            _ => {}
        }
    }
    found
}

/// The last of two or more checks: the one the writer mixes the lesson's ideas
/// in, which needs no worked example of its own.
fn mixed_check(blocks: &[Value]) -> Option<usize> {
    let checks: Vec<usize> = blocks
        .iter()
        .enumerate()
        .filter(|(_, block)| text(block, "kind") == LearningBlockKind::Check.as_str())
        .map(|(index, _)| index)
        .collect();
    if checks.len() >= 2 {
        checks.last().copied()
    } else {
        None
    }
}

fn order_findings(blocks: &[Value]) -> Vec<ReviewFinding> {
    let mut found = unshown_check_findings(blocks);
    found.extend(recap_findings(blocks));
    found
}

fn unshown_check_findings(blocks: &[Value]) -> Vec<ReviewFinding> {
    let mixed = mixed_check(blocks);
    unshown_checks(blocks, &BTreeSet::new())
        .into_iter()
        .filter(|index| mixed != Some(*index))
        .map(|index| {
            let site = FindingSite::Section(index);
            structure(
                Rule::UnshownCheck,
                site,
                format!("{} is a check on an idea that no worked example has shown since the previous check.", site.label()),
                format!("{} checks an idea no worked example has shown since the previous check; place each check after the example of the idea it asks the learner to apply.", site.note_label()),
            )
        })
        .collect()
}

fn recap_findings(blocks: &[Value]) -> Vec<ReviewFinding> {
    let first = |kind: LearningBlockKind| {
        blocks
            .iter()
            .position(|block| text(block, "kind") == kind.as_str())
    };
    let (Some(reflection), Some(recap)) = (
        first(LearningBlockKind::Reflection),
        first(LearningBlockKind::Recap),
    ) else {
        return Vec::new();
    };
    if reflection < recap {
        return Vec::new();
    }
    let site = FindingSite::Section(recap);
    vec![structure(
        Rule::RecapFirst,
        site,
        format!("{} recaps the lesson before the learner is asked to explain it from memory.", site.label()),
        format!("{} recaps before the reflection asks the learner to explain the lesson from memory; put the reflection first, so the recap is what they compare their explanation with.", site.note_label()),
    )]
}

fn section_mix_finding(blocks: &[Value]) -> Option<ReviewFinding> {
    let counts: Vec<_> = SECTION_MIX
        .iter()
        .map(|(kind, minimum, exact)| {
            (
                kind.as_str(),
                kind_count(blocks, kind.as_str()),
                *minimum,
                *exact,
            )
        })
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
        Rule::SectionMix,
        FindingSite::Lesson,
        format!(
            "The lesson should have {}. It has {}.",
            mix_asked(""),
            plain.join(", ")
        ),
        format!(
            "The lesson needs {}; it has {}.",
            mix_asked("exactly "),
            has.join(", ")
        ),
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
        Rule::QuestionCount,
        FindingSite::Lesson,
        format!(
            "The lesson needs {}. It has {}.",
            questions_asked(""),
            plain.join(", ")
        ),
        format!(
            "The lesson needs {}; it has {}.",
            questions_asked("distinct "),
            has.join(", ")
        ),
    ))
}

fn explanation_finding(blocks: &[Value]) -> Option<ReviewFinding> {
    if kind_count(blocks, LearningBlockKind::Explanation.as_str()) > 0 {
        return None;
    }
    Some(structure(
        Rule::NoExplanation,
        FindingSite::Lesson,
        "The lesson has no explanation.".to_owned(),
        "The lesson has no explanation section; it needs one, which teaches an idea before a check or a question asks about it.".to_owned(),
    ))
}

fn question_finding(questions: &[Value]) -> Option<ReviewFinding> {
    if !questions.is_empty() {
        return None;
    }
    Some(structure(
        Rule::NoQuestion,
        FindingSite::Lesson,
        "The lesson has no question.".to_owned(),
        "The lesson has no question; it needs at least one.".to_owned(),
    ))
}

#[cfg(test)]
#[path = "shape_tests.rs"]
mod tests;
