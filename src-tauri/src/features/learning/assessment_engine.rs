//! Subject-neutral assessment blueprint and immutable-form rules.
//!
//! This is the deterministic boundary around model-authored item banks. It
//! selects forms by accepted outcome coverage, prefers unseen items, labels any
//! exposed repeat, and grades only formats with application-owned keys. Open
//! responses enter a separate provisional rubric path.

use crate::shared::error::{AppError, Result};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, specta::Type, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum LearningItemFormat {
    MultipleChoice,
    ShortAnswer,
    Explanation,
    Ordering,
    Artifact,
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

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LearningAssessmentBlueprint {
    pub id: String,
    pub purpose: LearningAssessmentPurpose,
    pub title: String,
    pub instructions: String,
    pub expected_minutes: u32,
    pub allowed_aids: Vec<String>,
    pub pass_points: u32,
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LearningAssessmentCandidate {
    pub id: String,
    pub outcome_ids: Vec<String>,
    pub format: LearningItemFormat,
    pub difficulty: u8,
    pub prompt: String,
    pub options: Vec<String>,
    pub source_version_ids: Vec<String>,
    pub rubric: Vec<LearningRubricCriterion>,
    /// Internal application-owned key. This structure never crosses IPC.
    pub key: LearningAssessmentKey,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case", tag = "kind", content = "value")]
pub enum LearningAssessmentKey {
    Choice(usize),
    TextVariants(Vec<String>),
    Ordering(Vec<String>),
    Rubric,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LearningAssessmentFormItem {
    pub id: String,
    pub outcome_ids: Vec<String>,
    pub format: LearningItemFormat,
    pub prompt: String,
    pub options: Vec<String>,
    pub source_version_ids: Vec<String>,
    pub rubric: Vec<LearningRubricCriterion>,
    pub previously_exposed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LearningAssessmentForm {
    pub id: String,
    pub blueprint_id: String,
    pub purpose: LearningAssessmentPurpose,
    pub title: String,
    pub instructions: String,
    pub expected_minutes: u32,
    pub allowed_aids: Vec<String>,
    pub pass_points: u32,
    pub feedback_timing: LearningFeedbackTiming,
    pub items: Vec<LearningAssessmentFormItem>,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LearningAssessmentResponse {
    pub item_id: String,
    pub selected_index: Option<usize>,
    pub text: Option<String>,
    pub ordered_values: Vec<String>,
    pub artifact_json: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LearningDeterministicItemResult {
    pub item_id: String,
    pub correct: bool,
    pub points: u32,
    pub max_points: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LearningProvisionalCriterionResult {
    pub criterion_id: String,
    pub score: Option<u32>,
    pub max_points: u32,
    pub observation: String,
    pub artifact_quote: Option<String>,
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

pub fn validate_blueprint(blueprint: &LearningAssessmentBlueprint) -> Result<()> {
    id(&blueprint.id, "assessment blueprint")?;
    bounded(&blueprint.title, "Assessment title", 160)?;
    bounded(&blueprint.instructions, "Assessment instructions", 4_000)?;
    if !(1..=480).contains(&blueprint.expected_minutes) {
        return Err(AppError::InvalidInput(
            "Assessment time must be between 1 and 480 minutes.".into(),
        ));
    }
    if blueprint.allowed_aids.len() > 24
        || blueprint
            .allowed_aids
            .iter()
            .any(|aid| aid.trim().is_empty() || aid.chars().count() > 160)
    {
        return Err(AppError::InvalidInput(
            "Assessment aid descriptions must be bounded.".into(),
        ));
    }
    if blueprint.requirements.is_empty() || blueprint.requirements.len() > 32 {
        return Err(AppError::InvalidInput(
            "An assessment blueprint needs 1–32 coverage requirements.".into(),
        ));
    }
    let mut total = 0usize;
    let mut seen = HashSet::new();
    for requirement in &blueprint.requirements {
        id(&requirement.outcome_id, "outcome")?;
        if requirement.count == 0 || requirement.count > 12 {
            return Err(AppError::InvalidInput(
                "Each assessment coverage requirement needs 1–12 items.".into(),
            ));
        }
        if requirement.difficulty_min > requirement.difficulty_max
            || !(1..=5).contains(&requirement.difficulty_min)
            || !(1..=5).contains(&requirement.difficulty_max)
        {
            return Err(AppError::InvalidInput(
                "Assessment difficulty must use the bounded 1–5 scale.".into(),
            ));
        }
        if !seen.insert((requirement.outcome_id.as_str(), requirement.format)) {
            return Err(AppError::InvalidInput(
                "Assessment coverage requirements must be unique by outcome and format.".into(),
            ));
        }
        total += requirement.count;
    }
    if total > 64 {
        return Err(AppError::InvalidInput(
            "An assessment form cannot require more than 64 items.".into(),
        ));
    }
    Ok(())
}

pub fn validate_candidate(candidate: &LearningAssessmentCandidate) -> Result<()> {
    id(&candidate.id, "assessment item")?;
    bounded(&candidate.prompt, "Assessment prompt", 8_000)?;
    if candidate.outcome_ids.is_empty() || candidate.outcome_ids.len() > 8 {
        return Err(AppError::InvalidInput(
            "An assessment item must address 1–8 outcomes.".into(),
        ));
    }
    let mut outcomes = HashSet::new();
    for outcome in &candidate.outcome_ids {
        id(outcome, "outcome")?;
        if !outcomes.insert(outcome) {
            return Err(AppError::InvalidInput(
                "An assessment item repeats an outcome.".into(),
            ));
        }
    }
    if !(1..=5).contains(&candidate.difficulty) {
        return Err(AppError::InvalidInput(
            "Assessment item difficulty must use the 1–5 scale.".into(),
        ));
    }
    if candidate.source_version_ids.len() > 24 {
        return Err(AppError::InvalidInput(
            "Assessment items support at most 24 frozen source versions.".into(),
        ));
    }
    for source in &candidate.source_version_ids {
        id(source, "source version")?;
    }
    match (&candidate.format, &candidate.key) {
        (LearningItemFormat::MultipleChoice, LearningAssessmentKey::Choice(correct)) => {
            if !(2..=8).contains(&candidate.options.len()) || *correct >= candidate.options.len() {
                return Err(AppError::InvalidInput(
                    "Multiple-choice items need 2–8 options and one valid key.".into(),
                ));
            }
            let normalized = candidate
                .options
                .iter()
                .map(|value| {
                    value
                        .split_whitespace()
                        .collect::<Vec<_>>()
                        .join(" ")
                        .to_lowercase()
                })
                .collect::<HashSet<_>>();
            if normalized.len() != candidate.options.len()
                || normalized.iter().any(|option| option.is_empty())
            {
                return Err(AppError::InvalidInput(
                    "Multiple-choice options must be unique and non-empty.".into(),
                ));
            }
        }
        (LearningItemFormat::ShortAnswer, LearningAssessmentKey::TextVariants(variants)) => {
            if variants.is_empty()
                || variants.len() > 12
                || variants.iter().any(|answer| answer.trim().is_empty())
            {
                return Err(AppError::InvalidInput(
                    "Short-answer items need bounded accepted variants.".into(),
                ));
            }
            if !candidate.options.is_empty() {
                return Err(AppError::InvalidInput(
                    "Short-answer items cannot expose answer choices.".into(),
                ));
            }
        }
        (LearningItemFormat::Ordering, LearningAssessmentKey::Ordering(order)) => {
            if !(2..=12).contains(&candidate.options.len())
                || order.len() != candidate.options.len()
                || order.iter().collect::<HashSet<_>>().len() != order.len()
                || candidate.options.iter().collect::<HashSet<_>>()
                    != order.iter().collect::<HashSet<_>>()
            {
                return Err(AppError::InvalidInput(
                    "Ordering items need one permutation of 2–12 unique values.".into(),
                ));
            }
        }
        (
            LearningItemFormat::Explanation | LearningItemFormat::Artifact,
            LearningAssessmentKey::Rubric,
        ) => {
            if candidate.rubric.is_empty() || candidate.rubric.len() > 12 {
                return Err(AppError::InvalidInput(
                    "Open assessment items need 1–12 visible rubric criteria.".into(),
                ));
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
                    return Err(AppError::InvalidInput(
                        "Rubric criterion points must be between 1 and 100.".into(),
                    ));
                }
                if !criteria.insert(&criterion.id) {
                    return Err(AppError::InvalidInput(
                        "Rubric criterion IDs must be unique.".into(),
                    ));
                }
            }
        }
        _ => {
            return Err(AppError::InvalidInput(
                "Assessment item format and answer key do not match.".into(),
            ))
        }
    }
    Ok(())
}

/// Select a deterministic immutable form. Candidate order never controls the
/// result: unseen items are sorted by difficulty distance and stable ID.
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
            return Err(AppError::InvalidInput(
                "Assessment item-bank IDs must be unique.".into(),
            ));
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
                candidate.format == requirement.format
                    && candidate.outcome_ids.contains(&requirement.outcome_id)
                    && (requirement.difficulty_min..=requirement.difficulty_max)
                        .contains(&candidate.difficulty)
                    && !used.contains(candidate.id.as_str())
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
            return Err(AppError::InvalidState(format!(
                "The item bank cannot satisfy the accepted coverage for outcome {}.",
                requirement.outcome_id
            )));
        }
        for candidate in eligible.into_iter().take(requirement.count) {
            used.insert(candidate.id.as_str());
            selected.push(LearningAssessmentFormItem {
                id: candidate.id.clone(),
                outcome_ids: candidate.outcome_ids.clone(),
                format: candidate.format,
                prompt: candidate.prompt.clone(),
                options: candidate.options.clone(),
                source_version_ids: candidate.source_version_ids.clone(),
                rubric: candidate.rubric.clone(),
                previously_exposed: exposed_item_ids.contains(&candidate.id),
            });
        }
    }
    Ok(LearningAssessmentForm {
        id: form_id,
        blueprint_id: blueprint.id.clone(),
        purpose: blueprint.purpose,
        title: blueprint.title.clone(),
        instructions: blueprint.instructions.clone(),
        expected_minutes: blueprint.expected_minutes,
        allowed_aids: blueprint.allowed_aids.clone(),
        pass_points: blueprint.pass_points,
        feedback_timing: blueprint.feedback_timing,
        items: selected,
        created_at,
    })
}

fn normalized_answer(value: &str) -> String {
    value
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

pub fn grade_deterministic_responses(
    candidates: &[LearningAssessmentCandidate],
    responses: &[LearningAssessmentResponse],
) -> Result<Vec<LearningDeterministicItemResult>> {
    let bank = candidates
        .iter()
        .map(|candidate| (candidate.id.as_str(), candidate))
        .collect::<HashMap<_, _>>();
    let mut seen = HashSet::new();
    let mut results = Vec::with_capacity(responses.len());
    for response in responses {
        if !seen.insert(response.item_id.as_str()) {
            return Err(AppError::InvalidInput(
                "An assessment response was submitted more than once.".into(),
            ));
        }
        let candidate = bank.get(response.item_id.as_str()).ok_or_else(|| {
            AppError::InvalidInput("An assessment response references an unknown item.".into())
        })?;
        let correct = match &candidate.key {
            LearningAssessmentKey::Choice(index) => response.selected_index == Some(*index),
            LearningAssessmentKey::TextVariants(variants) => response
                .text
                .as_deref()
                .map(normalized_answer)
                .is_some_and(|answer| {
                    variants
                        .iter()
                        .map(|value| normalized_answer(value))
                        .any(|expected| expected == answer)
                }),
            LearningAssessmentKey::Ordering(order) => response.ordered_values == *order,
            LearningAssessmentKey::Rubric => {
                return Err(AppError::InvalidInput(
                    "Open responses require provisional rubric evaluation.".into(),
                ))
            }
        };
        results.push(LearningDeterministicItemResult {
            item_id: response.item_id.clone(),
            correct,
            points: u32::from(correct),
            max_points: 1,
        });
    }
    Ok(results)
}

pub fn validate_provisional_rubric_results(
    rubric: &[LearningRubricCriterion],
    artifact_text: &str,
    results: &[LearningProvisionalCriterionResult],
) -> Result<()> {
    if results.len() != rubric.len() {
        return Err(AppError::InvalidInput(
            "Provisional feedback must address every rubric criterion exactly once.".into(),
        ));
    }
    let expected = rubric
        .iter()
        .map(|criterion| (criterion.id.as_str(), criterion))
        .collect::<BTreeMap<_, _>>();
    let mut seen = HashSet::new();
    for result in results {
        let criterion = expected.get(result.criterion_id.as_str()).ok_or_else(|| {
            AppError::InvalidInput("Feedback references an unknown rubric criterion.".into())
        })?;
        if !seen.insert(result.criterion_id.as_str()) {
            return Err(AppError::InvalidInput(
                "Feedback repeats a rubric criterion.".into(),
            ));
        }
        if result.max_points != criterion.max_points
            || result
                .score
                .is_some_and(|score| score > criterion.max_points)
        {
            return Err(AppError::InvalidInput(
                "Feedback changed the accepted rubric scale.".into(),
            ));
        }
        bounded(&result.observation, "Criterion observation", 2_000)?;
        if let Some(quote) = result.artifact_quote.as_deref() {
            bounded(quote, "Artifact evidence quote", 800)?;
            if !artifact_text.contains(quote) {
                return Err(AppError::InvalidInput(
                    "Criterion feedback cites text that is absent from the submitted artifact."
                        .into(),
                ));
            }
        }
    }
    Ok(())
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
            pass_points: 2,
            feedback_timing: LearningFeedbackTiming::AfterSubmission,
            requirements: vec![LearningBlueprintRequirement {
                outcome_id: id(2),
                format: LearningItemFormat::MultipleChoice,
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
            format: LearningItemFormat::MultipleChoice,
            difficulty,
            prompt: format!("Which invariant holds in scenario {seed}?"),
            options: vec!["A".into(), "B".into(), "C".into()],
            source_version_ids: vec![id(20)],
            rubric: vec![],
            key: LearningAssessmentKey::Choice(1),
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
    fn deterministic_keys_grade_without_reaching_the_renderer() -> Result<()> {
        let candidate = candidate(10, 3);
        let candidate_id = candidate.id.clone();
        let results = grade_deterministic_responses(
            std::slice::from_ref(&candidate),
            &[LearningAssessmentResponse {
                item_id: candidate_id,
                selected_index: Some(1),
                text: None,
                ordered_values: vec![],
                artifact_json: None,
            }],
        )?;
        assert!(results[0].correct);
        Ok(())
    }

    #[test]
    fn provisional_feedback_must_use_the_visible_rubric_and_real_artifact_quotes() {
        let criterion = LearningRubricCriterion {
            id: id(50),
            title: "Explains the invariant".into(),
            description: "Names the invariant and relates it to the observed behavior.".into(),
            max_points: 4,
        };
        let artifact = "The queue retains ownership until the acknowledgement commits.";
        let result = LearningProvisionalCriterionResult {
            criterion_id: criterion.id.clone(),
            score: Some(3),
            max_points: 4,
            observation: "The invariant is named and connected to the acknowledgement.".into(),
            artifact_quote: Some("retains ownership until the acknowledgement commits".into()),
        };
        assert!(validate_provisional_rubric_results(
            std::slice::from_ref(&criterion),
            artifact,
            std::slice::from_ref(&result)
        )
        .is_ok());
        let mut invented = result;
        invented.artifact_quote = Some("a quote the learner never wrote".into());
        assert!(validate_provisional_rubric_results(&[criterion], artifact, &[invented]).is_err());
    }
}
