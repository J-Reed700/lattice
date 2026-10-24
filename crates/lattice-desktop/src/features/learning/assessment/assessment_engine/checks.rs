//! What a bank item must satisfy before any form may use it (section 5.2).
use super::answers::whole_words;
use super::*;

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
