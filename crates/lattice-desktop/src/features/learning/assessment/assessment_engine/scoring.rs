//! Grading keyed answers with partial credit, per-part verdicts, and the key
//! as a learner sees it after submission.
use super::answers::{first_variant, numeric_answer, quoted};
use super::*;

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
