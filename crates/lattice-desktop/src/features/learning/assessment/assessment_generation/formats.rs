//! Per-format item schemas, and turning a model's item into a checked bank
//! item (section 5.2).
use super::*;

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
    rubric: Vec<crate::features::learning::teaching::GeneratedCriterion>,
    explanation: String,
    block_index: usize,
    quote: String,
    difficulty: u8,
}

fn strings(min: usize, max: usize) -> Value {
    json!({"type":"array","minItems":min,"maxItems":max,"items":{"type":"string","minLength":1,"maxLength":500}})
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
    put(
        "prompt",
        json!({"type":"string","minLength":8,"maxLength":2000}),
    );
    match format {
        LearningItemFormat::SingleChoice => {
            put("options", strings(4, 4));
            put("distractorRationales", strings(4, 4));
            put(
                "answerIndex",
                json!({"type":"integer","minimum":0,"maximum":3}),
            );
        }
        LearningItemFormat::TrueFalse => put("isTrue", json!({"type":"boolean"})),
        LearningItemFormat::MultiSelect => {
            put("options", strings(4, 6));
            put("distractorRationales", strings(4, 6));
            put(
                "correctIndices",
                json!({"type":"array","minItems":2,"maxItems":3,"uniqueItems":true,"items":{"type":"integer","minimum":0,"maximum":5}}),
            );
        }
        LearningItemFormat::ShortAnswer => {
            put("acceptedAnswers", strings(3, 12));
            put("isNumeric", json!({"type":"boolean"}));
            put("numericValue", json!({"type":"number"}));
            put("tolerance", json!({"type":"number","minimum":0}));
            put(
                "units",
                json!({"type":"array","maxItems":8,"items":{"type":"string","maxLength":40}}),
            );
        }
        LearningItemFormat::Cloze => put(
            "blanks",
            json!({"type":"array","minItems":1,"maxItems":4,"items":{"type":"object","additionalProperties":false,"required":["acceptedAnswers"],"properties":{"acceptedAnswers":strings(1, 8)}}}),
        ),
        LearningItemFormat::Matching => {
            put(
                "pairs",
                json!({"type":"array","minItems":3,"maxItems":5,"items":{"type":"object","additionalProperties":false,"required":["left","right"],"properties":{"left":{"type":"string","minLength":1,"maxLength":300},"right":{"type":"string","minLength":1,"maxLength":300}}}}),
            );
            put("extraRight", json!({"type":"string","maxLength":300}));
        }
        LearningItemFormat::Ordering => put("orderedValues", strings(3, 8)),
        LearningItemFormat::Explanation | LearningItemFormat::Artifact => put(
            "rubric",
            crate::features::learning::teaching::rubric_schema(),
        ),
    }
    put(
        "explanation",
        json!({"type":"string","minLength":8,"maxLength":1200}),
    );
    put(
        "blockIndex",
        json!({"type":"integer","minimum":0,"maximum":passages.saturating_sub(1)}),
    );
    put(
        "quote",
        json!({"type":"string","minLength":12,"maxLength":700}),
    );
    put(
        "difficulty",
        json!({"type":"integer","minimum":min_d,"maximum":max_d}),
    );
    let required: Vec<String> = properties.keys().cloned().collect();
    json!({"type":"object","additionalProperties":false,"required":required,"properties":properties})
}

const ABSOLUTES: [&str; 8] = [
    "always", "never", "all", "none", "every", "only", "cannot", "must",
];

fn spaced_lower(value: &str) -> String {
    value
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

/// Turn one model item into a validated candidate, or say why it is refused.
pub(super) fn to_candidate(
    raw: RawItem,
    need: &AuthoringNeed,
    passages: &[&MaterialPassage],
) -> std::result::Result<AuthoredItem, String> {
    let format = need.requirement.format;
    let Some(passage) = passages.get(raw.block_index) else {
        return Err("The item names a passage that was not supplied.".into());
    };
    let quote = raw.quote.trim().to_string();
    if quote.chars().count() < 12 || !spaced_lower(&passage.text).contains(&spaced_lower(&quote)) {
        return Err("The item's quote is not copied exactly from its passage.".into());
    }
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
            let statement = engine::normalize_answer(&raw.prompt);
            let source = engine::normalize_answer(&passage.text);
            if let Some(word) = ABSOLUTES.iter().find(|w| {
                format!(" {statement} ").contains(&format!(" {w} "))
                    && !format!(" {source} ").contains(&format!(" {w} "))
            }) {
                return Err(format!(
                    "The statement says “{word}”, which its passage does not."
                ));
            }
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
            rubric = crate::features::learning::teaching::rubric(raw.rubric, true)
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
    let mut candidate = LearningAssessmentCandidate {
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
            .take(24)
            .cloned()
            .collect(),
        rubric,
        key,
        explanation: raw.explanation.trim().to_string(),
        distractor_rationales: rationales
            .into_iter()
            .map(|r| r.trim().chars().take(500).collect())
            .collect(),
    };
    engine::validate_candidate(&candidate).map_err(|e| e.to_string())?;
    if need
        .existing_prompts
        .iter()
        .any(|p| engine::near_duplicate(p, &candidate.prompt))
    {
        return Err("The item repeats a question already in the bank.".into());
    }
    // Code, not the model, decides where the key sits.
    engine::shuffle_authored(&mut candidate);
    Ok(AuthoredItem {
        candidate: candidate.into(),
        source_quote: quote,
        lesson_id: need.lesson_id.clone(),
    })
}
