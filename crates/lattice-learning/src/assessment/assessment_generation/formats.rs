//! Per-format item schemas, and turning a model's item into a checked bank
//! item (section 5.2).
use super::*;
use crate::assessment_engine::checks::{
    CHOICES_PER_QUESTION, MATCHING_PAIRS, MAX_SOURCE_VERSIONS, MULTI_SELECT_CORRECT,
    MULTI_SELECT_OPTIONS,
};
use crate::generation::{validate_excerpt_quote, MIN_QUOTE_CHARS, MIN_QUOTE_WORDS};
use std::ops::RangeInclusive;

/// How many accepted spellings, cloze blanks and ordered values a writer is asked for.
pub(super) const SHORT_ANSWERS_ASKED: RangeInclusive<usize> = 3..=12;
pub(super) const CLOZE_BLANKS_ASKED: RangeInclusive<usize> = 1..=4;
pub(super) const ORDERING_ASKED: RangeInclusive<usize> = 3..=8;

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct RawPair {
    left: String,
    right: String,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct RawBlank {
    accepted_answers: Vec<String>,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub(super) struct RawItem {
    prompt: String,
    options: Vec<String>,
    answer_index: usize,
    distractor_rationales: Vec<String>,
    is_true: bool,
    correct_indices: Vec<usize>,
    accepted_answers: Vec<String>,
    is_numeric: bool,
    numeric_value: f64,
    tolerance: f64,
    units: Vec<String>,
    blanks: Vec<RawBlank>,
    pairs: Vec<RawPair>,
    extra_right: String,
    ordered_values: Vec<String>,
    rubric: Vec<crate::teaching::GeneratedCriterion>,
    explanation: String,
    block_index: usize,
    quote: String,
    difficulty: u8,
}

fn strings(count: &RangeInclusive<usize>) -> Value {
    json!({"type":"array","minItems":count.start(),"maxItems":count.end(),"items":{"type":"string","minLength":1}})
}

pub(super) fn item_schema(
    format: LearningItemFormat,
    passages: usize,
    min_d: u8,
    max_d: u8,
) -> Value {
    let mut properties = serde_json::Map::new();
    let mut put = |name: &str, schema: Value| {
        properties.insert(name.into(), schema);
    };
    put("prompt", json!({"type":"string","minLength":1}));
    match format {
        LearningItemFormat::SingleChoice => {
            let choices = CHOICES_PER_QUESTION..=CHOICES_PER_QUESTION;
            put("options", strings(&choices));
            put("distractorRationales", strings(&choices));
            put(
                "answerIndex",
                json!({"type":"integer","minimum":0,"maximum":CHOICES_PER_QUESTION.saturating_sub(1)}),
            );
        }
        LearningItemFormat::TrueFalse => put("isTrue", json!({"type":"boolean"})),
        LearningItemFormat::MultiSelect => {
            put("options", strings(&MULTI_SELECT_OPTIONS));
            put("distractorRationales", strings(&MULTI_SELECT_OPTIONS));
            put(
                "correctIndices",
                json!({"type":"array","minItems":MULTI_SELECT_CORRECT.start(),"maxItems":MULTI_SELECT_CORRECT.end(),"uniqueItems":true,"items":{"type":"integer","minimum":0,"maximum":MULTI_SELECT_OPTIONS.end().saturating_sub(1)}}),
            );
        }
        LearningItemFormat::ShortAnswer => {
            put("acceptedAnswers", strings(&SHORT_ANSWERS_ASKED));
            put("isNumeric", json!({"type":"boolean"}));
            put("numericValue", json!({"type":"number"}));
            put("tolerance", json!({"type":"number","minimum":0}));
            put(
                "units",
                json!({"type":"array","maxItems":8,"items":{"type":"string"}}),
            );
        }
        LearningItemFormat::Cloze => put(
            "blanks",
            json!({"type":"array","minItems":CLOZE_BLANKS_ASKED.start(),"maxItems":CLOZE_BLANKS_ASKED.end(),"items":{"type":"object","additionalProperties":false,"required":["acceptedAnswers"],"properties":{"acceptedAnswers":strings(&(1..=8))}}}),
        ),
        LearningItemFormat::Matching => {
            put(
                "pairs",
                json!({"type":"array","minItems":MATCHING_PAIRS.start(),"maxItems":MATCHING_PAIRS.end(),"items":{"type":"object","additionalProperties":false,"required":["left","right"],"properties":{"left":{"type":"string","minLength":1},"right":{"type":"string","minLength":1}}}}),
            );
            put("extraRight", json!({"type":"string"}));
        }
        LearningItemFormat::Ordering => put("orderedValues", strings(&ORDERING_ASKED)),
        LearningItemFormat::Explanation | LearningItemFormat::Artifact => {
            put("rubric", crate::teaching::rubric_schema())
        }
    }
    put("explanation", json!({"type":"string","minLength":1}));
    put(
        "blockIndex",
        json!({"type":"integer","minimum":0,"maximum":passages.saturating_sub(1)}),
    );
    put(
        "quote",
        json!({"type":"string","minLength":MIN_QUOTE_CHARS}),
    );
    put(
        "difficulty",
        json!({"type":"integer","minimum":min_d,"maximum":max_d}),
    );
    let required: Vec<String> = properties.keys().cloned().collect();
    json!({"type":"object","additionalProperties":false,"required":required,"properties":properties})
}

/// An item the checks refused: what it said, and every fault, in words its rewrite is told.
pub(super) struct Unbuilt {
    pub(super) prompt: String,
    pub(super) options: Vec<String>,
    pub(super) faults: Vec<String>,
}

/// Turn one model item into a validated candidate, or say every fault it has.
pub(super) fn to_candidate(
    raw: RawItem,
    need: &AuthoringNeed,
    passages: &[&MaterialPassage],
) -> std::result::Result<AuthoredItem, Unbuilt> {
    let prompt = raw.prompt.trim().to_string();
    let said = raw.options.clone();
    let quote = raw.quote.trim().to_string();
    let refuse = |options: Vec<String>, faults: Vec<String>| Unbuilt {
        prompt: prompt.clone(),
        options,
        faults,
    };
    let Some(passage) = passages.get(raw.block_index) else {
        return Err(refuse(
            said,
            vec!["The item names a passage that was not supplied.".into()],
        ));
    };
    let mut faults = Vec::new();
    if validate_excerpt_quote(&quote, &passage.text).is_err() {
        faults.push(format!("The item's quote is not copied exactly from its passage, or is shorter than {MIN_QUOTE_CHARS} characters or {MIN_QUOTE_WORDS} words."));
    }
    let mut candidate = match candidate_of(raw, need, passage) {
        Ok(candidate) => candidate,
        Err(fault) => {
            faults.push(fault);
            return Err(refuse(said, faults));
        }
    };
    faults.extend(engine::checks::item_faults(&candidate));
    if let (true, Err(error)) = (faults.is_empty(), engine::validate_candidate(&candidate)) {
        faults.push(error.to_string());
    }
    if !faults.is_empty() {
        return Err(refuse(candidate.options, faults));
    }
    // Code, not the model, decides where the key sits.
    engine::shuffle_authored(&mut candidate);
    Ok(AuthoredItem {
        candidate: candidate.into(),
        source_quote: quote,
        lesson_id: need.lesson_id.clone(),
    })
}

/// The bank candidate a model item makes, unchecked; a fault when its rubric cannot be built.
fn candidate_of(
    raw: RawItem,
    need: &AuthoringNeed,
    passage: &MaterialPassage,
) -> std::result::Result<LearningAssessmentCandidate, String> {
    let format = need.requirement.format;
    let mut options = vec![];
    let mut left = vec![];
    let mut rationales = vec![];
    let mut rubric = vec![];
    let key = match format {
        LearningItemFormat::SingleChoice => {
            options = raw.options;
            rationales = raw.distractor_rationales;
            LearningAssessmentKey::Choice(raw.answer_index)
        }
        LearningItemFormat::TrueFalse => {
            options = engine::TRUE_FALSE.map(String::from).to_vec();
            LearningAssessmentKey::Choice(if raw.is_true { 0 } else { 1 })
        }
        LearningItemFormat::MultiSelect => {
            options = raw.options;
            rationales = raw.distractor_rationales;
            LearningAssessmentKey::Choices(raw.correct_indices.into_iter().collect::<BTreeSet<_>>())
        }
        LearningItemFormat::ShortAnswer => LearningAssessmentKey::Text(LearningTextKey {
            variants: raw.accepted_answers,
            numeric: raw.is_numeric.then(|| LearningNumericKey {
                value: raw.numeric_value,
                tolerance: raw.tolerance.max(0.0),
                unit_variants: raw.units,
            }),
        }),
        LearningItemFormat::Cloze => LearningAssessmentKey::Cloze(
            raw.blanks
                .into_iter()
                .map(|b| LearningTextKey {
                    variants: b.accepted_answers,
                    numeric: None,
                })
                .collect(),
        ),
        LearningItemFormat::Matching => {
            left = raw
                .pairs
                .iter()
                .map(|p| p.left.trim().to_string())
                .collect();
            options = raw
                .pairs
                .iter()
                .map(|p| p.right.trim().to_string())
                .collect();
            if !raw.extra_right.trim().is_empty() {
                options.push(raw.extra_right.trim().to_string());
            }
            LearningAssessmentKey::Matching((0..raw.pairs.len()).collect())
        }
        LearningItemFormat::Ordering => {
            options = raw.ordered_values.clone();
            LearningAssessmentKey::Ordering(raw.ordered_values)
        }
        LearningItemFormat::Explanation | LearningItemFormat::Artifact => {
            rubric = crate::teaching::rubric(raw.rubric, true)
                .map_err(|e| e.to_string())?
                .into_iter()
                .map(|c| LearningRubricCriterion {
                    id: c.id,
                    title: c.title,
                    description: c.description,
                    max_points: c.max_points as u32,
                })
                .collect();
            LearningAssessmentKey::Rubric
        }
    };
    let difficulty = raw.difficulty.clamp(
        need.requirement.difficulty_min,
        need.requirement.difficulty_max,
    );
    Ok(LearningAssessmentCandidate {
        id: uuid::Uuid::new_v4().to_string(),
        outcome_ids: vec![need.requirement.outcome_id.clone()],
        format,
        difficulty,
        prompt: raw.prompt.trim().to_string(),
        options,
        left,
        source_version_ids: passage
            .source_version_ids
            .iter()
            .filter(|id| uuid::Uuid::parse_str(id).is_ok())
            .take(MAX_SOURCE_VERSIONS)
            .cloned()
            .collect(),
        rubric,
        key,
        explanation: raw.explanation.trim().to_string(),
        distractor_rationales: rationales
            .into_iter()
            .map(|r| r.trim().to_string())
            .collect(),
    })
}
