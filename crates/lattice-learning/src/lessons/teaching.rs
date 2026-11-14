//! Instructional review and task-specific teaching contracts.
use super::copied_wording::{self, Wording};
use crate::dto::{
    LearningBlockKind, LearningPracticeCriterionDto, LearningPracticeEvidenceDimension,
};
use crate::generation::{MIN_QUOTE_CHARS, MIN_QUOTE_WORDS};
use crate::lessons::shape::LessonShape;
use crate::review_findings::{FindingKind, FindingSite, Remedy, ReviewFinding, Rule};
use lattice_core::shared::hash::sha256_hex;
use lattice_core::{
    application::ports::LLMPort,
    shared::error::{AppError, Result},
};
use serde::Deserialize;
use serde_json::{json, Value};

mod answers;
pub(crate) mod contract;
mod passes;
mod protected;
pub(crate) mod quotes;
mod repair;
mod review_loop;
mod standing;
mod subject;
mod waiting;
use passes::Passes;
use quotes::QuoteCheck;
use review_loop::{review_material, Reviewed};
pub(crate) use standing::{references_hold, versions};
use subject::{LessonSubject, MaterialSubject, OutlineSubject};
pub(crate) use waiting::waiting_part_texts;
pub(crate) use waiting::record as record_waiting;

pub fn project_schema() -> Value {
    let list = json!({"type":"array","minItems":1,"items":{"type":"string","minLength":1}});
    json!({"type":"object","additionalProperties":false,"required":["title","brief","deliverables","successCriteria"],"properties":{"title":{"type":"string","minLength":1},"brief":{"type":"string","minLength":1},"deliverables":list,"successCriteria":list}})
}

pub fn validate_project(project: &crate::dto::LearningProjectMilestoneDto) -> Result<()> {
    if project.title.trim().is_empty()
        || project.brief.trim().is_empty()
        || [&project.deliverables, &project.success_criteria]
            .iter()
            .any(|list| list.is_empty() || list.iter().any(|s| s.trim().is_empty()))
    {
        return Err(AppError::InvalidInput("Each project milestone needs a concrete brief, deliverables and observable success criteria.".into()));
    }
    Ok(())
}

#[derive(Debug, Deserialize)]
#[cfg_attr(
    any(test, feature = "test-support"),
    derive(serde::Serialize, PartialEq)
)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GeneratedCriterion {
    pub title: String,
    pub description: String,
    pub dimension: LearningPracticeEvidenceDimension,
}

pub fn rubric_schema() -> Value {
    json!({"type":"array","items":{"type":"object","additionalProperties":false,"required":["title","description","dimension"],"properties":{"title":{"type":"string","minLength":1},"description":{"type":"string","minLength":1},"dimension":{"enum":["recall","explanation","application","transfer"]}}}})
}

pub fn rubric(
    criteria: Vec<GeneratedCriterion>,
    needed: bool,
) -> Result<Vec<LearningPracticeCriterionDto>> {
    if needed && criteria.is_empty() {
        return Err(AppError::InvalidInput(
            "Practice needs task-specific criteria.".into(),
        ));
    }
    let mut titles = std::collections::HashSet::new();
    criteria
        .into_iter()
        .map(|c| {
            if c.title.trim().is_empty()
                || c.description.trim().is_empty()
                || !titles.insert(c.title.trim().to_lowercase())
            {
                return Err(AppError::InvalidInput(
                    "Teaching criteria must be distinct, observable and specific to the task."
                        .into(),
                ));
            }
            Ok(LearningPracticeCriterionDto {
                id: uuid::Uuid::new_v4().to_string(),
                title: c.title.trim().into(),
                description: c.description.trim().into(),
                dimension: c.dimension,
                max_points: 4,
            })
        })
        .collect()
}

/// Whether the criteria a section was written with are saved: the question
/// `rubric` itself answers, so a practice section waits whenever they are not.
fn rubric_is_kept(written: &Value) -> bool {
    match serde_json::from_value::<Vec<GeneratedCriterion>>(written.clone()) {
        Ok(criteria) => rubric(criteria, true).is_ok(),
        Err(_) => false,
    }
}

pub fn validate_saved_rubric(criteria: &[LearningPracticeCriterionDto]) -> Result<()> {
    let mut ids = std::collections::HashSet::new();
    if criteria.iter().any(|c| {
        uuid::Uuid::parse_str(&c.id).is_err()
            || !ids.insert(&c.id)
            || c.title.trim().is_empty()
            || c.description.trim().is_empty()
            || c.max_points < 1
    }) {
        return Err(AppError::InvalidInput(
            "Saved teaching criteria are invalid or outside their bounds.".into(),
        ));
    }
    Ok(())
}

fn json_or_text(raw: &str) -> Value {
    serde_json::from_str(raw).unwrap_or_else(|_| Value::String(raw.into()))
}

pub(crate) const LESSON_REPAIR_SYSTEM: &str = "Teach a rigorous lesson.";

pub const LESSON_REVIEW_EVIDENCE_SCOPE: &str = "The authoring reference excerpts are a partial selection, not the complete saved evidence collection. Whether each statement is true and supported, and whether each citation states its sentence, is checked separately against the current saved collection before publication: do not report it here, and do not report a detail as a defect because it is absent from these excerpts.";
/// What the reviewer of quiz material is told of the excerpts; no separate fact check stands behind it.
pub const MATERIAL_REVIEW_EVIDENCE_SCOPE: &str = "The authoring reference excerpts are a partial selection, not the complete saved evidence collection. Subsequent research and factual repairs can introduce details supported by other saved references. Do not report a teaching defect solely because a detail is absent from these original excerpts. Evidence completeness is checked separately, for every extracted factual claim, against the current saved collection before publication. Still reject concrete factual or mathematical errors, internal contradictions, and citations that misrepresent their attached source. Missing evidence is not proof that a claim is false, and this teaching review is not factual publication approval.";

/// What the code rules of a lesson pass decide, in the words the evidence gate is given.
pub(crate) const RULES_DECIDE: &str = "repeated sentences";

/// Deterministic findings: the schema, each section's body and rubric,
/// and, under a lesson's schema, the lesson's own rules. Nothing is
/// dropped; a caller that sends six at a time decides what waits. `wording` is
/// the source wording a lesson may not copy outside quotation marks.
pub(crate) fn structural_findings(
    schema: ReviewSchema<'_>,
    candidate: &str,
    wording: &Wording,
) -> Result<Vec<ReviewFinding>> {
    let value: Value = match serde_json::from_str(candidate) {
        Ok(value) => value,
        Err(_) => {
            return Ok(vec![ReviewFinding::rule(
                Rule::Unreadable,
                FindingKind::Structure,
                FindingSite::Lesson,
                "The material could not be read as a whole. Supply the complete material.",
            )
            .with_summary("The lesson could not be read as a whole.")])
        }
    };
    let validator = jsonschema::JSONSchema::compile(schema.value())
        .map_err(|_| AppError::InternalError("Invalid teaching schema".into()))?;
    let mut issues = Vec::new();
    if let Err(errors) = validator.validate(&value) {
        issues.extend(errors.map(|error| {
            let (at, rule_at) = (error.instance_path.to_string(), error.schema_path.to_string());
            let rule = plain_rule(schema.value().pointer(&rule_at));
            ReviewFinding::rule_about(
                Rule::Schema,
                FindingKind::Structure,
                FindingSite::of_pointer(&at),
                &format!("{at} {rule_at}"),
                format!("Fix the schema violation at {at}: the value there does not meet {rule_at}{rule}."),
            )
        }));
    }
    if let Some(blocks) = value.get("blocks").and_then(Value::as_array) {
        for (index, block) in blocks.iter().enumerate() {
            let site = FindingSite::Section(index);
            let kind = block["kind"].as_str().unwrap_or_default();
            if block["body"].as_str().unwrap_or_default().trim().is_empty() {
                issues.push(
                    ReviewFinding::rule(
                        Rule::BlockBody,
                        FindingKind::Structure,
                        site,
                        format!("{} has no body.", site.note_label()),
                    )
                    .with_summary(format!("{} has no text.", site.label())),
                );
            }
            if LearningBlockKind::parse(kind).is_some_and(|kind| kind.sets_practice())
                && !rubric_is_kept(&block["rubric"])
            {
                issues.push(
                    ReviewFinding::rule(
                        Rule::PracticeRubric,
                        FindingKind::Structure,
                        site,
                        format!(
                            "{} needs observable, task-specific rubric criteria.",
                            site.note_label()
                        ),
                    )
                    .with_summary(format!(
                        "{} is a practice section and needs clear criteria for judging an answer.",
                        site.label()
                    )),
                );
            }
        }
    }
    if let ReviewSchema::Lesson(shape) = schema {
        issues.extend(copied_wording::findings(&value, wording));
        issues.extend(shape.policy.rules(&value, shape.phase()));
    }
    Ok(issues)
}

/// A schema rule as a note quotes it: a scalar, or a list of scalars. A rule
/// that holds more is left to the schema path.
fn plain_rule(rule: Option<&Value>) -> String {
    let scalar = |value: &Value| !matches!(value, Value::Array(_) | Value::Object(_));
    match rule {
        Some(rule)
            if scalar(rule)
                || rule
                    .as_array()
                    .is_some_and(|items| items.iter().all(scalar)) =>
        {
            format!(" ({rule})")
        }
        _ => String::new(),
    }
}

/// What material is reviewed against. Only a lesson's schema, with the policy
/// that built it, brings the lesson's own rules, in the schema's phase.
#[derive(Clone, Copy)]
pub(crate) enum ReviewSchema<'a> {
    Lesson(LessonShape<'a>),
    Other(&'a Value),
}

impl<'a> From<LessonShape<'a>> for ReviewSchema<'a> {
    fn from(shape: LessonShape<'a>) -> Self {
        Self::Lesson(shape)
    }
}

impl<'a> ReviewSchema<'a> {
    pub(crate) fn value(&self) -> &'a Value {
        match self {
            Self::Lesson(shape) => shape.value(),
            Self::Other(schema) => schema,
        }
    }
}

/// Findings for a source quotation that matches no saved reference, placed
/// at the part that cites it.
pub(crate) fn quotation_findings(checks: &[QuoteCheck]) -> Vec<ReviewFinding> {
    checks
        .iter()
        .map(|check| {
            let site = FindingSite::of_part(&check.path).unwrap_or(FindingSite::Lesson);
            ReviewFinding::rule_about(
                Rule::Quotation,
                FindingKind::Quotation,
                site,
                &check.quote,
                format!(
                    "The source quotation at {} does not match its saved reference: {} Copy a relevant exact span from the cited source, including its punctuation and formatting; quotes contain at least {MIN_QUOTE_CHARS} characters and {MIN_QUOTE_WORDS} words.",
                    check.path, check.error
                ),
            )
            .with_summary(format!(
                "The quotation cited in {} could not be found in its source.",
                site.plain()
            ))
            .with_passage(&check.quote)
        })
        .collect()
}

/// The structure findings only a rewrite of the whole material can answer.
fn whole_repairs(structure: &[ReviewFinding]) -> impl Iterator<Item = &ReviewFinding> {
    structure
        .iter()
        .filter(|finding| finding.remedy() == Remedy::Whole)
}

/// Whether a rewrite of the whole material still has structure to answer, which
/// holds up the content review.
fn holds_up_review(structure: &[ReviewFinding], passes: Option<&Passes>) -> bool {
    whole_repairs(structure).any(|finding| passes.is_none_or(|passes| passes.repairable(finding)))
}

/// A second model call critiques the actual candidate. One targeted repair is
/// allowed; unresolved issues fail closed without publishing partial material.
/// A lesson (`review_lesson`) instead repairs while each pass fixes something
/// and publishes with what is left waiting for the user (`passes`).
/// This is an AI review, not a claim of independent expert verification.
/// For material that is not a lesson (`review_and_repair_lesson` is the lesson's).
pub async fn review_and_repair(
    llm: &dyn LLMPort,
    system: &str,
    prompt: &str,
    schema: &Value,
    candidate: String,
    output_tokens: usize,
) -> Result<String> {
    review_and_repair_under(
        llm,
        system,
        prompt,
        ReviewSchema::Other(schema),
        candidate,
        output_tokens,
    )
    .await
}

/// `review_and_repair` for a lesson written to the writer's schema for
/// `source_count` sources: the lesson's rules apply to it.
pub async fn review_and_repair_lesson(
    llm: &dyn LLMPort,
    system: &str,
    prompt: &str,
    source_count: usize,
    candidate: String,
    output_tokens: usize,
) -> Result<String> {
    let policy = crate::lessons::shape::policy();
    let schema = policy.schema(crate::lessons::shape::Phase::Draft, source_count);
    review_and_repair_under(
        llm,
        system,
        prompt,
        ReviewSchema::Lesson(LessonShape::new(&schema, policy)),
        candidate,
        output_tokens,
    )
    .await
}

async fn review_and_repair_under(
    llm: &dyn LLMPort,
    system: &str,
    prompt: &str,
    schema: ReviewSchema<'_>,
    candidate: String,
    output_tokens: usize,
) -> Result<String> {
    let context = json!({"policy":"lesson-teaching","model":llm.model_name(),"system":system,"prompt":prompt,"schema":schema.value()});
    let receipt = sha256_hex(context.to_string());
    if crate::lesson_drafts::teaching_completed(&candidate, &receipt).await? {
        crate::lesson_progress::phase(
            crate::lesson_progress::Phase::Review,
            "Reusing completed teaching and answer-key checks for the unchanged draft",
        );
        return Ok(candidate);
    }
    let candidate = review_material(
        llm,
        system,
        prompt,
        schema,
        candidate,
        output_tokens,
        &mut MaterialSubject::new(),
    )
    .await
    .map(|reviewed| reviewed.candidate)?;
    crate::lesson_drafts::record_teaching(&candidate, receipt).await?;
    Ok(candidate)
}

pub struct OutlineReviewContext<'a> {
    pub progress: &'a crate::outline_progress::OutlineProgress,
    pub sources: &'a [crate::dto::LearningSourceDto],
}

pub(crate) async fn review_lesson<'s>(
    llm: &dyn LLMPort,
    system: &str,
    prompt: &str,
    review_schema: impl Into<ReviewSchema<'s>>,
    candidate: String,
    output_tokens: usize,
    (references, numbered): (
        &crate::reference_collection::ReferenceCollection<'_>,
        &[crate::dto::LearningSourceDto],
    ),
    protected: &[crate::lessons::content_verification::Protected],
) -> Result<String> {
    let review_schema = review_schema.into();
    let schema = review_schema.value();
    let sources = versions(&references.sources);
    // Teaching structure is bound to the candidate and authoring inputs. New
    // references are evaluated by the factual gate; they do not invalidate an
    // already completed pedagogical review. Changed/removed snapshots still do.
    let review_input = json!({"policy":"lesson-teaching-review","findingPolicy":crate::review_evidence::POLICY,"sectionReviewPolicy":crate::teaching_review::POLICY,"model":llm.model_name(),"system":system,"prompt":prompt,"schema":schema});
    let source_key = |candidate: &str| {
        format!(
            "teaching-sources:{}",
            sha256_hex(json!({"input":review_input,"candidate":candidate}).to_string())
        )
    };
    let unchanged_review = crate::lesson_drafts::checkpoint(&source_key(&candidate))
        .await?
        .and_then(|value| serde_json::from_value::<Vec<(String, String)>>(value).ok())
        .is_some_and(|saved| standing::references_hold(&saved, &sources));
    if unchanged_review {
        crate::lesson_progress::phase(
            crate::lesson_progress::Phase::Review,
            "Reusing completed teaching and answer-key checks for the unchanged draft",
        );
        return Ok(candidate);
    }
    let held = waiting::held(&json_or_text(&candidate)).await?;
    let Reviewed {
        candidate,
        set_aside,
    } = review_material(
        llm,
        system,
        prompt,
        review_schema,
        candidate,
        output_tokens,
        &mut LessonSubject::new(references, numbered, protected, held),
    )
    .await?;
    waiting::settle(&json_or_text(&candidate), &set_aside).await?;
    crate::lesson_drafts::record_checkpoint(&source_key(&candidate), json!(sources)).await?;
    Ok(candidate)
}

pub async fn review_and_repair_with_progress(
    llm: &dyn LLMPort,
    system: &str,
    prompt: &str,
    schema: &Value,
    candidate: String,
    output_tokens: usize,
    outline: OutlineReviewContext<'_>,
) -> Result<String> {
    review_material(
        llm,
        system,
        prompt,
        ReviewSchema::Other(schema),
        candidate,
        output_tokens,
        &mut OutlineSubject::new(outline)?,
    )
    .await
    .map(|reviewed| reviewed.candidate)
}

#[cfg(test)]
mod answers_tests;
#[cfg(test)]
mod citation_tests;
#[cfg(test)]
mod contract_tests;
#[cfg(test)]
mod gate_recorded_tests;
#[cfg(test)]
mod loop_tests;
#[cfg(test)]
mod memory_tests;
#[cfg(test)]
mod open_findings_tests;
#[cfg(test)]
mod question_tests;
#[cfg(test)]
mod standing_tests;

#[cfg(test)]
mod tests;
