//! The lesson's own rules beyond its schema: distinct, titled sections,
//! checks that hold their answer back, distinct, answerable questions, and a
//! specific that stands only in a sentence the writer cited. The lesson's
//! shape (its counts and the order of its sections) is the shape policy's. A
//! broken rule is a finding: the teaching repair is given it, and what the
//! repairs cannot settle waits for the user. It never stops the lesson from
//! publishing.
use super::validate_text;
use crate::assessment_engine::{key_fault, longest_allowed_key, repeated_option, KeyFault};
use crate::content_verification::{names_a_source, quoted_wording, CITED_KINDS};
use crate::lessons::repeats;
use crate::lessons::same_text::{fold, overlap};
use crate::lessons::shape;
use crate::review_findings::{join_plain, FindingKind, FindingSite, ReviewFinding, Rule};
use lattice_core::shared::text::sentences;
use serde_json::Value;
use std::collections::{HashMap, HashSet};

/// The heading a check section's reveal starts under (learning science
/// lessons §3.3): the learner commits to an answer to what is above it
/// before what is below it is shown. `checkReveal.ts` splits on the same line.
pub(crate) const CHECK_REVEAL: &str = "### Check your thinking";

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

fn text<'a>(item: &'a Value, field: &str) -> &'a str {
    item.get(field).and_then(Value::as_str).unwrap_or_default()
}

/// A question's reason for each of its choices, in the choices' order.
fn reasons(question: &Value) -> Vec<&str> {
    question
        .get("rationales")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(|reason| reason.as_str().unwrap_or_default())
        .collect()
}

/// A broken rule, with what is wrong in plain words for the user and the
/// note the repair is given.
fn structure(rule: Rule, site: FindingSite, summary: String, note: String) -> ReviewFinding {
    ReviewFinding::rule(rule, FindingKind::Structure, site, note).with_summary(summary)
}

/// A citation a part lacks. Not structure: it does not hold up the content
/// review, and the repair that follows settles it by appending the citation.
fn citation(
    rule: Rule,
    site: FindingSite,
    summary: String,
    note: String,
    (quote, sentence): (&str, &str),
) -> ReviewFinding {
    ReviewFinding::rule_about(rule, FindingKind::Quotation, site, sentence, note)
        .with_summary(summary)
        .with_passage(quote)
        .with_sentence(sentence)
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

/// Quoted wording stands only in a sentence the writer lists in the part's
/// citations, with a quote that contains it. Otherwise it is cited, when a
/// supplied excerpt holds it, or written as the lesson's own sentence without
/// quotation marks, when none does (claim classes design §3.4, §3.13). A
/// figure is not read here: whether it is the lesson's own or a fact a source
/// must state is the classifier's question. Code is exempt.
fn uncited_quotations(
    item: &Value,
    body: &str,
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
    for sentence in prose_sentences(body) {
        let wanted: Vec<&str> = quoted_wording(&sentence).collect();
        if wanted.is_empty() {
            continue;
        }
        let quotes: Vec<&str> = cited
            .iter()
            .filter(|(cited, _)| overlap(cited, &sentence))
            .map(|(_, quote)| *quote)
            .collect();
        let lacking = |quote: &str| -> Vec<String> {
            let held = fold(quote);
            wanted
                .iter()
                .filter(|wording| !held.contains(&fold(wording)))
                .map(|wording| (*wording).to_owned())
                .collect()
        };
        let (summary, note, quote) = if quotes.is_empty() {
            let summary =
                format!("No source is cited for this sentence: \u{201c}{sentence}\u{201d}");
            let note = format!("{label} states a specific in \"{sentence}\" without citing it. Cite it when a supplied excerpt holds the wording: add an entry to the citations of {path} with the sentence, the excerpt's sourceIndex and an exact quote of the excerpt that contains the wording. When no supplied excerpt holds it, or the words are not quoted from a source (a misconception, a learner's thought or an example remark), remove the quotation marks and write it as the lesson's own sentence.");
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
            let note = format!("{label}: the quote cited for \"{sentence}\" does not contain {}. Cite a quote of a supplied excerpt that does, in an entry of the citations of {path}, or remove the quotation marks and write it as the lesson's own sentence when no excerpt holds the wording.", missing.join(", "));
            (summary, note, quote)
        };
        findings.push(citation(
            Rule::Uncited,
            site,
            summary,
            note,
            (quote, sentence.as_str()),
        ));
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

/// Writer scaffolding in the text a learner reads: a sentence naming a source
/// by its number and a citation label line. Code is exempt.
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
    let mut texts: Vec<&str> = Vec::new();
    for field in fields {
        match item.get(*field) {
            Some(Value::String(value)) => texts.push(value),
            Some(Value::Array(values)) => texts.extend(values.iter().filter_map(Value::as_str)),
            _ => {}
        }
    }
    for body in texts {
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
            findings.push(citation(
                Rule::CitationLabel,
                site,
                summary,
                note,
                ("", line.as_str()),
            ));
        }
        for sentence in prose_sentences(body) {
            if labels.iter().any(|line| line.contains(&sentence)) {
                continue;
            }
            if !names_a_source(&sentence) {
                continue;
            }
            let summary = format!(
                "This sentence names a source by its number, which learners never see: \u{201c}{sentence}\u{201d}"
            );
            let note = format!("{label} names a source by its number in \"{sentence}\". Learners never see source numbers: state what the source says as the lesson's own sentence without naming a source, and cite that sentence in the citations list of {path}.");
            findings.push(citation(
                Rule::SourceNumber,
                site,
                summary,
                note,
                ("", sentence.as_str()),
            ));
        }
    }
}

/// What `lesson_rules` reports, as its notes: for evals outside the crate,
/// which assert a reviewed lesson keeps the lesson's form.
pub fn lesson_rule_notes(candidate: &Value) -> Vec<String> {
    shape::policy()
        .rules(candidate, shape::Phase::Draft)
        .into_iter()
        .map(|finding| finding.note)
        .collect()
}

/// The lesson's findings: the rules of its parts, with `shape_findings`, the shape
/// policy's about its counts and order, among them.
pub(crate) fn lesson_rules(
    candidate: &Value,
    shape_findings: Vec<ReviewFinding>,
) -> Vec<ReviewFinding> {
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
                Rule::TitleMissing,
                FindingSite::Section(index),
                format!("{plain} needs a title."),
                format!("{section} needs a title."),
            ));
        } else if let Some(first) = titles.get(&fold(title)) {
            findings.push(structure(
                Rule::TitleRepeated,
                FindingSite::Section(index),
                format!("{plain} has the same title as section {}.", first + 1),
                format!("{section} repeats the title of section {}; give each section a distinct title.", first + 1),
            ));
        } else {
            titles.insert(fold(title), index);
        }
    }
    let mut seen = HashSet::new();
    for (index, block) in blocks.iter().enumerate() {
        let mut reported = HashSet::new();
        for sentence in repeats::comparable(text(block, "body")) {
            let key = repeats::key(&sentence);
            if !seen.insert(key.clone()) && reported.insert(key) {
                findings.push(
                    ReviewFinding::rule_about(
                        Rule::SentenceRepeated,
                        FindingKind::Teaching,
                        FindingSite::Section(index),
                        &sentence,
                        format!("Section {} (candidate blocks-{index}) says \"{sentence}\" when the lesson has said it already; say it once, in the section it belongs to, and take it out or reword it here.", index + 1),
                    )
                    .with_summary(format!("Section {} repeats a sentence the lesson already has: \u{201c}{sentence}\u{201d}", index + 1))
                    .with_passage(&sentence),
                );
            }
        }
    }
    findings.extend(shape_findings);
    for (index, block) in blocks.iter().enumerate() {
        if text(block, "kind") != "check" {
            continue;
        }
        let section = format!("Section {} (candidate blocks-{index})", index + 1);
        let plain = format!("Section {}", index + 1);
        let held_back = split_check(text(block, "body"))
            .is_some_and(|(case, reveal)| !case.trim().is_empty() && !reveal.trim().is_empty());
        if !held_back {
            findings.push(structure(
                Rule::CheckHoldback,
                FindingSite::Section(index),
                format!("{plain} is a check, but it does not hold its answer back until the learner has tried."),
                format!("{section} is a check: write the case to reason through first, then one line `{CHECK_REVEAL}`, then the answer, the reasoning and the misconception each plausible wrong answer reveals. Nothing above that line may give the answer away."),
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
                Rule::PromptMissing,
                site,
                format!("{label} needs a prompt."),
                format!("{label} needs a prompt."),
            ));
        } else if let Some(first) = prompts.get(&fold(prompt)) {
            findings.push(structure(
                Rule::PromptRepeated,
                site,
                format!("{label} asks the same thing as question {}.", first + 1),
                format!(
                    "{label} repeats question {}; ask something distinct.",
                    first + 1
                ),
            ));
        } else {
            prompts.insert(fold(prompt), index);
        }
        if !validate_text(text(question, "explanation")) {
            findings.push(structure(
                Rule::ExplanationMissing,
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
        let choice_texts: Vec<String> = options.iter().map(|o| (*o).to_owned()).collect();
        let right = question
            .get("correctIndex")
            .and_then(Value::as_u64)
            .and_then(|right| usize::try_from(right).ok());
        let choices_fault = if options.len() != 4 {
            Some(format!(
                "{label} needs four answer choices; it has {}.",
                options.len()
            ))
        } else if options.iter().any(|option| !validate_text(option)) {
            Some(format!("{label} has a blank answer choice."))
        } else {
            repeated_option(&choice_texts).map(|choice| {
                format!(
                    "{label} repeats the answer choice \"{}\"; each choice must differ.",
                    choice.trim()
                )
            })
        };
        if let Some(note) = choices_fault {
            findings.push(structure(Rule::Choices, site, note.clone(), note));
        } else if let Some(fault) = right.and_then(|right| key_fault(prompt, &choice_texts, right))
        {
            findings.push(match fault {
                KeyFault::LengthBiased => {
                    let length = |at: usize| {
                        choice_texts
                            .get(at)
                            .map_or(0, |choice| choice.trim().chars().count())
                    };
                    let key = right.map_or(0, length);
                    let allowed = right
                        .and_then(|right| longest_allowed_key(&choice_texts, right))
                        .unwrap_or(0);
                    let other = (0..choice_texts.len())
                        .filter(|at| Some(*at) != right)
                        .map(length)
                        .max()
                        .unwrap_or(0);
                    structure(
                        Rule::KeyLongest,
                        site,
                        format!("{label}'s right choice stands out as the longest."),
                        format!("{label}'s right choice is {key} characters and the longest wrong choice {other}; so that a learner cannot pick the right choice for its length, it may be at most {allowed} characters. Shorten the right choice to {allowed} characters or fewer, or make the wrong choices as specific."),
                    )
                }
                KeyFault::GivesAway => structure(
                    Rule::KeyGivenAway,
                    site,
                    format!("{label}'s prompt gives away its answer."),
                    format!("{label}'s right choice repeats the prompt's words, so the prompt gives the answer away; reword the prompt or the right choice."),
                ),
            });
        }
        let reasons = reasons(question);
        if reasons.len() != options.len() {
            findings.push(structure(
                Rule::RationaleCount,
                site,
                format!("{label} needs a reason for each of its choices."),
                format!("{label} needs one entry in rationales for each answer choice, in the order of the choices: empty for the right choice, and for each wrong choice the misconception it reflects and why it fails."),
            ));
        } else {
            let lacking: Vec<String> = reasons
                .iter()
                .enumerate()
                .filter(|(at, reason)| Some(*at) != right && !validate_text(reason))
                .map(|(at, _)| format!("choice {}", at + 1))
                .collect();
            if !lacking.is_empty() {
                findings.push(structure(
                    Rule::RationaleMissing,
                    site,
                    format!("{label} needs a reason for each wrong choice: {}.", join_plain(&lacking)),
                    format!("{label} gives no reason in rationales for {}: for each wrong choice, name the misconception it reflects and why it fails.", join_plain(&lacking)),
                ));
            }
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
            uncited_quotations(
                block,
                text(block, "body"),
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
        uncited_quotations(
            question,
            text(question, "explanation"),
            (FindingSite::Question(index), &label),
            &mut findings,
        );
        for reason in reasons(question) {
            uncited_quotations(
                question,
                reason,
                (FindingSite::Question(index), &label),
                &mut findings,
            );
        }
    }
    findings
}

#[cfg(test)]
#[path = "lesson_rules_tests.rs"]
mod tests;
