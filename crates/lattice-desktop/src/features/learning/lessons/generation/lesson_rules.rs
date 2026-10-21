//! The lesson's own rules beyond its schema: distinct, titled sections in the
//! mix the lesson is written for, and distinct, answerable questions of every
//! kind. A broken rule is a finding: the teaching repair is given it, and
//! what the repairs cannot settle waits for the user. It never stops the
//! lesson from publishing.
use super::{normalize, validate_text, MAX_BODY, MAX_EXPLANATION, MAX_OPTION, MAX_QUESTION};
use crate::features::learning::review_findings::{FindingKind, FindingSite, ReviewFinding};
use serde_json::Value;
use std::collections::{HashMap, HashSet};

/// The sections a lesson is written with: at least, or with a minimum of
/// one exactly, this many of each kind.
const SECTION_MIX: [(&str, usize, &str); 6] = [
    ("explanation", 2, "explanations"),
    ("worked_example", 2, "worked examples"),
    ("guided_practice", 1, "guided practice"),
    ("independent_practice", 1, "independent practice"),
    ("reflection", 1, "reflection"),
    ("recap", 1, "recap"),
];

/// Questions a lesson asks of each kind, at least.
const QUESTIONS_PER_KIND: usize = 2;

fn text<'a>(item: &'a Value, field: &str) -> &'a str {
    item.get(field).and_then(Value::as_str).unwrap_or_default()
}

fn structure(site: FindingSite, note: String) -> ReviewFinding {
    ReviewFinding::new(FindingKind::Structure, Some(site), note)
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
        .map(|(kind, minimum, name)| (kinds(blocks, kind), *minimum, *name))
        .collect();
    if counts
        .iter()
        .any(|(count, minimum, _)| *count < *minimum || (*minimum == 1 && *count != 1))
    {
        let has: Vec<_> = counts
            .iter()
            .map(|(count, _, name)| format!("{count} {name}"))
            .collect();
        findings.push(structure(
            FindingSite::Lesson,
            format!("The lesson needs at least two explanations and two worked examples, and exactly one each of guided practice, independent practice, reflection and recap; it has {}.", has.join(", ")),
        ));
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
        json!({"kind":kind,"title":title,"body":"Body."})
    }

    fn question(kind: &str, prompt: &str) -> Value {
        json!({"kind":kind,"prompt":prompt,"options":["One","Two","Three","Four"],"explanation":"Because."})
    }

    fn lesson() -> Value {
        json!({"blocks":[
            block("explanation","First idea"), block("explanation","Second idea"),
            block("worked_example","First example"), block("worked_example","Second example"),
            block("guided_practice","Guided"), block("independent_practice","On your own"),
            block("reflection","Reflect"), block("recap","Recap"),
        ],"questions":[
            question("practice","P1?"), question("practice","P2?"), question("quiz","Q1?"),
            question("quiz","Q2?"), question("test","T1?"), question("test","T2?"),
        ]})
    }

    #[test]
    fn a_lesson_in_its_mix_breaks_no_rule() {
        assert!(lesson_rules(&lesson()).is_empty());
    }

    #[test]
    fn each_broken_rule_is_a_finding_at_its_place() {
        let mut candidate = lesson();
        candidate["blocks"][1]["title"] = json!("first  IDEA");
        candidate["blocks"][7]["kind"] = json!("explanation");
        candidate["questions"][3]["prompt"] = json!("q1?");
        candidate["questions"][4]["options"] = json!(["Yes", "yes", "Maybe", "No way"]);
        let findings = lesson_rules(&candidate);
        let sites: Vec<_> = findings.iter().map(|f| f.key().site).collect();
        assert_eq!(
            sites,
            [
                FindingSite::Section(1),
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

    #[test]
    fn too_few_questions_of_a_kind_is_one_lesson_finding() {
        let mut candidate = lesson();
        candidate["questions"].as_array_mut().unwrap().truncate(5);
        let findings = lesson_rules(&candidate);
        assert_eq!(findings.len(), 1);
        assert!(findings[0].note.ends_with("2 practice, 2 quiz, 1 test."));
    }
}
