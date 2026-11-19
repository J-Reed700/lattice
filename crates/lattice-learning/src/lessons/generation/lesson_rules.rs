//! The lesson's own rules beyond its schema: distinct, titled sections,
//! checks that hold their answer back, distinct, answerable questions, and a
//! specific that stands only in a sentence the writer cited. The lesson's
//! shape (its counts and the order of its sections) is the shape policy's. A
//! broken rule is a finding: the teaching repair is given it, and what the
//! repairs cannot settle waits for the user. It never stops the lesson from
//! publishing.
use super::validate_text;
use crate::assessment::assessment_engine::checks::{
    answer_in_prompt, answer_in_prompt_message, choice_faults, ChoiceFault, CHOICES_PER_QUESTION,
};
use crate::content_verification::{names_a_source, quoted_wording};
use crate::dto::LearningBlockKind;
use crate::lessons::repeats;
use crate::lessons::shape;
use crate::review_findings::{join_plain, FindingKind, FindingSite, ReviewFinding, Rule};
use lattice_core::shared::text::same_text::{fold, overlap};
use lattice_core::shared::text::segmentation::code_fences;
use lattice_core::shared::text::sentences;
use serde_json::Value;
use std::collections::{HashMap, HashSet};

/// The heading a check section's reveal starts under (learning science
/// lessons §3.3): the learner commits to an answer to what is above it
/// before what is below it is shown. `checkReveal.ts` splits on the same line.
pub(crate) const CHECK_REVEAL: &str = "### Check your thinking";

/// A check's case and its reveal, split at its `CHECK_REVEAL` line: `None`
/// unless exactly one line is that heading (at level two to four) and both
/// halves hold text. Claim extraction uses it to know a reveal's
/// misconceptions are named as wrong, and the lesson serves the halves.
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
    let (case, reveal) = (body.get(..start)?, body.get(end..)?);
    (!case.trim().is_empty() && !reveal.trim().is_empty()).then_some((case, reveal))
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
    let fences = code_fences(text);
    let mut prose = String::new();
    let mut offset = 0;
    for line in text.split_inclusive('\n') {
        if !fences.iter().any(|fence| fence.whole().contains(&offset)) {
            prose.push_str(line);
        }
        offset += line.len();
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
    site: FindingSite,
    findings: &mut Vec<ReviewFinding>,
) {
    let Some(path) = site.pointer() else {
        return;
    };
    let label = site.note_label();
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
    site: FindingSite,
    findings: &mut Vec<ReviewFinding>,
) {
    let Some(path) = site.pointer() else {
        return;
    };
    let label = site.note_label();
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
        let site = FindingSite::Section(index);
        let (section, plain) = (site.note_label(), site.label());
        let title = text(block, "title");
        if !validate_text(title) {
            findings.push(structure(
                Rule::TitleMissing,
                site,
                format!("{plain} needs a title."),
                format!("{section} needs a title."),
            ));
        } else if let Some(first) = titles.get(&fold(title)) {
            let first = FindingSite::Section(*first).plain();
            findings.push(structure(
                Rule::TitleRepeated,
                site,
                format!("{plain} has the same title as {first}."),
                format!(
                    "{section} repeats the title of {first}; give each section a distinct title."
                ),
            ));
        } else {
            titles.insert(fold(title), index);
        }
    }
    for (index, block) in blocks.iter().enumerate() {
        if code_fences(text(block, "body"))
            .iter()
            .any(|fence| fence.closing.is_none())
        {
            let site = FindingSite::Section(index);
            findings.push(structure(
                Rule::FenceUnclosed,
                site,
                format!("{} opens a code fence it never closes.", site.label()),
                format!("{} opens a code fence it never closes, so everything after its opening marks is shown as code. Close the fence with a line of the same marks after the example's last line.", site.note_label()),
            ));
        }
    }
    let mut seen = HashSet::new();
    for (index, block) in blocks.iter().enumerate() {
        let mut reported = HashSet::new();
        for sentence in repeats::comparable(text(block, "body")) {
            let key = repeats::key(&sentence);
            if !seen.insert(key.clone()) && reported.insert(key) {
                let site = FindingSite::Section(index);
                findings.push(
                    ReviewFinding::rule_about(
                        Rule::SentenceRepeated,
                        FindingKind::Teaching,
                        site,
                        &sentence,
                        format!("{} says \"{sentence}\" when the lesson has said it already; say it once, in the section it belongs to, and take it out or reword it here.", site.note_label()),
                    )
                    .with_summary(format!("{} repeats a sentence the lesson already has: \u{201c}{sentence}\u{201d}", site.label()))
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
        let site = FindingSite::Section(index);
        let (section, plain) = (site.note_label(), site.label());
        if split_check(text(block, "body")).is_none() {
            findings.push(structure(
                Rule::CheckHoldback,
                site,
                format!("{plain} is a check, but it does not hold its answer back until the learner has tried."),
                format!("{section} is a check: write the case to reason through first, then one line `{CHECK_REVEAL}`, then the answer, the reasoning and the misconception each plausible wrong answer reveals. Nothing above that line may give the answer away."),
            ));
        }
    }
    let mut prompts = HashMap::new();
    for (index, question) in questions.iter().enumerate() {
        let site = FindingSite::Question(index);
        let (label, note_label) = (site.label(), site.note_label());
        let prompt = text(question, "prompt");
        if !validate_text(prompt) {
            findings.push(structure(
                Rule::PromptMissing,
                site,
                format!("{label} needs a prompt."),
                format!("{note_label} needs a prompt."),
            ));
        } else if let Some(first) = prompts.get(&fold(prompt)) {
            let first = FindingSite::Question(*first).plain();
            findings.push(structure(
                Rule::PromptRepeated,
                site,
                format!("{label} asks the same thing as {first}."),
                format!("{note_label} repeats {first}; ask something distinct."),
            ));
        } else {
            prompts.insert(fold(prompt), index);
        }
        if !validate_text(text(question, "explanation")) {
            findings.push(structure(
                Rule::ExplanationMissing,
                site,
                format!("{label} needs an answer explanation."),
                format!("{note_label} needs an answer explanation."),
            ));
        }
        let options: Vec<String> = question
            .get("options")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .map(|option| option.as_str().unwrap_or_default().to_owned())
            .collect();
        let right = question
            .get("correctIndex")
            .and_then(Value::as_u64)
            .and_then(|right| usize::try_from(right).ok());
        findings.extend(choice_findings(question, (&options, right), site));
        let reasons = reasons(question);
        if reasons.len() != options.len() {
            findings.push(structure(
                Rule::RationaleCount,
                site,
                format!("{label} needs a reason for each of its choices."),
                format!("{note_label} needs one entry in rationales for each answer choice, in the order of the choices: empty for the right choice, and for each wrong choice the misconception it reflects and why it fails."),
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
                    format!("{note_label} gives no reason in rationales for {}: for each wrong choice, name the misconception it reflects and why it fails.", join_plain(&lacking)),
                ));
            }
        }
    }
    for (index, block) in blocks.iter().enumerate() {
        let site = FindingSite::Section(index);
        scaffolding(block, &["body"], site, &mut findings);
        if LearningBlockKind::parse(text(block, "kind")).is_some_and(|kind| kind.cites()) {
            uncited_quotations(block, text(block, "body"), site, &mut findings);
        }
    }
    for (index, question) in questions.iter().enumerate() {
        let site = FindingSite::Question(index);
        scaffolding(
            question,
            &["prompt", "options", "explanation"],
            site,
            &mut findings,
        );
        uncited_quotations(question, text(question, "explanation"), site, &mut findings);
        for reason in reasons(question) {
            uncited_quotations(question, reason, site, &mut findings);
        }
    }
    findings
}

/// The findings about a question's answer choices: the first of the count, a
/// blank choice, a repeated choice and a missing key (one finding, as they are
/// mended together), a right choice much longer than the others, and a prompt
/// that contains the right choice.
fn choice_findings(
    question: &Value,
    (options, right): (&[String], Option<usize>),
    site: FindingSite,
) -> Vec<ReviewFinding> {
    let (label, note_label) = (site.label(), site.note_label());
    let faults = choice_faults(options, right, CHOICES_PER_QUESTION..=CHOICES_PER_QUESTION);
    let mut found = Vec::new();
    if let Some(fault) = faults
        .iter()
        .find(|fault| **fault != ChoiceFault::LengthBiased)
    {
        let message = fault.message();
        found.push(structure(
            Rule::Choices,
            site,
            format!("{label}: {message}"),
            format!("{note_label}: {message}"),
        ));
    }
    if faults.contains(&ChoiceFault::LengthBiased) {
        let message = ChoiceFault::LengthBiased.message();
        found.push(structure(
            Rule::KeyLongest,
            site,
            format!("{label}: {message}"),
            format!("{note_label}: {message}"),
        ));
    }
    let key = right.and_then(|right| options.get(right));
    let others: Vec<&str> = options
        .iter()
        .enumerate()
        .filter(|(at, _)| Some(*at) != right)
        .map(|(_, option)| option.as_str())
        .collect();
    let words = key.and_then(|key| answer_in_prompt(text(question, "prompt"), key, &others));
    if let Some(words) = words {
        let message = answer_in_prompt_message(&words);
        found.push(structure(
            Rule::AnswerInPrompt,
            site,
            format!("{label}: {message}"),
            format!(
                "{note_label}: {message} Reword the prompt so it does not state the right choice."
            ),
        ));
    }
    found
}

#[cfg(test)]
#[path = "lesson_rules_tests.rs"]
mod tests;
