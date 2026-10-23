//! Subject-neutral assessment rules: blueprints, the item bank's keys,
//! immutable forms and deterministic grading.
//!
//! This is the deterministic boundary around model-authored item banks. It
//! selects forms by accepted outcome coverage, prefers unseen items, labels any
//! exposed repeat, shuffles option order per form, and grades every keyed
//! format with explainable partial credit (docs/design/2026-10-23-quizzes-and-
//! tests.md, section 3). Written answers, and short answers that match no
//! accepted variant, leave this module ungraded for the model grader.

use lattice_core::shared::error::{AppError, Result};
use rand::{seq::SliceRandom, SeedableRng};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeSet, HashMap, HashSet};
use unicode_normalization::UnicodeNormalization;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, specta::Type, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum LearningItemFormat {
    SingleChoice,
    MultiSelect,
    TrueFalse,
    ShortAnswer,
    Cloze,
    Matching,
    Ordering,
    Explanation,
    /// Kept for the practical path; quiz blueprints never author it.
    Artifact,
}

impl LearningItemFormat {
    /// Whether code grades this format against a key.
    pub fn is_keyed(self) -> bool {
        !matches!(self, Self::Explanation | Self::Artifact)
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, specta::Type, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LearningAssessmentPurpose {
    Practice,
    Checkpoint,
    ModuleTest,
    Cumulative,
    Transfer,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, specta::Type, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LearningFeedbackTiming {
    Immediate,
    AfterBatch,
    AfterSubmission,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LearningBlueprintRequirement {
    pub outcome_id: String,
    pub format: LearningItemFormat,
    pub count: usize,
    pub difficulty_min: u8,
    pub difficulty_max: u8,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LearningAssessmentBlueprint {
    pub id: String,
    pub purpose: LearningAssessmentPurpose,
    pub title: String,
    pub instructions: String,
    pub expected_minutes: u32,
    pub allowed_aids: Vec<String>,
    pub feedback_timing: LearningFeedbackTiming,
    pub requirements: Vec<LearningBlueprintRequirement>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LearningRubricCriterion {
    pub id: String,
    pub title: String,
    pub description: String,
    pub max_points: u32,
}

/// A number the answer must equal within an absolute tolerance, in the key's
/// unit. `unit_variants` are the unit spellings a response may carry.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LearningNumericKey {
    pub value: f64,
    pub tolerance: f64,
    #[serde(default)]
    pub unit_variants: Vec<String>,
}

/// Accepted spellings of one answer, and its number when it is one.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LearningTextKey {
    pub variants: Vec<String>,
    pub numeric: Option<LearningNumericKey>,
}

/// The application-owned key, stored as `key_json`. It never crosses IPC;
/// learners see a [`KeyReveal`] only after submission.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case", tag = "kind", content = "value")]
pub enum LearningAssessmentKey {
    /// Single choice and true/false (0 = True, 1 = False).
    Choice(usize),
    /// Multi-select: every correct option.
    Choices(BTreeSet<usize>),
    /// Short answer.
    Text(LearningTextKey),
    /// One key per `{{n}}` blank, in order.
    Cloze(Vec<LearningTextKey>),
    /// Per left-hand entry, the right-hand index it matches.
    Matching(Vec<usize>),
    /// The values in their correct order.
    Ordering(Vec<String>),
    /// Written answers are graded against the rubric.
    Rubric,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LearningAssessmentCandidate {
    pub id: String,
    pub outcome_ids: Vec<String>,
    pub format: LearningItemFormat,
    pub difficulty: u8,
    pub prompt: String,
    /// Choices, true/false labels, matching's right-hand side, or the values
    /// to order. Empty for short answer, cloze and written answers.
    pub options: Vec<String>,
    /// Matching's left-hand entries; empty for every other format.
    pub left: Vec<String>,
    pub source_version_ids: Vec<String>,
    pub rubric: Vec<LearningRubricCriterion>,
    pub key: LearningAssessmentKey,
    pub explanation: String,
    /// Per option, why it is wrong (choice formats); empty strings for keys.
    pub distractor_rationales: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LearningAssessmentFormItem {
    pub id: String,
    pub outcome_ids: Vec<String>,
    pub format: LearningItemFormat,
    pub prompt: String,
    /// Options in shown order.
    pub options: Vec<String>,
    pub left: Vec<String>,
    /// Shown position -> authored option index.
    pub option_order: Vec<usize>,
    pub source_version_ids: Vec<String>,
    pub rubric: Vec<LearningRubricCriterion>,
    pub previously_exposed: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LearningAssessmentForm {
    pub id: String,
    pub blueprint_id: String,
    pub purpose: LearningAssessmentPurpose,
    pub title: String,
    pub instructions: String,
    pub expected_minutes: u32,
    pub allowed_aids: Vec<String>,
    pub feedback_timing: LearningFeedbackTiming,
    pub items: Vec<LearningAssessmentFormItem>,
    pub created_at: i64,
}

/// A saved answer, in the item's shown option order.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LearningAssessmentResponse {
    pub item_id: String,
    /// Chosen shown positions: one for single choice and true/false, several
    /// for multi-select.
    #[serde(default)]
    pub selected_indices: Vec<usize>,
    pub text: Option<String>,
    /// One answer per cloze blank, in blank order.
    #[serde(default)]
    pub blank_responses: Vec<String>,
    /// Per left-hand matching entry, the chosen shown right-hand position.
    #[serde(default)]
    pub matches: Vec<Option<usize>>,
    #[serde(default)]
    pub ordered_values: Vec<String>,
    pub artifact_json: Option<serde_json::Value>,
}

impl LearningAssessmentResponse {
    pub fn empty(item_id: impl Into<String>) -> Self {
        Self {
            item_id: item_id.into(),
            selected_indices: vec![],
            text: None,
            blank_responses: vec![],
            matches: vec![],
            ordered_values: vec![],
            artifact_json: None,
        }
    }

    /// Whether anything was answered.
    pub fn is_answered(&self) -> bool {
        !self.selected_indices.is_empty()
            || self.text.as_deref().is_some_and(|t| !t.trim().is_empty())
            || self.blank_responses.iter().any(|b| !b.trim().is_empty())
            || self.matches.iter().any(Option::is_some)
            || !self.ordered_values.is_empty()
            || self.artifact_json.is_some()
    }
}

fn bounded(value: &str, label: &str, max: usize) -> Result<()> {
    let length = value.trim().chars().count();
    if length == 0 || length > max {
        return Err(AppError::InvalidInput(format!(
            "{label} must contain 1–{max} characters."
        )));
    }
    Ok(())
}

fn id(value: &str, label: &str) -> Result<()> {
    uuid::Uuid::parse_str(value)
        .map(|_| ())
        .map_err(|_| AppError::InvalidInput(format!("Invalid {label} ID")))
}

fn invalid(message: &str) -> AppError {
    AppError::InvalidInput(message.into())
}

pub fn validate_blueprint(blueprint: &LearningAssessmentBlueprint) -> Result<()> {
    id(&blueprint.id, "assessment blueprint")?;
    bounded(&blueprint.title, "Assessment title", 160)?;
    bounded(&blueprint.instructions, "Assessment instructions", 4_000)?;
    if blueprint.expected_minutes == 0 {
        return Err(invalid("Assessment time must be at least one minute."));
    }
    if blueprint.allowed_aids.len() > 24
        || blueprint
            .allowed_aids
            .iter()
            .any(|aid| aid.trim().is_empty() || aid.chars().count() > 160)
    {
        return Err(invalid("Assessment aid descriptions must be bounded."));
    }
    if blueprint.requirements.is_empty() || blueprint.requirements.len() > 256 {
        return Err(invalid(
            "An assessment blueprint needs 1–256 coverage requirements.",
        ));
    }
    let mut total = 0usize;
    let mut seen = HashSet::new();
    for requirement in &blueprint.requirements {
        id(&requirement.outcome_id, "outcome")?;
        if requirement.count == 0 || requirement.count > 12 {
            return Err(invalid(
                "Each assessment coverage requirement needs 1–12 items.",
            ));
        }
        if requirement.difficulty_min > requirement.difficulty_max
            || !(1..=5).contains(&requirement.difficulty_min)
            || !(1..=5).contains(&requirement.difficulty_max)
        {
            return Err(invalid(
                "Assessment difficulty must use the bounded 1–5 scale.",
            ));
        }
        if !seen.insert((requirement.outcome_id.as_str(), requirement.format)) {
            return Err(invalid(
                "Assessment coverage requirements must be unique by outcome and format.",
            ));
        }
        total += requirement.count;
    }
    if total > MAX_FORM_ITEMS {
        return Err(invalid(
            "An assessment form cannot require more than 200 items.",
        ));
    }
    Ok(())
}

/// The most items one form may hold (a course exam covers every prepared
/// lesson and module).
pub const MAX_FORM_ITEMS: usize = 200;

/// Whitespace-collapsed, lowercased text with diacritics removed.
fn folded(value: &str) -> String {
    value
        .nfkd()
        .filter(|c| !unicode_normalization::char::is_combining_mark(*c))
        .collect::<String>()
        .to_lowercase()
}

/// The normalised form of a typed answer: case, whitespace, punctuation,
/// diacritics, a leading article and thousands separators do not matter.
pub fn normalize_answer(value: &str) -> String {
    let folded = folded(value);
    let chars: Vec<char> = folded.chars().collect();
    let mut kept = String::with_capacity(chars.len());
    for (index, c) in chars.iter().enumerate() {
        let before = index.checked_sub(1).and_then(|i| chars.get(i));
        let after = chars.get(index + 1);
        let between_digits =
            before.is_some_and(char::is_ascii_digit) && after.is_some_and(char::is_ascii_digit);
        if (*c == ',' || *c == '\u{2009}' || *c == '_') && between_digits {
            continue; // thousands separator
        }
        if *c == '.' && between_digits {
            kept.push('.');
        } else if c.is_alphanumeric() {
            kept.push(*c);
        } else {
            kept.push(' ');
        }
    }
    let words: Vec<&str> = kept.split_whitespace().collect();
    let words = match words.split_first() {
        Some((first, rest)) if matches!(*first, "the" | "a" | "an") && !rest.is_empty() => rest,
        _ => &words[..],
    };
    words.join(" ")
}

/// The number a typed answer states, with an accepted unit removed. None when
/// the answer is not a number (or carries a unit the key does not accept).
pub fn parse_number(value: &str, unit_variants: &[String]) -> Option<f64> {
    let mut text = folded(value).trim().replace(['\u{2212}', '\u{2013}'], "-");
    for unit in unit_variants
        .iter()
        .map(|u| folded(u).trim().to_string())
        .filter(|u| !u.is_empty())
    {
        if let Some(stripped) = text.strip_suffix(unit.as_str()) {
            text = stripped.trim().to_string();
            break;
        }
    }
    let cleaned: String = {
        let chars: Vec<char> = text.chars().collect();
        chars
            .iter()
            .enumerate()
            .filter(|(index, c)| {
                let before = index.checked_sub(1).and_then(|i| chars.get(i));
                let after = chars.get(index + 1);
                !((**c == ',' || **c == ' ' || **c == '\u{2009}' || **c == '_')
                    && before.is_some_and(char::is_ascii_digit)
                    && after.is_some_and(char::is_ascii_digit))
            })
            .map(|(_, c)| *c)
            .collect()
    };
    let cleaned = cleaned.trim().trim_start_matches('+');
    if cleaned.is_empty() {
        return None;
    }
    if let Some((numerator, denominator)) = cleaned.split_once('/') {
        let n: f64 = numerator.trim().parse().ok()?;
        let d: f64 = denominator.trim().parse().ok()?;
        return (d != 0.0 && (n / d).is_finite()).then_some(n / d);
    }
    if !cleaned
        .chars()
        .all(|c| c.is_ascii_digit() || matches!(c, '.' | '-' | 'e'))
    {
        return None;
    }
    cleaned.parse::<f64>().ok().filter(|v| v.is_finite())
}

/// A number written for people: no trailing zeros, no exponent for ordinary
/// magnitudes.
pub fn show_number(value: f64) -> String {
    let text = format!("{value:.6}");
    let text = text.trim_end_matches('0').trim_end_matches('.');
    if text == "-0" {
        "0".into()
    } else {
        text.into()
    }
}

fn quoted(value: &str) -> String {
    format!("“{}”", value.trim())
}

fn whole_words(haystack: &str, needle: &str) -> bool {
    !needle.is_empty() && format!(" {haystack} ").contains(&format!(" {needle} "))
}

/// The leak check (section 5.2): the key's own text in the stem. Comparison
/// stems name several options, so a short key counts only when it is the one
/// option the stem names; a key of four or more words counts on any shared
/// run of four words.
pub fn key_leaks(stem: &str, key_text: &str, other_options: &[String]) -> bool {
    let stem = normalize_answer(stem);
    let key = normalize_answer(key_text);
    if key.chars().count() < 3 {
        return false;
    }
    let words: Vec<&str> = key.split_whitespace().collect();
    if words.len() >= 4 {
        return words
            .windows(4)
            .any(|gram| whole_words(&stem, &gram.join(" ")));
    }
    whole_words(&stem, &key)
        && !other_options
            .iter()
            .map(|o| normalize_answer(o))
            .any(|o| o.chars().count() >= 3 && whole_words(&stem, &o))
}

/// Length bias (section 5.2): the key is the unique longest option by more
/// than 60 % of the median option length.
pub fn length_biased(options: &[String], key: usize) -> bool {
    let mut lengths: Vec<usize> = options.iter().map(|o| o.trim().chars().count()).collect();
    let Some(&key_length) = lengths.get(key) else {
        return false;
    };
    let longest_other = lengths
        .iter()
        .enumerate()
        .filter(|(index, _)| *index != key)
        .map(|(_, l)| *l)
        .max()
        .unwrap_or(0);
    lengths.sort_unstable();
    let median = lengths.get(lengths.len() / 2).copied().unwrap_or(0) as f64;
    key_length > longest_other && (key_length - longest_other) as f64 > 0.6 * median
}

/// Normalised stem similarity at or above which two items of one outcome are
/// the same question (word-set Jaccard).
pub fn near_duplicate(a: &str, b: &str) -> bool {
    let a: HashSet<String> = normalize_answer(a)
        .split_whitespace()
        .map(str::to_owned)
        .collect();
    let b: HashSet<String> = normalize_answer(b)
        .split_whitespace()
        .map(str::to_owned)
        .collect();
    let union = a.union(&b).count();
    union > 0 && a.intersection(&b).count() as f64 / union as f64 >= 0.8
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

fn distinct_options(options: &[String]) -> bool {
    let normalized: HashSet<String> = options.iter().map(|o| normalize_answer(o)).collect();
    normalized.len() == options.len() && !normalized.iter().any(String::is_empty)
}

fn text_key_valid(key: &LearningTextKey) -> bool {
    !key.variants.is_empty()
        && key.variants.len() <= 12
        && key
            .variants
            .iter()
            .all(|v| !v.trim().is_empty() && v.chars().count() <= 500)
        && key.numeric.as_ref().is_none_or(|n| {
            n.value.is_finite()
                && n.tolerance.is_finite()
                && n.tolerance >= 0.0
                && n.unit_variants.len() <= 12
        })
}

pub const TRUE_FALSE: [&str; 2] = ["True", "False"];

pub fn validate_candidate(candidate: &LearningAssessmentCandidate) -> Result<()> {
    id(&candidate.id, "assessment item")?;
    bounded(&candidate.prompt, "Assessment prompt", 8_000)?;
    if candidate.outcome_ids.is_empty() || candidate.outcome_ids.len() > 8 {
        return Err(invalid("An assessment item must address 1–8 outcomes."));
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
    if candidate.source_version_ids.len() > 24 {
        return Err(invalid(
            "Assessment items support at most 24 frozen source versions.",
        ));
    }
    for source in &candidate.source_version_ids {
        id(source, "source version")?;
    }
    if candidate.explanation.chars().count() > 2_000
        || candidate.distractor_rationales.len() > candidate.options.len().max(8)
    {
        return Err(invalid("Assessment item explanations are bounded."));
    }
    let no_left = candidate.left.is_empty();
    match (&candidate.format, &candidate.key) {
        (LearningItemFormat::SingleChoice, LearningAssessmentKey::Choice(key)) => {
            if !(2..=8).contains(&candidate.options.len()) || *key >= candidate.options.len() {
                return Err(invalid(
                    "Single-choice items need 2–8 options and one valid key.",
                ));
            }
            if !distinct_options(&candidate.options) || !no_left {
                return Err(invalid("Choice options must be unique and non-empty."));
            }
            let others: Vec<String> = candidate
                .options
                .iter()
                .enumerate()
                .filter(|(i, _)| i != key)
                .map(|(_, o)| o.clone())
                .collect();
            if candidate
                .options
                .get(*key)
                .is_some_and(|k| key_leaks(&candidate.prompt, k, &others))
            {
                return Err(invalid("The question's stem gives away its answer."));
            }
            if candidate.options.len() >= 3 && length_biased(&candidate.options, *key) {
                return Err(invalid(
                    "The correct option stands out as much the longest.",
                ));
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
            if !(4..=6).contains(&candidate.options.len())
                || !(2..=3).contains(&keys.len())
                || keys.iter().any(|k| *k >= candidate.options.len())
            {
                return Err(invalid(
                    "Multi-select items need 4–6 options with 2–3 correct.",
                ));
            }
            if !distinct_options(&candidate.options) || !no_left {
                return Err(invalid("Choice options must be unique and non-empty."));
            }
        }
        (LearningItemFormat::ShortAnswer, LearningAssessmentKey::Text(key)) => {
            if !text_key_valid(key) || !candidate.options.is_empty() || !no_left {
                return Err(invalid(
                    "Short-answer items need 1–12 accepted answers and no choices.",
                ));
            }
            if key
                .variants
                .iter()
                .any(|v| key_leaks(&candidate.prompt, v, &[]))
            {
                return Err(invalid("The question's stem gives away its answer."));
            }
        }
        (LearningItemFormat::Cloze, LearningAssessmentKey::Cloze(blanks)) => {
            let found = cloze_blanks(&candidate.prompt);
            let expected: Vec<usize> = (1..=blanks.len()).collect();
            if blanks.is_empty()
                || blanks.len() > 8
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
            if !(3..=5).contains(&lefts)
                || !(rights == lefts || rights == lefts + 1)
                || pairs.len() != lefts
                || pairs.iter().any(|p| *p >= rights)
                || pairs.iter().collect::<HashSet<_>>().len() != pairs.len()
                || !distinct_options(&candidate.left)
                || !distinct_options(&candidate.options)
            {
                return Err(invalid(
                    "Matching items need 3–5 distinct pairs, at most one extra right-hand entry, and no shared right-hand answers.",
                ));
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
            if candidate.rubric.is_empty() || candidate.rubric.len() > 12 || !no_left {
                return Err(invalid("Written items need 1–12 visible rubric criteria."));
            }
            let mut criteria = HashSet::new();
            for criterion in &candidate.rubric {
                id(&criterion.id, "rubric criterion")?;
                bounded(&criterion.title, "Rubric criterion title", 160)?;
                bounded(
                    &criterion.description,
                    "Rubric criterion description",
                    1_200,
                )?;
                if criterion.max_points == 0 || criterion.max_points > 100 {
                    return Err(invalid(
                        "Rubric criterion points must be between 1 and 100.",
                    ));
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
    Ok(())
}

/// Whether a format shuffles its options per form.
fn shuffles(format: LearningItemFormat) -> bool {
    matches!(
        format,
        LearningItemFormat::SingleChoice
            | LearningItemFormat::MultiSelect
            | LearningItemFormat::Matching
            | LearningItemFormat::Ordering
    )
}

/// A permutation of `0..len` (shown position -> authored index), stable for
/// one seed text.
pub fn option_order(format: LearningItemFormat, len: usize, seed: &str) -> Vec<usize> {
    let mut order: Vec<usize> = (0..len).collect();
    if shuffles(format) && len > 1 {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        std::hash::Hash::hash(seed, &mut hasher);
        let mut rng = rand::rngs::StdRng::seed_from_u64(std::hash::Hasher::finish(&hasher));
        order.shuffle(&mut rng);
    }
    order
}

/// Reorders an authored candidate's options at random before it is stored, so
/// the bank never keeps the model's position habits, and moves its key and
/// distractor rationales with them.
pub fn shuffle_authored(candidate: &mut LearningAssessmentCandidate) {
    if !shuffles(candidate.format) || candidate.format == LearningItemFormat::Ordering {
        if candidate.format == LearningItemFormat::Ordering {
            candidate.options.shuffle(&mut rand::thread_rng());
        }
        return;
    }
    let mut order: Vec<usize> = (0..candidate.options.len()).collect();
    order.shuffle(&mut rand::thread_rng());
    // order[new] = old
    let mut new_of_old = vec![0; order.len()];
    for (new, old) in order.iter().enumerate() {
        if let Some(slot) = new_of_old.get_mut(*old) {
            *slot = new;
        }
    }
    let at = |old: &usize| new_of_old.get(*old).copied().unwrap_or(*old);
    candidate.options = order
        .iter()
        .filter_map(|old| candidate.options.get(*old).cloned())
        .collect();
    if candidate.distractor_rationales.len() == order.len() {
        candidate.distractor_rationales = order
            .iter()
            .filter_map(|old| candidate.distractor_rationales.get(*old).cloned())
            .collect();
    }
    candidate.key = match &candidate.key {
        LearningAssessmentKey::Choice(k) => LearningAssessmentKey::Choice(at(k)),
        LearningAssessmentKey::Choices(keys) => {
            LearningAssessmentKey::Choices(keys.iter().map(at).collect())
        }
        LearningAssessmentKey::Matching(pairs) => {
            LearningAssessmentKey::Matching(pairs.iter().map(at).collect())
        }
        other => other.clone(),
    };
}

/// Select a deterministic immutable form. Candidate order never controls the
/// result: unseen items are sorted by difficulty distance and stable ID. Each
/// item's options are shown in an order drawn from the form and item IDs, so
/// a retake reshuffles them.
pub fn build_assessment_form(
    blueprint: &LearningAssessmentBlueprint,
    candidates: &[LearningAssessmentCandidate],
    exposed_item_ids: &HashSet<String>,
    form_id: String,
    created_at: i64,
) -> Result<LearningAssessmentForm> {
    validate_blueprint(blueprint)?;
    id(&form_id, "assessment form")?;
    let mut candidate_ids = HashSet::new();
    for candidate in candidates {
        validate_candidate(candidate)?;
        if !candidate_ids.insert(candidate.id.as_str()) {
            return Err(invalid("Assessment item-bank IDs must be unique."));
        }
    }
    let mut selected = Vec::new();
    let mut used = HashSet::new();
    for requirement in &blueprint.requirements {
        let center =
            (u16::from(requirement.difficulty_min) + u16::from(requirement.difficulty_max)) / 2;
        let mut eligible = candidates
            .iter()
            .filter(|candidate| {
                eligible_for(candidate, requirement) && !used.contains(candidate.id.as_str())
            })
            .collect::<Vec<_>>();
        eligible.sort_by_key(|candidate| {
            (
                exposed_item_ids.contains(&candidate.id),
                (i32::from(candidate.difficulty) - i32::from(center)).unsigned_abs(),
                candidate.id.as_str(),
            )
        });
        if eligible.len() < requirement.count {
            return Err(AppError::InvalidState(
                "This quiz's question bank is not ready yet. Prepare its questions first.".into(),
            ));
        }
        for candidate in eligible.into_iter().take(requirement.count) {
            used.insert(candidate.id.as_str());
            let order = option_order(
                candidate.format,
                candidate.options.len(),
                &format!("{form_id}:{}", candidate.id),
            );
            selected.push(LearningAssessmentFormItem {
                id: candidate.id.clone(),
                outcome_ids: candidate.outcome_ids.clone(),
                format: candidate.format,
                prompt: candidate.prompt.clone(),
                options: order
                    .iter()
                    .filter_map(|i| candidate.options.get(*i).cloned())
                    .collect(),
                left: candidate.left.clone(),
                option_order: order,
                source_version_ids: candidate.source_version_ids.clone(),
                rubric: candidate.rubric.clone(),
                previously_exposed: exposed_item_ids.contains(&candidate.id),
            });
        }
    }
    if selected.len() > MAX_FORM_ITEMS {
        return Err(invalid("Assessment form exceeds the item limit."));
    }
    Ok(LearningAssessmentForm {
        id: form_id,
        blueprint_id: blueprint.id.clone(),
        purpose: blueprint.purpose,
        title: blueprint.title.clone(),
        instructions: blueprint.instructions.clone(),
        expected_minutes: blueprint.expected_minutes,
        allowed_aids: blueprint.allowed_aids.clone(),
        feedback_timing: blueprint.feedback_timing,
        items: selected,
        created_at,
    })
}

/// Whether a bank item can fill a blueprint requirement.
pub fn eligible_for(
    candidate: &LearningAssessmentCandidate,
    requirement: &LearningBlueprintRequirement,
) -> bool {
    candidate.format == requirement.format
        && candidate.outcome_ids.contains(&requirement.outcome_id)
        && (requirement.difficulty_min..=requirement.difficulty_max).contains(&candidate.difficulty)
}

/// A response translated from shown positions back to authored indices.
pub fn to_authored(
    response: &LearningAssessmentResponse,
    option_order: &[usize],
) -> LearningAssessmentResponse {
    let authored = |shown: &usize| option_order.get(*shown).copied().unwrap_or(usize::MAX);
    LearningAssessmentResponse {
        selected_indices: response.selected_indices.iter().map(authored).collect(),
        matches: response
            .matches
            .iter()
            .map(|m| m.as_ref().map(authored))
            .collect(),
        ..response.clone()
    }
}

/// How a keyed item scored: a fraction of its points, whether it is fully
/// right, and why it lost points, in words.
#[derive(Debug, Clone, PartialEq)]
pub struct KeyedGrade {
    pub fraction: f64,
    pub correct: bool,
    pub detail: String,
}

/// A keyed item's result, or a short answer that matched no accepted variant
/// and goes to the model's equivalence judge.
#[derive(Debug, Clone, PartialEq)]
pub enum KeyedOutcome {
    Graded(KeyedGrade),
    NeedsJudge,
}

pub const NO_ANSWER: &str = "No answer given";

fn graded(fraction: f64, detail: String) -> KeyedOutcome {
    let fraction = fraction.clamp(0.0, 1.0);
    KeyedOutcome::Graded(KeyedGrade {
        fraction,
        correct: fraction >= 1.0,
        detail,
    })
}

fn first_variant(key: &LearningTextKey) -> String {
    match &key.numeric {
        Some(n) => numeric_answer(n),
        None => key.variants.first().cloned().unwrap_or_default(),
    }
}

fn numeric_answer(n: &LearningNumericKey) -> String {
    let unit = n
        .unit_variants
        .first()
        .map(|u| format!(" {u}"))
        .unwrap_or_default();
    if n.tolerance > 0.0 {
        format!(
            "{} ± {}{unit}",
            show_number(n.value),
            show_number(n.tolerance)
        )
    } else {
        format!("{}{unit}", show_number(n.value))
    }
}

/// Text against one text key: Some(true/false) when code decides, None when
/// only the model's equivalence judge can.
pub fn text_matches(answer: &str, key: &LearningTextKey) -> Option<bool> {
    let units = key
        .numeric
        .as_ref()
        .map(|n| n.unit_variants.as_slice())
        .unwrap_or(&[]);
    if let Some(given) = parse_number(answer, units) {
        if let Some(n) = &key.numeric {
            return Some((given - n.value).abs() <= n.tolerance + 1e-9 * n.value.abs().max(1.0));
        }
        return Some(key.variants.iter().any(|v| {
            parse_number(v, &[]).is_some_and(|expected| (expected - given).abs() <= 1e-9)
                || normalize_answer(v) == normalize_answer(answer)
        }));
    }
    let normalized = normalize_answer(answer);
    if key
        .variants
        .iter()
        .any(|v| normalize_answer(v) == normalized)
    {
        return Some(true);
    }
    None
}

/// Grade one keyed item against its key. `response` is in authored terms
/// (see [`to_authored`]). An empty response scores 0 with the reason "No
/// answer given" and never reaches a model.
pub fn grade_keyed(
    candidate: &LearningAssessmentCandidate,
    response: &LearningAssessmentResponse,
) -> Result<KeyedOutcome> {
    if !response.is_answered() {
        return Ok(graded(0.0, NO_ANSWER.into()));
    }
    let option = |i: usize| {
        candidate
            .options
            .get(i)
            .map(|o| quoted(o))
            .unwrap_or_else(|| "an unavailable option".into())
    };
    Ok(match &candidate.key {
        LearningAssessmentKey::Choice(key) => match response.selected_indices.as_slice() {
            [chosen] if chosen == key => graded(1.0, String::new()),
            [chosen] => graded(
                0.0,
                format!(
                    "You chose {}; the answer is {}.",
                    option(*chosen),
                    option(*key)
                ),
            ),
            _ => graded(0.0, "Choose exactly one option.".into()),
        },
        LearningAssessmentKey::Choices(keys) => {
            let chosen: BTreeSet<usize> = response.selected_indices.iter().copied().collect();
            let hits = chosen.intersection(keys).count();
            let wrong = chosen.difference(keys).count();
            let fraction = (hits as f64 - wrong as f64).max(0.0) / keys.len().max(1) as f64;
            let mut detail = if hits == keys.len() && wrong == 0 {
                String::new()
            } else {
                format!(
                    "{hits} of {} right, {wrong} wrong pick{}.",
                    keys.len(),
                    if wrong == 1 { "" } else { "s" }
                )
            };
            let missed: Vec<String> = keys.difference(&chosen).map(|i| option(*i)).collect();
            if !missed.is_empty() {
                detail.push_str(&format!(" You missed {}.", missed.join(", ")));
            }
            graded(fraction, detail.trim().to_string())
        }
        LearningAssessmentKey::Text(key) => {
            let answer = response.text.as_deref().unwrap_or("").trim();
            match text_matches(answer, key) {
                Some(true) => graded(1.0, String::new()),
                Some(false) => match &key.numeric {
                    Some(n) => graded(
                        0.0,
                        format!(
                            "You gave {}; the answer is {}.",
                            parse_number(answer, &n.unit_variants)
                                .map(show_number)
                                .unwrap_or_else(|| answer.to_string()),
                            numeric_answer(n)
                        ),
                    ),
                    None => graded(
                        0.0,
                        format!(
                            "{} isn't an accepted answer; the answer is {}.",
                            quoted(answer),
                            quoted(&first_variant(key))
                        ),
                    ),
                },
                None => KeyedOutcome::NeedsJudge,
            }
        }
        LearningAssessmentKey::Cloze(blanks) => {
            let mut right = 0usize;
            let mut wrong = Vec::new();
            for (index, key) in blanks.iter().enumerate() {
                let answer = response
                    .blank_responses
                    .get(index)
                    .map(|a| a.trim())
                    .unwrap_or("");
                if answer.is_empty() {
                    wrong.push(format!("Blank {}: no answer given.", index + 1));
                } else if text_matches(answer, key) == Some(true) {
                    right += 1;
                } else {
                    wrong.push(format!(
                        "Blank {}: {} isn't accepted; the answer is {}.",
                        index + 1,
                        quoted(answer),
                        quoted(&first_variant(key))
                    ));
                }
            }
            graded(right as f64 / blanks.len().max(1) as f64, wrong.join(" "))
        }
        LearningAssessmentKey::Matching(pairs) => {
            let mut right = 0usize;
            let mut wrong = Vec::new();
            for (index, expected) in pairs.iter().enumerate() {
                let left = candidate
                    .left
                    .get(index)
                    .map(|l| quoted(l))
                    .unwrap_or_default();
                match response.matches.get(index).copied().flatten() {
                    Some(chosen) if chosen == *expected => right += 1,
                    Some(chosen) => wrong.push(format!(
                        "Pair {}: {left} matches {}, not {}.",
                        index + 1,
                        option(*expected),
                        option(chosen)
                    )),
                    None => wrong.push(format!(
                        "Pair {}: {left} matches {}; no match was chosen.",
                        index + 1,
                        option(*expected)
                    )),
                }
            }
            graded(right as f64 / pairs.len().max(1) as f64, wrong.join(" "))
        }
        LearningAssessmentKey::Ordering(order) => {
            let given = &response.ordered_values;
            let same_values = given.len() == order.len()
                && given.iter().collect::<HashSet<_>>() == order.iter().collect::<HashSet<_>>();
            if !same_values {
                graded(0.0, "Order every value exactly once.".into())
            } else {
                let position: HashMap<&String, usize> =
                    given.iter().enumerate().map(|(i, v)| (v, i)).collect();
                let mut in_order = 0usize;
                let mut total = 0usize;
                for i in 0..order.len() {
                    for j in i + 1..order.len() {
                        total += 1;
                        if let (Some(a), Some(b)) = (
                            order.get(i).and_then(|v| position.get(v)),
                            order.get(j).and_then(|v| position.get(v)),
                        ) {
                            if a < b {
                                in_order += 1;
                            }
                        }
                    }
                }
                let detail = if in_order == total {
                    String::new()
                } else {
                    format!("{in_order} of {total} pairs in order.")
                };
                graded(in_order as f64 / total.max(1) as f64, detail)
            }
        }
        LearningAssessmentKey::Rubric => {
            return Err(invalid("Written answers are graded against their rubric."))
        }
    })
}

/// One part of a keyed answer and whether it was right: a cloze blank, a
/// multi-select option, a matching pair or an ordering position. `index` is
/// in the item's shown order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PartResult {
    pub index: usize,
    pub correct: bool,
    pub detail: String,
}

/// Per-part verdicts for a keyed answer, so a result can say which part was
/// wrong and why. `response` is in authored terms (see [`to_authored`]).
/// Empty for single-answer formats, whose detail already says it.
pub fn part_results(
    candidate: &LearningAssessmentCandidate,
    response: &LearningAssessmentResponse,
    option_order: &[usize],
) -> Vec<PartResult> {
    let shown = |authored: usize| {
        option_order
            .iter()
            .position(|a| *a == authored)
            .unwrap_or(authored)
    };
    let option = |i: usize| {
        candidate
            .options
            .get(i)
            .map(|o| quoted(o))
            .unwrap_or_default()
    };
    match &candidate.key {
        LearningAssessmentKey::Choices(keys) => {
            let chosen: BTreeSet<usize> = response.selected_indices.iter().copied().collect();
            let mut parts: Vec<PartResult> = chosen
                .union(keys)
                .map(|i| {
                    let (correct, detail) = match (chosen.contains(i), keys.contains(i)) {
                        (true, true) => (true, String::new()),
                        (true, false) => {
                            (false, format!("{} is not one of the answers.", option(*i)))
                        }
                        _ => (
                            false,
                            format!("{} is one of the answers and was not chosen.", option(*i)),
                        ),
                    };
                    PartResult {
                        index: shown(*i),
                        correct,
                        detail,
                    }
                })
                .collect();
            parts.sort_by_key(|p| p.index);
            parts
        }
        LearningAssessmentKey::Cloze(blanks) => blanks
            .iter()
            .enumerate()
            .map(|(index, key)| {
                let answer = response
                    .blank_responses
                    .get(index)
                    .map(|a| a.trim())
                    .unwrap_or("");
                let correct = !answer.is_empty() && text_matches(answer, key) == Some(true);
                PartResult {
                    index,
                    correct,
                    detail: if correct {
                        String::new()
                    } else if answer.is_empty() {
                        format!(
                            "No answer given; the answer is {}.",
                            quoted(&first_variant(key))
                        )
                    } else {
                        format!(
                            "{} isn't accepted; the answer is {}.",
                            quoted(answer),
                            quoted(&first_variant(key))
                        )
                    },
                }
            })
            .collect(),
        LearningAssessmentKey::Matching(pairs) => pairs
            .iter()
            .enumerate()
            .map(|(index, expected)| {
                let chosen = response.matches.get(index).copied().flatten();
                let left = candidate
                    .left
                    .get(index)
                    .map(|l| quoted(l))
                    .unwrap_or_default();
                PartResult {
                    index,
                    correct: chosen == Some(*expected),
                    detail: match chosen {
                        Some(c) if c == *expected => String::new(),
                        Some(c) => {
                            format!("{left} matches {}, not {}.", option(*expected), option(c))
                        }
                        None => {
                            format!("{left} matches {}; no match was chosen.", option(*expected))
                        }
                    },
                }
            })
            .collect(),
        LearningAssessmentKey::Ordering(order) => response
            .ordered_values
            .iter()
            .enumerate()
            .map(|(index, value)| {
                let correct = order.get(index) == Some(value);
                PartResult {
                    index,
                    correct,
                    detail: if correct {
                        String::new()
                    } else {
                        match order.iter().position(|v| v == value) {
                            Some(at) => {
                                format!("{} belongs in position {}.", quoted(value), at + 1)
                            }
                            None => format!("{} is not one of the values.", quoted(value)),
                        }
                    },
                }
            })
            .collect(),
        _ => vec![],
    }
}

/// What an input may say about a typed answer before submission without
/// giving it away: whether it is a number, and in which unit.
pub fn answer_hint(candidate: &LearningAssessmentCandidate) -> Option<(bool, Option<String>)> {
    match &candidate.key {
        LearningAssessmentKey::Text(key) => Some(match &key.numeric {
            Some(n) => (true, n.unit_variants.first().cloned()),
            None => (false, None),
        }),
        _ => None,
    }
}

/// The key after submission, in the item's shown option order.
#[derive(Debug, Clone, PartialEq)]
pub struct KeyReveal {
    pub correct_indices: Vec<usize>,
    pub accepted_answer: Option<String>,
    pub ordered_values: Vec<String>,
    pub matches: Vec<usize>,
    pub blanks: Vec<String>,
    pub explanation: String,
    pub distractor_rationales: Vec<String>,
}

pub fn key_reveal(candidate: &LearningAssessmentCandidate, option_order: &[usize]) -> KeyReveal {
    let shown = |authored: usize| {
        option_order
            .iter()
            .position(|a| *a == authored)
            .unwrap_or(authored)
    };
    let mut reveal = KeyReveal {
        correct_indices: vec![],
        accepted_answer: None,
        ordered_values: vec![],
        matches: vec![],
        blanks: vec![],
        explanation: candidate.explanation.clone(),
        distractor_rationales: if option_order.len() == candidate.distractor_rationales.len() {
            option_order
                .iter()
                .filter_map(|a| candidate.distractor_rationales.get(*a).cloned())
                .collect()
        } else {
            candidate.distractor_rationales.clone()
        },
    };
    match &candidate.key {
        LearningAssessmentKey::Choice(k) => reveal.correct_indices = vec![shown(*k)],
        LearningAssessmentKey::Choices(keys) => {
            let mut indices: Vec<usize> = keys.iter().map(|k| shown(*k)).collect();
            indices.sort_unstable();
            reveal.correct_indices = indices;
        }
        LearningAssessmentKey::Text(key) => reveal.accepted_answer = Some(first_variant(key)),
        LearningAssessmentKey::Cloze(blanks) => {
            reveal.blanks = blanks.iter().map(first_variant).collect()
        }
        LearningAssessmentKey::Matching(pairs) => {
            reveal.matches = pairs.iter().map(|p| shown(*p)).collect()
        }
        LearningAssessmentKey::Ordering(order) => reveal.ordered_values = order.clone(),
        LearningAssessmentKey::Rubric => {}
    }
    reveal
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(seed: u128) -> String {
        uuid::Uuid::from_u128(seed).to_string()
    }

    fn blueprint() -> LearningAssessmentBlueprint {
        LearningAssessmentBlueprint {
            id: id(1),
            purpose: LearningAssessmentPurpose::ModuleTest,
            title: "Concurrency under changed constraints".into(),
            instructions: "Answer from the frozen scenario and explain decisions.".into(),
            expected_minutes: 30,
            allowed_aids: vec!["Saved source library".into()],
            feedback_timing: LearningFeedbackTiming::AfterSubmission,
            requirements: vec![LearningBlueprintRequirement {
                outcome_id: id(2),
                format: LearningItemFormat::SingleChoice,
                count: 2,
                difficulty_min: 2,
                difficulty_max: 4,
            }],
        }
    }

    fn candidate(seed: u128, difficulty: u8) -> LearningAssessmentCandidate {
        LearningAssessmentCandidate {
            id: id(seed),
            outcome_ids: vec![id(2)],
            format: LearningItemFormat::SingleChoice,
            difficulty,
            prompt: format!("Which invariant holds in scenario {seed}?"),
            options: vec!["Alpha".into(), "Bravo".into(), "Charlie".into()],
            left: vec![],
            source_version_ids: vec![id(20)],
            rubric: vec![],
            key: LearningAssessmentKey::Choice(1),
            explanation: "Bravo holds.".into(),
            distractor_rationales: vec![],
        }
    }

    #[test]
    fn form_selection_prefers_unseen_items_and_labels_required_repeats() -> Result<()> {
        let candidates = vec![candidate(10, 3), candidate(11, 3), candidate(12, 4)];
        let exposed = HashSet::from([id(10), id(11)]);
        let form = build_assessment_form(&blueprint(), &candidates, &exposed, id(30), 40)?;
        assert_eq!(form.items.len(), 2);
        assert_eq!(form.items[0].id, id(12));
        assert!(!form.items[0].previously_exposed);
        assert!(form.items[1].previously_exposed);
        Ok(())
    }

    #[test]
    fn insufficient_blueprint_coverage_is_an_error_instead_of_synthetic_content() {
        assert!(build_assessment_form(
            &blueprint(),
            &[candidate(10, 3)],
            &HashSet::new(),
            id(30),
            40,
        )
        .is_err());
    }

    #[test]
    fn shown_options_follow_the_stored_order() -> Result<()> {
        let candidates = vec![candidate(10, 3), candidate(11, 3)];
        let form = build_assessment_form(&blueprint(), &candidates, &HashSet::new(), id(30), 40)?;
        for item in &form.items {
            let authored = candidates.iter().find(|c| c.id == item.id).unwrap();
            for (shown, original) in item.option_order.iter().enumerate() {
                assert_eq!(item.options[shown], authored.options[*original]);
            }
        }
        Ok(())
    }
}
