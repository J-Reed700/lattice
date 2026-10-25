//! The lesson's own rules beyond its schema: distinct, titled sections in the
//! mix the lesson is written for, checks that hold their answer back, an
//! order that tests what was taught and recalls before it recaps, and
//! distinct, answerable questions of every kind. A broken rule is a finding:
//! the teaching repair is given it, and what the repairs cannot settle waits
//! for the user. It never stops the lesson from publishing.
use super::{normalize, validate_text, MAX_BODY, MAX_EXPLANATION, MAX_OPTION, MAX_QUESTION};
use crate::features::learning::review_findings::{FindingKind, FindingSite, ReviewFinding};
use serde_json::Value;
use std::collections::{HashMap, HashSet};

/// The heading a check section's reveal starts under (learning science
/// lessons §3.3): the learner commits to an answer to what is above it
/// before what is below it is shown. `checkReveal.ts` splits on the same line.
pub(in crate::features::learning) const CHECK_REVEAL: &str = "### Check your thinking";

/// The least a check holds on either side of its reveal: a case to reason
/// through above it, an answer with its reasoning below.
const CHECK_PART_MIN: usize = 80;

/// The sections a lesson is written with: at least this many of each kind,
/// or exactly this many when `exact`.
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

/// A check's case and its reveal, split at its `CHECK_REVEAL` line: `None`
/// unless exactly one line is that heading (at level two to four). Claim
/// extraction uses it to know a reveal's misconceptions are named as wrong.
pub(in crate::features::learning) fn split_check(body: &str) -> Option<(&str, &str)> {
    let is_reveal = |line: &str| {
        let line = line.trim();
        let level = line.chars().take_while(|c| *c == '#').count();
        (2..=4).contains(&level)
            && line
                .get(level..)
                .is_some_and(|title| title.trim().eq_ignore_ascii_case("Check your thinking"))
    };
    let mut offset = 0;
    let mut found = None;
    for line in body.split_inclusive('\n') {
        if is_reveal(line) {
            if found.is_some() {
                return None;
            }
            found = Some((offset, offset + line.len()));
        }
        offset += line.len();
    }
    let (start, end) = found?;
    Some((body.get(..start)?, body.get(end..)?))
}

/// Questions a lesson asks of each kind, at least.
const QUESTIONS_PER_KIND: usize = 2;

fn text<'a>(item: &'a Value, field: &str) -> &'a str {
    item.get(field).and_then(Value::as_str).unwrap_or_default()
}

fn structure(site: FindingSite, note: String) -> ReviewFinding {
    ReviewFinding::new(FindingKind::Structure, Some(site), note)
}

/// What `lesson_rules` reports, as its notes: for evals outside the crate,
/// which assert a reviewed lesson keeps the lesson's form.
pub fn lesson_rule_notes(candidate: &Value) -> Vec<String> {
    lesson_rules(candidate)
        .into_iter()
        .map(|finding| finding.note)
        .collect()
}

pub(in crate::features::learning) fn lesson_rules(candidate: &Value) -> Vec<ReviewFinding> {
    let items = |field| {
        candidate
            .get(field)
            .and_then(Value::as_array)
            .map(Vec::as_slice)
            .unwrap_or_default()
    };
    let (blocks, questions) = (items("blocks"), items("questions"));
    let mut findings = Vec::new();
    let mut titles = HashMap::new();
    for (index, block) in blocks.iter().enumerate() {
        let section = format!("Section {} (candidate blocks-{index})", index + 1);
        let title = text(block, "title");
        if !validate_text(title, 160) {
            findings.push(structure(
                FindingSite::Section(index),
                format!("{section} needs a title of 3–160 characters."),
            ));
        } else if let Some(first) = titles.get(&normalize(title)) {
            findings.push(structure(
                FindingSite::Section(index),
                format!("{section} repeats the title of section {}; give each section a distinct title.", first + 1),
            ));
        } else {
            titles.insert(normalize(title), index);
        }
        if text(block, "body").trim().chars().count() > MAX_BODY {
            findings.push(structure(
                FindingSite::Section(index),
                format!("{section} is longer than {MAX_BODY} characters; tighten it without dropping what it teaches."),
            ));
        }
    }
    let kinds = |items: &[Value], kind: &str| {
        items
            .iter()
            .filter(|item| item.get("kind").and_then(Value::as_str) == Some(kind))
            .count()
    };
    let counts: Vec<_> = SECTION_MIX
        .iter()
        .map(|(kind, minimum, exact)| (*kind, kinds(blocks, kind), *minimum, *exact))
        .collect();
    if counts
        .iter()
        .any(|(_, count, minimum, exact)| *count < *minimum || (*exact && *count != *minimum))
    {
        let has: Vec<_> = counts
            .iter()
            .map(|(kind, count, _, _)| format!("{count} {}", kind.replace('_', " ")))
            .collect();
        findings.push(structure(
            FindingSite::Lesson,
            format!("The lesson needs at least two explanations, two worked examples, one contrast and two checks, and exactly one each of guided practice, independent practice, reflection and recap; it has {}.", has.join(", ")),
        ));
    }
    let kind_at = |index: usize| blocks.get(index).map(|block| text(block, "kind"));
    let first = |kind: &str| (0..blocks.len()).find(|index| kind_at(*index) == Some(kind));
    // Each check applies an idea a worked example has shown since the
    // previous check: two checks in a row, or a check before its idea's
    // example, test what the lesson has not yet worked through.
    let mut example_since_check = false;
    for (index, block) in blocks.iter().enumerate() {
        match text(block, "kind") {
            "check" => {}
            "worked_example" => {
                example_since_check = true;
                continue;
            }
            _ => continue,
        }
        let section = format!("Section {} (candidate blocks-{index})", index + 1);
        let held_back = split_check(text(block, "body")).is_some_and(|(case, reveal)| {
            case.trim().chars().count() >= CHECK_PART_MIN
                && reveal.trim().chars().count() >= CHECK_PART_MIN
        });
        if !held_back {
            findings.push(structure(
                FindingSite::Section(index),
                format!("{section} is a check: write the case to reason through first, then one line `{CHECK_REVEAL}`, then the answer, the reasoning and the misconception each plausible wrong answer reveals. Nothing above that line may give the answer away."),
            ));
        }
        if !example_since_check {
            findings.push(structure(
                FindingSite::Section(index),
                format!("{section} checks an idea no worked example has shown since the previous check; place each check after the example of the idea it asks the learner to apply."),
            ));
        }
        example_since_check = false;
    }
    if let (Some(reflection), Some(recap)) = (first("reflection"), first("recap")) {
        if reflection > recap {
            findings.push(structure(
                FindingSite::Section(recap),
                format!("Section {} (candidate blocks-{recap}) recaps before the reflection asks the learner to explain the lesson from memory; put the reflection first, so the recap is what they compare their explanation with.", recap + 1),
            ));
        }
    }
    let mut prompts = HashMap::new();
    for (index, question) in questions.iter().enumerate() {
        let label = format!("Question {}", index + 1);
        let site = FindingSite::Question(index);
        let prompt = text(question, "prompt");
        if !validate_text(prompt, MAX_QUESTION) {
            findings.push(structure(
                site,
                format!("{label} needs a prompt of 3–{MAX_QUESTION} characters."),
            ));
        } else if let Some(first) = prompts.get(&normalize(prompt)) {
            findings.push(structure(
                site,
                format!(
                    "{label} repeats question {}; ask something distinct.",
                    first + 1
                ),
            ));
        } else {
            prompts.insert(normalize(prompt), index);
        }
        if !validate_text(text(question, "explanation"), MAX_EXPLANATION) {
            findings.push(structure(
                site,
                format!("{label} needs an answer explanation of 3–{MAX_EXPLANATION} characters."),
            ));
        }
        let options: Vec<_> = question
            .get("options")
            .and_then(Value::as_array)
            .map(|options| {
                options
                    .iter()
                    .map(|o| o.as_str().unwrap_or_default())
                    .collect()
            })
            .unwrap_or_default();
        let mut choices = HashSet::new();
        if options.len() != 4
            || options.iter().any(|option| {
                !validate_text(option, MAX_OPTION) || !choices.insert(normalize(option))
            })
        {
            findings.push(structure(
                site,
                format!(
                    "{label} needs four distinct answer choices of 3–{MAX_OPTION} characters each."
                ),
            ));
        }
    }
    let per_kind: Vec<_> = ["practice", "quiz", "test"]
        .iter()
        .map(|kind| (*kind, kinds(questions, kind)))
        .collect();
    if per_kind
        .iter()
        .any(|(_, count)| *count < QUESTIONS_PER_KIND)
    {
        let has: Vec<_> = per_kind
            .iter()
            .map(|(kind, count)| format!("{count} {kind}"))
            .collect();
        findings.push(structure(
            FindingSite::Lesson,
            format!("The lesson needs at least two distinct questions each for practice, quiz and test; it has {}.", has.join(", ")),
        ));
    }
    findings
}

#[cfg(test)]
mod tests {
    #![allow(clippy::indexing_slicing, clippy::unwrap_used)]
    use super::*;
    use serde_json::json;

    fn block(kind: &str, title: &str) -> Value {
        let body = if kind == "check" {
            format!("{CASE}\n\n{CHECK_REVEAL}\n\n{REVEAL}")
        } else {
            "Body.".to_owned()
        };
        json!({"kind":kind,"title":title,"body":body})
    }

    const CASE: &str = "A sealed jar of air is carried from the beach up a mountain. Predict what happens to its lid, and say why.";
    const REVEAL: &str = "The lid bulges: outside pressure falls with altitude while the air inside keeps its pressure. If you said it caves in, you reversed which side lost pressure.";

    fn question(kind: &str, prompt: &str) -> Value {
        json!({"kind":kind,"prompt":prompt,"options":["One","Two","Three","Four"],"explanation":"Because."})
    }

    fn lesson() -> Value {
        json!({"blocks":[
            block("explanation","First idea"), block("worked_example","First example"),
            block("contrast","Not to be confused with"), block("check","Your turn"),
            block("explanation","Second idea"), block("worked_example","Second example"),
            block("check","Your turn again"), block("guided_practice","Guided"),
            block("independent_practice","On your own"), block("reflection","From memory"),
            block("recap","Recap"),
        ],"questions":[
            question("practice","P1?"), question("practice","P2?"), question("quiz","Q1?"),
            question("quiz","Q2?"), question("test","T1?"), question("test","T2?"),
        ]})
    }

    #[test]
    fn rule_notes_are_the_findings_notes() {
        let mut candidate = lesson();
        candidate["blocks"].as_array_mut().unwrap().swap(5, 6);
        let notes = lesson_rule_notes(&candidate);
        assert_eq!(notes.len(), 1);
        assert!(notes[0].contains("no worked example has shown"));
        assert!(lesson_rule_notes(&lesson()).is_empty());
    }

    #[test]
    fn a_lesson_in_its_mix_breaks_no_rule() {
        assert!(lesson_rules(&lesson()).is_empty());
    }

    #[test]
    fn each_broken_rule_is_a_finding_at_its_place() {
        let mut candidate = lesson();
        candidate["blocks"][4]["title"] = json!("first  IDEA");
        candidate["blocks"][10]["kind"] = json!("explanation");
        candidate["questions"][3]["prompt"] = json!("q1?");
        candidate["questions"][4]["options"] = json!(["Yes", "yes", "Maybe", "No way"]);
        let findings = lesson_rules(&candidate);
        let sites: Vec<_> = findings.iter().map(|f| f.key().site).collect();
        assert_eq!(
            sites,
            [
                FindingSite::Section(4),
                FindingSite::Lesson,
                FindingSite::Question(3),
                FindingSite::Question(4),
            ]
        );
        assert!(findings[0].note.contains("repeats the title of section 1"));
        assert!(findings[1].note.contains("0 recap"));
        assert!(findings
            .iter()
            .all(|finding| finding.kind == FindingKind::Structure));
    }

    /// A check that gives its answer away, or has nothing to reason through,
    /// is no check: the reveal line must split a case from its answer.
    #[test]
    fn a_check_holds_its_answer_back_under_one_reveal_line() {
        assert_eq!(
            split_check(&format!("{CASE}\n#### check your thinking\n{REVEAL}")),
            Some((format!("{CASE}\n").as_str(), REVEAL))
        );
        for body in [
            format!("{CASE} {REVEAL}"),
            format!("{CASE}\n{CHECK_REVEAL}\nToo short."),
            format!("Too short.\n{CHECK_REVEAL}\n{REVEAL}"),
            format!("{CASE}\n{CHECK_REVEAL}\n{REVEAL}\n{CHECK_REVEAL}\n{REVEAL}"),
        ] {
            let mut candidate = lesson();
            candidate["blocks"][3]["body"] = json!(body);
            let findings = lesson_rules(&candidate);
            assert_eq!(findings.len(), 1, "{body}");
            assert_eq!(findings[0].key().site, FindingSite::Section(3));
            assert!(findings[0].note.contains(CHECK_REVEAL));
        }
    }

    /// A check tests what an example has shown; the learner explains the
    /// lesson from memory before the recap they compare it with.
    #[test]
    fn checks_follow_an_example_and_recall_comes_before_the_recap() {
        let mut candidate = lesson();
        let blocks = candidate["blocks"].as_array_mut().unwrap();
        blocks.swap(1, 3);
        blocks.swap(9, 10);
        let findings = lesson_rules(&candidate);
        let sites: Vec<_> = findings.iter().map(|f| f.key().site).collect();
        assert_eq!(sites, [FindingSite::Section(1), FindingSite::Section(9)]);
        assert!(findings[0].note.contains("no worked example has shown"));
        assert!(findings[1].note.contains("from memory"));
    }

    /// The second idea's check before the second idea's example tests what
    /// the lesson has not worked through, even after the first idea's example.
    #[test]
    fn a_check_needs_its_own_ideas_example_since_the_last_check() {
        let mut candidate = lesson();
        candidate["blocks"].as_array_mut().unwrap().swap(5, 6);
        let findings = lesson_rules(&candidate);
        let sites: Vec<_> = findings.iter().map(|f| f.key().site).collect();
        assert_eq!(sites, [FindingSite::Section(5)]);
    }

    #[test]
    fn too_few_questions_of_a_kind_is_one_lesson_finding() {
        let mut candidate = lesson();
        candidate["questions"].as_array_mut().unwrap().truncate(5);
        let findings = lesson_rules(&candidate);
        assert_eq!(findings.len(), 1);
        assert!(findings[0].note.ends_with("2 practice, 2 quiz, 1 test."));
    }
}
