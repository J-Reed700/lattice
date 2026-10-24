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

mod answers;
mod checks;
mod scoring;
#[cfg(test)]
mod tests;

pub use answers::{normalize_answer, parse_number, show_number, text_matches};
pub use checks::{
    cloze_blanks, key_leaks, length_biased, near_duplicate, validate_candidate, TRUE_FALSE,
};
pub use scoring::{
    answer_hint, grade_keyed, key_reveal, part_results, KeyReveal, KeyedGrade, KeyedOutcome,
    PartResult, NO_ANSWER,
};
