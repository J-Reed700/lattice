//! The lesson's own rules beyond its schema: distinct, titled sections in the
//! mix the lesson is written for, checks that hold their answer back, an
//! order that tests what was taught and recalls before it recaps, and
//! distinct, answerable questions of every kind, and a specific that stands
//! only in a sentence the writer cited. A broken rule is a finding:
//! the teaching repair is given it, and what the repairs cannot settle waits
//! for the user. It never stops the lesson from publishing.
use super::{normalize, validate_text};
use crate::content_verification::{figures, names_a_source, needs_citation, Givens, CITED_KINDS};
use crate::review_findings::{join_plain, FindingKind, FindingSite, ReviewFinding};
use lattice_core::shared::text::sentences;
use serde_json::Value;
use std::collections::{HashMap, HashSet};

/// The heading a check section's reveal starts under (learning science
/// lessons §3.3): the learner commits to an answer to what is above it
/// before what is below it is shown. `checkReveal.ts` splits on the same line.
pub(crate) const CHECK_REVEAL: &str = "### Check your thinking";

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
pub(crate) fn split_check(body: &str) -> Option<(&str, &str)> {
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

/// Questions the writer is asked for of each kind; a lesson is saved with
/// one of each (`LessonQuestionKind::ALL`).
const QUESTIONS_PER_KIND: usize = 2;

fn text<'a>(item: &'a Value, field: &str) -> &'a str {
    item.get(field).and_then(Value::as_str).unwrap_or_default()
}

/// A broken rule, with what is wrong in plain words for the user and the
/// note the repair is given.
fn structure(site: FindingSite, summary: String, note: String) -> ReviewFinding {
    ReviewFinding::new(FindingKind::Structure, Some(site), note).with_summary(summary)
}

/// A citation a part lacks. Not structure: it does not hold up the content
/// review, and the repair that follows settles it by appending the citation.
fn citation(
    site: FindingSite,
    summary: String,
    note: String,
    (quote, sentence): (&str, &str),
) -> ReviewFinding {
    ReviewFinding::new(FindingKind::Quotation, Some(site), note)
        .with_summary(summary)
        .with_passage(quote)
        .with_sentence(sentence)
}

/// "2 checks", "1 contrast": `count` of `noun`, in the plural unless it is one.
fn counted(count: usize, noun: &str) -> String {
    format!("{count} {noun}{}", if count == 1 { "" } else { "s" })
}

/// `sentence` without the Markdown that opens it, a heading's hashes or a
/// list item's marker, which is no part of what it states.
fn without_marker(sentence: &str) -> &str {
    let text = sentence.trim_start_matches('#').trim_start();
    if let Some(rest) = ["- ", "* ", "+ "]
        .iter()
        .find_map(|bullet| text.strip_prefix(bullet))
    {
        return rest.trim_start();
    }
    let digits = text.chars().take_while(char::is_ascii_digit).count();
    let rest = text.get(digits..).unwrap_or_default();
    if digits > 0 && (rest.starts_with(". ") || rest.starts_with(") ")) {
        return rest.get(2..).unwrap_or_default().trim_start();
    }
    text
}

/// `text` without its code fences.
fn prose(text: &str) -> String {
    let mut prose = String::new();
    let mut fenced = false;
    for line in text.split_inclusive('\n') {
        if line.trim_start().starts_with("```") {
            fenced = !fenced;
        } else if !fenced {
            prose.push_str(line);
        }
    }
    prose
}

/// The sentences of `text` that teach: its prose outside code fences, without
/// headings (claim classes design §3.10).
pub(crate) fn teaching_sentences(text: &str) -> Vec<String> {
    let prose = prose(text);
    sentences(&prose)
        .into_iter()
        .filter_map(|(from, to)| prose.get(from..to))
        .filter(|sentence| !sentence.starts_with('#'))
        .map(|sentence| without_marker(sentence.trim()).to_owned())
        .filter(|sentence| sentence.chars().any(char::is_alphabetic))
        .collect()
}

/// The sentences of `text` outside its code fences, as a writer lists them.
pub(crate) fn prose_sentences(text: &str) -> Vec<String> {
    let prose = prose(text);
    sentences(&prose)
        .into_iter()
        .filter_map(|(from, to)| prose.get(from..to))
        .map(|sentence| without_marker(sentence.trim()).to_owned())
        .filter(|sentence| sentence.chars().any(char::is_alphabetic))
        .collect()
}

/// The figures beyond its part's `givens` and the quoted wording of
/// `sentence`, which the quote cited for it must hold.
fn specifics(sentence: &str, givens: &Givens) -> Vec<String> {
    let own = figures(sentence)
        .filter(|figure| !givens.holds(figure))
        .map(str::to_owned);
    let quoted = lazy_regex::regex!(r#"['"‘’“”]([^'"‘’“”]{12,})['"‘’“”]"#)
        .captures_iter(sentence)
        .filter_map(|captures| captures.get(1))
        .map(|found| found.as_str().to_owned());
    own.chain(quoted).collect()
}

fn same_sentence(a: &str, b: &str) -> bool {
    let (a, b) = (normalize(a), normalize(b));
    !a.is_empty() && !b.is_empty() && (a == b || a.contains(&b) || b.contains(&a))
}

/// A specific stands only in a sentence the writer lists in the part's
/// citations, with a quote that contains it. Otherwise it is cited, when a
/// supplied excerpt states it, or the part is written in its general form,
/// when none does (claim classes design §3.4, §3.5). Code is exempt.
fn uncited_specifics(
    item: &Value,
    field: &str,
    (site, label): (FindingSite, &str),
    findings: &mut Vec<ReviewFinding>,
) {
    let path = match site {
        FindingSite::Section(index) => format!("/blocks/{index}"),
        FindingSite::Question(index) => format!("/questions/{index}"),
        FindingSite::Lesson => return,
    };
    let cited: Vec<(&str, &str)> = item
        .get("citations")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(|entry| (text(entry, "sentence"), text(entry, "quote")))
        .collect();
    let givens = Givens::of(item);
    for sentence in prose_sentences(text(item, field)) {
        if !needs_citation(&sentence, &givens) {
            continue;
        }
        let shown: String = sentence.chars().take(90).collect();
        let quotes: Vec<&str> = cited
            .iter()
            .filter(|(cited, _)| same_sentence(cited, &sentence))
            .map(|(_, quote)| *quote)
            .collect();
        let wanted = specifics(&sentence, &givens);
        let lacking = |quote: &str| -> Vec<String> {
            let held = normalize(quote);
            wanted
                .iter()
                .filter(|specific| !held.contains(&normalize(specific)))
                .cloned()
                .collect()
        };
        let (summary, note, quote) = if quotes.is_empty() {
            let summary =
                format!("No source is cited for this sentence: \u{201c}{sentence}\u{201d}");
            let note = format!("{label} states a specific in \"{shown}\" without citing it. Cite it when a supplied excerpt states it: add an entry to the citations of {path} with the sentence, the excerpt's sourceIndex and an exact quote of the excerpt that contains the figure or wording. Write the general form only when no supplied excerpt does.");
            (summary, note, "")
        } else {
            let (missing, quote) = quotes
                .iter()
                .map(|quote| (lacking(quote), *quote))
                .min_by_key(|(missing, _)| missing.len())
                .unwrap_or_default();
            if missing.is_empty() {
                continue;
            }
            let summary = format!(
                "The quotation cited for this sentence does not contain {}: \u{201c}{sentence}\u{201d}",
                join_plain(&missing)
            );
            let note = format!("{label}: the quote cited for \"{shown}\" does not contain {}. Cite a quote of a supplied excerpt that does, in an entry of the citations of {path}, or write the general form when no excerpt states it.", missing.join(", "));
            (summary, note, quote)
        };
        findings.push(citation(site, summary, note, (quote, sentence.as_str())));
    }
}

/// Whether `line` is a citation label: "Citation:" or "Citations:", with or
/// without a list marker or bold.
fn is_citation_label(line: &str) -> bool {
    let line = without_marker(line.trim()).trim_start_matches(['*', '_', '>', ' ']);
    let word: String = line.chars().take_while(|c| c.is_alphabetic()).collect();
    let after = line
        .get(word.len()..)
        .unwrap_or_default()
        .trim_start_matches(['*', '_']);
    (word.eq_ignore_ascii_case("citation") || word.eq_ignore_ascii_case("citations"))
        && after.starts_with(':')
}

/// Whether `sentence` names a choice by its number, letter or position, as
/// "Option 2", "Option B" or "the first choice".
fn names_a_choice(sentence: &str) -> bool {
    lazy_regex::regex!(r"(?i)\b(?:option|choice)s?\s+#?\d+\b").is_match(sentence)
        || lazy_regex::regex!(r"(?i:\b(?:option|choice|answer)s?)\s+\(?[A-H]\b\)?")
            .is_match(sentence)
        || lazy_regex::regex!(
            r"(?i)\b(?:first|second|third|fourth|fifth|last|final)\s+(?:choice|option|answer)s?\b"
        )
        .is_match(sentence)
}

/// Writer scaffolding in the text a learner reads: a sentence naming a source
/// by its number, a citation label line and, in an answer explanation, a
/// choice named by its number, letter or position. Code is exempt.
fn scaffolding(
    item: &Value,
    fields: &[&str],
    (site, label): (FindingSite, &str),
    findings: &mut Vec<ReviewFinding>,
) {
    let path = match site {
        FindingSite::Section(index) => format!("/blocks/{index}"),
        FindingSite::Question(index) => format!("/questions/{index}"),
        FindingSite::Lesson => return,
    };
    let mut texts: Vec<(&str, bool)> = Vec::new();
    for field in fields {
        let explanation = *field == "explanation";
        match item.get(*field) {
            Some(Value::String(value)) => texts.push((value, explanation)),
            Some(Value::Array(values)) => {
                texts.extend(
                    values
                        .iter()
                        .filter_map(Value::as_str)
                        .map(|v| (v, explanation)),
                );
            }
            _ => {}
        }
    }
    for (body, explanation) in texts {
        let labels: Vec<String> = prose(body)
            .lines()
            .filter(|line| is_citation_label(line))
            .map(|line| line.trim().to_owned())
            .collect();
        for line in &labels {
            let summary = format!(
                "This line is a citation written into the lesson's text: \u{201c}{line}\u{201d}"
            );
            let note = format!("{label} has a citation written into its text: \"{line}\". Remove that line from the text and cite the sentence it supports in an entry of the citations of {path}.");
            findings.push(citation(site, summary, note, ("", line.as_str())));
        }
        for sentence in prose_sentences(body) {
            if labels.iter().any(|line| line.contains(&sentence)) {
                continue;
            }
            let (summary, note) = if names_a_source(&sentence) {
                let summary = format!(
                    "This sentence names a source by its number, which learners never see: \u{201c}{sentence}\u{201d}"
                );
                let note = format!("{label} names a source by its number in \"{sentence}\". Learners never see source numbers: state what the source says as the lesson's own sentence without naming a source, and cite that sentence in the citations list of {path}.");
                (summary, note)
            } else if explanation && names_a_choice(&sentence) {
                let summary = format!(
                    "This explanation names a choice by its number, letter or position, and the choices are shuffled before learners see them: \u{201c}{sentence}\u{201d}"
                );
                let note = format!("{label} names a choice by its number, letter or position in \"{sentence}\". The choices are shuffled before learners see them: name the choice by its words instead.");
                (summary, note)
            } else {
                continue;
            };
            findings.push(citation(site, summary, note, ("", sentence.as_str())));
        }
    }
}

/// What `lesson_rules` reports, as its notes: for evals outside the crate,
/// which assert a reviewed lesson keeps the lesson's form.
pub fn lesson_rule_notes(candidate: &Value) -> Vec<String> {
    lesson_rules(candidate)
        .into_iter()
        .map(|finding| finding.note)
        .collect()
}

pub(crate) fn lesson_rules(candidate: &Value) -> Vec<ReviewFinding> {
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
        let plain = format!("Section {}", index + 1);
        let title = text(block, "title");
        if !validate_text(title) {
            findings.push(structure(
                FindingSite::Section(index),
                format!("{plain} needs a title."),
                format!("{section} needs a title."),
            ));
        } else if let Some(first) = titles.get(&normalize(title)) {
            findings.push(structure(
                FindingSite::Section(index),
                format!("{plain} has the same title as section {}.", first + 1),
                format!("{section} repeats the title of section {}; give each section a distinct title.", first + 1),
            ));
        } else {
            titles.insert(normalize(title), index);
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
        let plain: Vec<_> = counts
            .iter()
            .map(|(kind, count, _, _)| counted(*count, &kind.replace('_', " ")))
            .collect();
        findings.push(structure(
            FindingSite::Lesson,
            format!(
                "The lesson should have at least two explanations, two worked examples, one contrast and two checks, and one each of guided practice, independent practice, reflection and recap. It has {}.",
                plain.join(", ")
            ),
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
        let plain = format!("Section {}", index + 1);
        let held_back = split_check(text(block, "body"))
            .is_some_and(|(case, reveal)| !case.trim().is_empty() && !reveal.trim().is_empty());
        if !held_back {
            findings.push(structure(
                FindingSite::Section(index),
                format!("{plain} is a check, but it does not hold its answer back until the learner has tried."),
                format!("{section} is a check: write the case to reason through first, then one line `{CHECK_REVEAL}`, then the answer, the reasoning and the misconception each plausible wrong answer reveals. Nothing above that line may give the answer away."),
            ));
        }
        if !example_since_check {
            findings.push(structure(
                FindingSite::Section(index),
                format!("{plain} is a check on an idea that no worked example has shown since the previous check."),
                format!("{section} checks an idea no worked example has shown since the previous check; place each check after the example of the idea it asks the learner to apply."),
            ));
        }
        example_since_check = false;
    }
    if let (Some(reflection), Some(recap)) = (first("reflection"), first("recap")) {
        if reflection > recap {
            findings.push(structure(
                FindingSite::Section(recap),
                format!("Section {} recaps the lesson before the learner is asked to explain it from memory.", recap + 1),
                format!("Section {} (candidate blocks-{recap}) recaps before the reflection asks the learner to explain the lesson from memory; put the reflection first, so the recap is what they compare their explanation with.", recap + 1),
            ));
        }
    }
    let mut prompts = HashMap::new();
    for (index, question) in questions.iter().enumerate() {
        let label = format!("Question {}", index + 1);
        let site = FindingSite::Question(index);
        let prompt = text(question, "prompt");
        if !validate_text(prompt) {
            findings.push(structure(
                site,
                format!("{label} needs a prompt."),
                format!("{label} needs a prompt."),
            ));
        } else if let Some(first) = prompts.get(&normalize(prompt)) {
            findings.push(structure(
                site,
                format!("{label} asks the same thing as question {}.", first + 1),
                format!(
                    "{label} repeats question {}; ask something distinct.",
                    first + 1
                ),
            ));
        } else {
            prompts.insert(normalize(prompt), index);
        }
        if !validate_text(text(question, "explanation")) {
            findings.push(structure(
                site,
                format!("{label} needs an answer explanation."),
                format!("{label} needs an answer explanation."),
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
            || options
                .iter()
                .any(|option| !validate_text(option) || !choices.insert(normalize(option)))
        {
            findings.push(structure(
                site,
                format!("{label} needs four distinct answer choices."),
                format!("{label} needs four distinct answer choices."),
            ));
        }
    }
    for (index, block) in blocks.iter().enumerate() {
        let label = format!("Section {} (candidate blocks-{index})", index + 1);
        scaffolding(
            block,
            &["body"],
            (FindingSite::Section(index), &label),
            &mut findings,
        );
        if CITED_KINDS.contains(&text(block, "kind")) {
            uncited_specifics(
                block,
                "body",
                (FindingSite::Section(index), &label),
                &mut findings,
            );
        }
    }
    for (index, question) in questions.iter().enumerate() {
        let label = format!("Question {}", index + 1);
        scaffolding(
            question,
            &["prompt", "options", "explanation"],
            (FindingSite::Question(index), &label),
            &mut findings,
        );
        uncited_specifics(
            question,
            "explanation",
            (FindingSite::Question(index), &label),
            &mut findings,
        );
    }
    let per_kind: Vec<_> = crate::dto::LessonQuestionKind::ALL
        .iter()
        .map(|kind| (kind.as_str(), kinds(questions, kind.as_str())))
        .collect();
    if per_kind
        .iter()
        .any(|(_, count)| *count < QUESTIONS_PER_KIND)
    {
        let has: Vec<_> = per_kind
            .iter()
            .map(|(kind, count)| format!("{count} {kind}"))
            .collect();
        let plain: Vec<_> = per_kind
            .iter()
            .map(|(kind, count)| counted(*count, &format!("{kind} question")))
            .collect();
        findings.push(structure(
            FindingSite::Lesson,
            format!(
                "The lesson needs at least two questions each for practice, quiz and test. It has {}.",
                plain.join(", ")
            ),
            format!("The lesson needs at least two distinct questions each for practice, quiz and test; it has {}.", has.join(", ")),
        ));
    }
    findings
}

#[cfg(test)]
#[path = "lesson_rules_tests.rs"]
mod tests;
