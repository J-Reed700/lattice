//! What a bank item must satisfy before any form may use it (section 5.2).
use super::answers::{quoted, whole_words};
use super::*;
use std::ops::RangeInclusive;
use unicode_normalization::UnicodeNormalization;

/// The key's words when a learner can copy them out of a choice question's
/// prompt: its whole text is in the prompt and no other option's is, since a
/// prompt naming several options is a comparison.
pub fn answer_in_prompt(prompt: &str, key: &str, others: &[&str]) -> Option<String> {
    let stem = normalize_answer(prompt);
    if others
        .iter()
        .any(|other| whole_words(&stem, &normalize_answer(other)))
    {
        return None;
    }
    whole_words(&stem, &normalize_answer(key)).then(|| key.trim().to_owned())
}

/// The sentence saying the prompt holds its answer, `words` being what `answer_in_prompt` found.
pub fn answer_in_prompt_message(words: &str) -> String {
    format!("The question contains its answer: {}.", quoted(words))
}

/// The share of the median option length the key may exceed the longest other option by.
const LENGTH_SLACK: f64 = 0.6;

/// The most the key may exceed the longest other option by: `LENGTH_SLACK` of
/// the median option length, whole characters.
fn length_slack(lengths: &[usize]) -> usize {
    let mut sorted = lengths.to_vec();
    sorted.sort_unstable();
    let median = sorted.get(sorted.len() / 2).copied().unwrap_or(0) as f64;
    (LENGTH_SLACK * median) as usize
}

/// The longest the key may be before it stands out: the longest other option
/// plus the slack. `None` when `key` is no option.
pub fn longest_allowed_key(options: &[String], key: usize) -> Option<usize> {
    let lengths: Vec<usize> = options.iter().map(|o| o.trim().chars().count()).collect();
    lengths.get(key)?;
    let longest_other = lengths
        .iter()
        .enumerate()
        .filter(|(index, _)| *index != key)
        .map(|(_, l)| *l)
        .max()
        .unwrap_or(0);
    Some(longest_other + length_slack(&lengths))
}

/// Length bias (section 5.2): the key is the unique longest option by more
/// than `LENGTH_SLACK` of the median option length.
pub fn length_biased(options: &[String], key: usize) -> bool {
    let Some(allowed) = longest_allowed_key(options, key) else {
        return false;
    };
    let key_length = options.get(key).map_or(0, |o| o.trim().chars().count());
    key_length > allowed
}

/// The `{{n}}` blanks a cloze prompt holds, in order of appearance.
pub fn cloze_blanks(prompt: &str) -> Vec<usize> {
    let mut found = Vec::new();
    let mut rest = prompt;
    while let Some(start) = rest.find("{{") {
        let after = &rest[start + 2..];
        let Some(end) = after.find("}}") else { break };
        if let Ok(n) = after[..end].trim().parse::<usize>() {
            found.push(n);
        }
        rest = &after[end + 2..];
    }
    found
}

fn comparable(option: &str) -> String {
    let composed: String = option
        .nfc()
        .map(|c| match c {
            '\u{2018}' | '\u{2019}' => '\'',
            '\u{201C}' | '\u{201D}' => '"',
            '\u{2010}'..='\u{2015}' | '\u{2212}' => '-',
            other => other,
        })
        .collect();
    let words = composed.split_whitespace().collect::<Vec<_>>().join(" ");
    words
        .strip_suffix('.')
        .unwrap_or(&words)
        .trim_end()
        .to_owned()
}

/// Two choices are the same only when a reader could not tell them apart:
/// typography, spacing and a closing full stop do not count; letter case,
/// accents and path punctuation do.
pub fn distinct_options(options: &[String]) -> bool {
    repeated_option(options).is_none() && options.iter().all(|o| !o.trim().is_empty())
}

/// The first choice that repeats an earlier one, as `distinct_options` reads them.
pub fn repeated_option(options: &[String]) -> Option<&str> {
    let mut seen = HashSet::new();
    options
        .iter()
        .find(|option| !seen.insert(comparable(option)))
        .map(String::as_str)
}

/// The answer choices every single-choice question is written with.
pub(crate) const CHOICES_PER_QUESTION: usize = 4;

/// How many options, and how many of them correct, a multi-select item has.
pub const MULTI_SELECT_OPTIONS: RangeInclusive<usize> = 4..=6;
pub const MULTI_SELECT_CORRECT: RangeInclusive<usize> = 2..=3;

/// How many pairs a matching item has.
pub const MATCHING_PAIRS: RangeInclusive<usize> = 3..=5;

/// A count range in words: "4" or "2–4".
pub fn span(range: &RangeInclusive<usize>) -> String {
    let (least, most) = (range.start(), range.end());
    if least == most {
        least.to_string()
    } else {
        format!("{least}–{most}")
    }
}

/// What is wrong with a single-choice question's choices, in words its writer is told.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChoiceFault {
    Count {
        has: usize,
        wanted: RangeInclusive<usize>,
    },
    Blank,
    Repeated(String),
    NoKey,
    LengthBiased,
}

impl ChoiceFault {
    pub fn message(&self) -> String {
        match self {
            Self::Count { has, wanted } => {
                format!("It has {has} answer choices; it needs {}.", span(wanted))
            }
            Self::Blank => "A choice is blank.".to_owned(),
            Self::Repeated(option) => format!("Two choices are the same: {}.", quoted(option)),
            Self::NoKey => "No choice is marked as the correct one.".to_owned(),
            Self::LengthBiased => "The correct option stands out as much the longest.".to_owned(),
        }
    }
}

/// Every fault of a single-choice question's choices and key. A key that
/// stands out is judged only among three or more choices.
pub fn choice_faults(
    options: &[String],
    key: Option<usize>,
    wanted: RangeInclusive<usize>,
) -> Vec<ChoiceFault> {
    let mut faults = Vec::new();
    if !wanted.contains(&options.len()) {
        faults.push(ChoiceFault::Count {
            has: options.len(),
            wanted,
        });
    }
    if options.iter().any(|option| option.trim().is_empty()) {
        faults.push(ChoiceFault::Blank);
    }
    if let Some(option) = repeated_option(options) {
        faults.push(ChoiceFault::Repeated(option.to_owned()));
    }
    match key.filter(|key| *key < options.len()) {
        None => faults.push(ChoiceFault::NoKey),
        Some(key) if options.len() >= 3 && length_biased(options, key) => {
            faults.push(ChoiceFault::LengthBiased);
        }
        Some(_) => {}
    }
    faults
}

/// The writer's statement of `length_biased`.
pub fn key_length_rule() -> String {
    format!(
        "The correct choice may be longer than the longest other choice by at most {} % of the median choice length.",
        (LENGTH_SLACK * 100.0).round()
    )
}

/// Every content fault of a bank item, each in words its writer is told: a
/// choice question's choices and key, its key being in its prompt, and its
/// key's length.
pub fn item_faults(candidate: &LearningAssessmentCandidate) -> Vec<String> {
    let prompt = candidate.prompt.as_str();
    match (&candidate.format, &candidate.key) {
        (LearningItemFormat::SingleChoice, LearningAssessmentKey::Choice(key)) => {
            let options = &candidate.options;
            let mut faults: Vec<String> = choice_faults(options, Some(*key), SINGLE_CHOICE_OPTIONS)
                .iter()
                .map(ChoiceFault::message)
                .collect();
            let others: Vec<&str> = options
                .iter()
                .enumerate()
                .filter(|(index, _)| index != key)
                .map(|(_, option)| option.as_str())
                .collect();
            faults.extend(
                options
                    .get(*key)
                    .and_then(|key| answer_in_prompt(prompt, key, &others))
                    .map(|words| answer_in_prompt_message(&words)),
            );
            faults
        }
        _ => Vec::new(),
    }
}

fn text_key_valid(key: &LearningTextKey) -> bool {
    !key.variants.is_empty()
        && key.variants.iter().all(|v| !v.trim().is_empty())
        && key
            .numeric
            .as_ref()
            .is_none_or(|n| n.value.is_finite() && n.tolerance.is_finite() && n.tolerance >= 0.0)
}

pub const TRUE_FALSE: [&str; 2] = ["True", "False"];

/// How many options a single-choice item may have.
pub const SINGLE_CHOICE_OPTIONS: RangeInclusive<usize> = 2..=8;

pub fn validate_candidate(candidate: &LearningAssessmentCandidate) -> Result<()> {
    id(&candidate.id, "assessment item")?;
    present(&candidate.prompt, "Assessment prompt")?;
    if candidate.outcome_ids.is_empty() {
        return Err(invalid("An assessment item must address an outcome."));
    }
    let mut outcomes = HashSet::new();
    for outcome in &candidate.outcome_ids {
        id(outcome, "outcome")?;
        if !outcomes.insert(outcome) {
            return Err(invalid("An assessment item repeats an outcome."));
        }
    }
    if !(1..=5).contains(&candidate.difficulty) {
        return Err(invalid(
            "Assessment item difficulty must use the 1–5 scale.",
        ));
    }
    for source in &candidate.source_version_ids {
        id(source, "source version")?;
    }
    if candidate.distractor_rationales.len() > candidate.options.len() {
        return Err(invalid(
            "An assessment item has more distractor rationales than options.",
        ));
    }
    let no_left = candidate.left.is_empty();
    match (&candidate.format, &candidate.key) {
        (LearningItemFormat::SingleChoice, LearningAssessmentKey::Choice(_)) => {
            if !no_left {
                return Err(invalid("Single-choice items take no left-hand entries."));
            }
        }
        (LearningItemFormat::TrueFalse, LearningAssessmentKey::Choice(key)) => {
            if candidate.options != TRUE_FALSE.map(String::from) || *key > 1 || !no_left {
                return Err(invalid(
                    "True/false items use the options True and False and one key.",
                ));
            }
        }
        (LearningItemFormat::MultiSelect, LearningAssessmentKey::Choices(keys)) => {
            if !MULTI_SELECT_OPTIONS.contains(&candidate.options.len())
                || !MULTI_SELECT_CORRECT.contains(&keys.len())
                || keys.iter().any(|k| *k >= candidate.options.len())
            {
                return Err(AppError::InvalidInput(format!(
                    "Multi-select items need {} options with {} correct.",
                    span(&MULTI_SELECT_OPTIONS),
                    span(&MULTI_SELECT_CORRECT)
                )));
            }
            if !distinct_options(&candidate.options) || !no_left {
                return Err(invalid("Choice options must be unique and non-empty."));
            }
        }
        (LearningItemFormat::ShortAnswer, LearningAssessmentKey::Text(key)) => {
            if !text_key_valid(key) || !candidate.options.is_empty() || !no_left {
                return Err(invalid(
                    "Short-answer items need accepted answers and no choices.",
                ));
            }
        }
        (LearningItemFormat::Cloze, LearningAssessmentKey::Cloze(blanks)) => {
            let found = cloze_blanks(&candidate.prompt);
            let expected: Vec<usize> = (1..=blanks.len()).collect();
            if blanks.is_empty()
                || found != expected
                || !blanks.iter().all(text_key_valid)
                || !candidate.options.is_empty()
                || !no_left
            {
                return Err(invalid(
                    "Cloze items need blanks {{1}}…{{n}}, each once and in order, with accepted answers for each.",
                ));
            }
        }
        (LearningItemFormat::Matching, LearningAssessmentKey::Matching(pairs)) => {
            let lefts = candidate.left.len();
            let rights = candidate.options.len();
            if !MATCHING_PAIRS.contains(&lefts)
                || !(rights == lefts || rights == lefts + 1)
                || pairs.len() != lefts
                || pairs.iter().any(|p| *p >= rights)
                || pairs.iter().collect::<HashSet<_>>().len() != pairs.len()
                || !distinct_options(&candidate.left)
                || !distinct_options(&candidate.options)
            {
                return Err(AppError::InvalidInput(format!(
                    "Matching items need {} distinct pairs, at most one extra right-hand entry, and no shared right-hand answers.",
                    span(&MATCHING_PAIRS)
                )));
            }
        }
        (LearningItemFormat::Ordering, LearningAssessmentKey::Ordering(order)) => {
            if !(3..=12).contains(&candidate.options.len())
                || order.len() != candidate.options.len()
                || !distinct_options(order)
                || candidate.options.iter().collect::<HashSet<_>>()
                    != order.iter().collect::<HashSet<_>>()
                || !no_left
            {
                return Err(invalid(
                    "Ordering items need one permutation of 3–12 distinct values.",
                ));
            }
        }
        (
            LearningItemFormat::Explanation | LearningItemFormat::Artifact,
            LearningAssessmentKey::Rubric,
        ) => {
            if candidate.rubric.is_empty() || !no_left {
                return Err(invalid("Written items need visible rubric criteria."));
            }
            let mut criteria = HashSet::new();
            for criterion in &candidate.rubric {
                id(&criterion.id, "rubric criterion")?;
                present(&criterion.title, "Rubric criterion title")?;
                present(&criterion.description, "Rubric criterion description")?;
                if criterion.max_points == 0 {
                    return Err(invalid("Rubric criterion points must be at least 1."));
                }
                if !criteria.insert(&criterion.id) {
                    return Err(invalid("Rubric criterion IDs must be unique."));
                }
            }
        }
        _ => {
            return Err(invalid(
                "Assessment item format and answer key do not match.",
            ))
        }
    }
    match item_faults(candidate).first() {
        Some(fault) => Err(invalid(fault)),
        None => Ok(()),
    }
}
