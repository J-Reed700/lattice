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
    if !candidate.distractor_rationales.is_empty() {
        let right_has_reason = match &candidate.key {
            LearningAssessmentKey::Choice(right) => candidate
                .distractor_rationales
                .get(*right)
                .is_some_and(|reason| !reason.trim().is_empty()),
            LearningAssessmentKey::Choices(rights) => rights.iter().any(|right| {
                candidate
                    .distractor_rationales
                    .get(*right)
                    .is_some_and(|reason| !reason.trim().is_empty())
            }),
            _ => false,
        };
        if right_has_reason {
            return Err(invalid(
                "An assessment item keeps the right choice's distractor rationale empty.",
            ));
        }
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

/// Checks freshly authored quiz items and lesson questions for course/lesson
/// meta-talk, phantom build/version numbers absent from the item and `quote`,
/// and ungrounded external company framing when `companies` is non-empty.
#[cfg(test)]
pub(crate) fn authored_content_faults(
    candidate: &LearningAssessmentCandidate,
    quote: &str,
) -> Vec<String> {
    authored_content_faults_for(candidate, quote, &[])
}

pub(crate) fn authored_content_faults_for(
    candidate: &LearningAssessmentCandidate,
    quote: &str,
    companies: &[String],
) -> Vec<String> {
    let mut faults = Vec::new();
    let texts = || {
        std::iter::once(candidate.prompt.as_str())
            .chain(candidate.options.iter().map(String::as_str))
            .chain(candidate.left.iter().map(String::as_str))
            .chain(std::iter::once(candidate.explanation.as_str()))
            .chain(candidate.distractor_rationales.iter().map(String::as_str))
            .chain(
                candidate
                    .rubric
                    .iter()
                    .flat_map(|c| [c.title.as_str(), c.description.as_str()]),
            )
    };
    if texts().any(contains_course_meta_talk) {
        faults.push(
            "The question or its feedback uses course or lesson meta-talk instead of domain terms."
                .into(),
        );
    }
    if let Some(phantom) = find_phantom_version(candidate, quote) {
        faults.push(format!(
            "The explanation or rationale introduces version {phantom}, which does not appear in the question or its answer."
        ));
    }
    if !companies.is_empty() {
        for text in texts() {
            for fault in crate::lessons::company_framing::company_framing_faults(text, companies) {
                if !faults.contains(&fault) {
                    faults.push(fault);
                }
            }
        }
    }
    let scope_sources = (!quote.trim().is_empty()).then_some(quote);
    let source_slice: &[&str] = match scope_sources {
        Some(q) => &[q],
        None => &[],
    };
    let right_indices: std::collections::HashSet<usize> = match &candidate.key {
        LearningAssessmentKey::Choice(idx) => std::iter::once(*idx).collect(),
        LearningAssessmentKey::Choices(idxs) => idxs.iter().copied().collect(),
        LearningAssessmentKey::Matching(pairs) => pairs.iter().copied().collect(),
        _ => std::collections::HashSet::new(),
    };
    let checked_for_scopes = std::iter::once(candidate.prompt.as_str())
        .chain(
            candidate
                .options
                .iter()
                .enumerate()
                .filter(|(idx, _)| right_indices.contains(idx))
                .map(|(_, opt)| opt.as_str()),
        )
        .chain(candidate.left.iter().map(String::as_str))
        .chain(std::iter::once(candidate.explanation.as_str()))
        .chain(candidate.distractor_rationales.iter().map(String::as_str))
        .chain(
            candidate
                .rubric
                .iter()
                .flat_map(|c| [c.title.as_str(), c.description.as_str()]),
        );
    for text in checked_for_scopes {
        for fault in crate::lessons::spec_checks::smart_scope_faults(text, source_slice) {
            if !faults.contains(&fault) {
                faults.push(fault);
            }
        }
    }
    let mentions_403_quote = crate::lessons::spec_checks::mentions_403_scope_error(quote);
    let claims_scope_401 = crate::lessons::spec_checks::asserts_401_for_scope_error(
        &candidate.explanation.to_ascii_lowercase(),
    ) || candidate
        .options
        .iter()
        .enumerate()
        .filter(|(idx, _)| right_indices.contains(idx))
        .any(|(_, opt)| {
            crate::lessons::spec_checks::asserts_401_for_scope_error(&opt.to_ascii_lowercase())
        });
    if claims_scope_401
        && (mentions_403_quote || (!quote.contains("401") && !candidate.prompt.contains("401")))
    {
        faults.push(if mentions_403_quote {
            "Claims HTTP `401` for a scope validation failure when the grounded lesson passage specifies `403 Forbidden`."
                .into()
        } else {
            "Claims HTTP `401` for a scope validation failure when neither the grounded lesson passage nor the question prompt specifies `401` (`403 Forbidden` applies to insufficient scopes)."
                .into()
        });
    }
    if !matches!(
        candidate.format,
        LearningItemFormat::SingleChoice
            | LearningItemFormat::MultiSelect
            | LearningItemFormat::Matching
    ) {
        let mut non_choice_texts = std::iter::once(candidate.prompt.as_str())
            .chain(std::iter::once(candidate.explanation.as_str()))
            .chain(
                candidate
                    .rubric
                    .iter()
                    .flat_map(|c| [c.title.as_str(), c.description.as_str()]),
            );
        if non_choice_texts.any(contains_non_choice_distractor_reference) {
            faults.push(
                "A non-choice question's prompt, explanation, or rubric refers to distractors or answer options."
                    .into(),
            );
        }
    }
    if std::iter::once(candidate.explanation.as_str())
        .chain(candidate.distractor_rationales.iter().map(String::as_str))
        .any(|text| {
            contains_option_letter_or_number_reference(text)
                || contains_ordinal_option_reference(text)
        })
    {
        faults.push(
            "Names an answer choice by letter, number, or position instead of stating what it says (options are shuffled)."
                .into(),
        );
    }
    faults
}

mod text;
pub(crate) use text::contains_option_letter_or_number_reference;
use text::{contains_non_choice_distractor_reference, contains_ordinal_option_reference};

fn contains_course_meta_talk(text: &str) -> bool {
    let normalized = text.replace('\u{2019}', "'").to_ascii_lowercase();
    const PHRASES: &[&str] = &[
        "of this course",
        "in this course",
        "for this course",
        "rules of this course",
        "rules of the course",
        "this lesson's",
        "in this lesson",
        "of this lesson",
        "for this lesson",
        "from this lesson",
        "nothing in this lesson",
        "taught in this lesson",
        "based on the provided",
        "the provided passage",
        "the provided excerpt",
        "the provided text",
        "the provided principles",
    ];
    if PHRASES.iter().any(|phrase| normalized.contains(phrase)) {
        return true;
    }
    const SOURCE_REFS: &[&str] = &[
        "the passage's",
        "this passage's",
        "the passage",
        "this passage",
        "that passage",
        "the excerpt's",
        "the excerpt",
        "this excerpt",
        "that excerpt",
    ];
    SOURCE_REFS.iter().any(|phrase| {
        normalized.match_indices(phrase).any(|(pos, _)| {
            let before = normalized[..pos].chars().next_back();
            let after = normalized[pos + phrase.len()..].chars().next();
            before.is_none_or(|c| !c.is_ascii_alphanumeric())
                && after.is_none_or(|c| !c.is_ascii_alphanumeric())
        })
    })
}

fn find_phantom_version(candidate: &LearningAssessmentCandidate, quote: &str) -> Option<String> {
    let mut allowed = String::new();
    allowed.push_str(&candidate.prompt);
    for text in candidate.options.iter().chain(candidate.left.iter()) {
        allowed.push(' ');
        allowed.push_str(text);
    }
    for criterion in &candidate.rubric {
        allowed.push(' ');
        allowed.push_str(&criterion.title);
        allowed.push(' ');
        allowed.push_str(&criterion.description);
    }
    match &candidate.key {
        LearningAssessmentKey::Text(key) => {
            for variant in &key.variants {
                allowed.push(' ');
                allowed.push_str(variant);
            }
        }
        LearningAssessmentKey::Cloze(blanks) => {
            for blank in blanks {
                for variant in &blank.variants {
                    allowed.push(' ');
                    allowed.push_str(variant);
                }
            }
        }
        LearningAssessmentKey::Ordering(values) => {
            for value in values {
                allowed.push(' ');
                allowed.push_str(value);
            }
        }
        _ => {}
    }
    allowed.push(' ');
    allowed.push_str(quote);
    let allowed_versions = extract_three_part_versions(&allowed);

    std::iter::once(candidate.explanation.as_str())
        .chain(candidate.distractor_rationales.iter().map(String::as_str))
        .find_map(|text| {
            let lower = text.to_ascii_lowercase();
            extract_three_part_versions(text).into_iter().find(|ver| {
                !allowed_versions.contains(ver) && !is_named_standard_version(&lower, ver)
            })
        })
}

fn is_named_standard_version(lower_text: &str, ver: &str) -> bool {
    const STANDARDS: &[&str] = &[
        "semver",
        "semantic versioning",
        "openapi",
        "fhir r4b",
        "fhir r4",
        "fhir r5",
        "fhir",
        "hl7",
        "oauth",
        "rfc",
        "ecma",
    ];
    let mut start = 0;
    while let Some(pos) = lower_text.get(start..).and_then(|s| s.find(ver)) {
        let abs = start + pos;
        let prefix = lower_text
            .get(..abs)
            .unwrap_or_default()
            .trim_end_matches(|c: char| c == 'v' || c == '(' || c.is_whitespace());
        if STANDARDS.iter().any(|std_name| {
            prefix.strip_suffix(std_name).is_some_and(|before| {
                before.is_empty() || !before.ends_with(|c: char| c.is_ascii_alphanumeric())
            })
        }) {
            return true;
        }
        start = abs + ver.len();
    }
    false
}

fn extract_three_part_versions(text: &str) -> Vec<String> {
    let bytes = text.as_bytes();
    let mut versions = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        let Some(&ch) = bytes.get(i) else { break };
        if ch.is_ascii_digit() {
            let start = i;
            let prev = start.checked_sub(1).and_then(|idx| bytes.get(idx)).copied();
            if prev
                .is_some_and(|p| (p.is_ascii_alphanumeric() && p != b'v' && p != b'V') || p == b'.')
            {
                while bytes
                    .get(i)
                    .is_some_and(|b| b.is_ascii_digit() || *b == b'.')
                {
                    i += 1;
                }
                continue;
            }
            let mut end = start;
            let mut dots = 0usize;
            let mut ok = true;
            let mut seg_len = 0usize;
            while let Some(&b) = bytes.get(end) {
                if b.is_ascii_digit() {
                    seg_len += 1;
                    end += 1;
                } else if b == b'.' {
                    let next_digit = bytes.get(end + 1).is_some_and(u8::is_ascii_digit);
                    if seg_len > 0 && next_digit {
                        dots += 1;
                        seg_len = 0;
                        end += 1;
                    } else if seg_len == 0 {
                        ok = false;
                        break;
                    } else {
                        break;
                    }
                } else {
                    break;
                }
            }
            let trailing_part = bytes.get(end).is_some_and(|b| {
                b.is_ascii_digit()
                    || (*b == b'.' && bytes.get(end + 1).is_some_and(u8::is_ascii_digit))
            });
            if ok && dots == 2 && seg_len > 0 && !trailing_part {
                if let Some(slice) = text.get(start..end) {
                    versions.push(slice.to_owned());
                }
            }
            i = end.max(start + 1);
        } else {
            i += 1;
        }
    }
    versions
}
