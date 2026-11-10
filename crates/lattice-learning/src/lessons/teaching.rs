//! Instructional review and task-specific teaching contracts.
use super::copied_wording::{self, Wording};
use crate::dto::{LearningPracticeCriterionDto, LearningPracticeEvidenceDimension};
use crate::generation::{MIN_QUOTE_CHARS, MIN_QUOTE_WORDS};
use crate::lessons::shape::LessonShape;
use crate::review_findings::{plain_part, FindingKind, FindingSite, ReviewFinding, Rule};
use lattice_core::{
    application::ports::LLMPort,
    shared::error::{AppError, Result},
};
use serde::Deserialize;
use serde_json::{json, Value};

pub(crate) mod contract;
mod passes;
mod protected;
mod quotes;
mod repair;
mod review_loop;
mod subject;
mod waiting;
use passes::Passes;
use review_loop::{review_material, Reviewed};
use subject::{LessonSubject, MaterialSubject, OutlineSubject};
pub(crate) use waiting::record as record_waiting;

pub fn project_schema() -> Value {
    let list =
        json!({"type":"array","minItems":2,"maxItems":5,"items":{"type":"string","minLength":1}});
    json!({"type":"object","additionalProperties":false,"required":["title","brief","deliverables","successCriteria"],"properties":{"title":{"type":"string","minLength":1},"brief":{"type":"string","minLength":1},"deliverables":list,"successCriteria":list}})
}

pub fn validate_project(project: &crate::dto::LearningProjectMilestoneDto) -> Result<()> {
    if project.title.trim().is_empty()
        || project.brief.trim().is_empty()
        || [&project.deliverables, &project.success_criteria]
            .iter()
            .any(|list| !(2..=5).contains(&list.len()) || list.iter().any(|s| s.trim().is_empty()))
    {
        return Err(AppError::InvalidInput("Each project milestone needs a concrete brief, deliverables and observable success criteria.".into()));
    }
    Ok(())
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GeneratedCriterion {
    pub title: String,
    pub description: String,
    pub dimension: LearningPracticeEvidenceDimension,
}

pub fn rubric_schema() -> Value {
    json!({"type":"array","maxItems":4,"items":{"type":"object","additionalProperties":false,"required":["title","description","dimension"],"properties":{"title":{"type":"string","minLength":1},"description":{"type":"string","minLength":1},"dimension":{"enum":["recall","explanation","application","transfer"]}}}})
}

pub fn rubric(
    criteria: Vec<GeneratedCriterion>,
    required: bool,
) -> Result<Vec<LearningPracticeCriterionDto>> {
    if criteria.len() > 4 || (required && criteria.len() < 2) {
        return Err(AppError::InvalidInput(
            "Practice needs two to four task-specific criteria.".into(),
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

pub fn validate_saved_rubric(criteria: &[LearningPracticeCriterionDto]) -> Result<()> {
    let mut ids = std::collections::HashSet::new();
    if criteria.len() > 4
        || criteria.iter().any(|c| {
            uuid::Uuid::parse_str(&c.id).is_err()
                || !ids.insert(&c.id)
                || c.title.trim().is_empty()
                || c.description.trim().is_empty()
                || !(1..=100).contains(&c.max_points)
        })
    {
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

pub const LESSON_REVIEW_EVIDENCE_SCOPE: &str = "The authoring reference excerpts are a partial selection, not the complete saved evidence collection. Subsequent research and factual repairs can introduce details supported by other saved references. Do not report a teaching defect solely because a detail is absent from these original excerpts. Evidence completeness is checked separately, for every extracted factual claim, against the current saved collection before publication. Still reject concrete factual or mathematical errors, internal contradictions, and citations that misrepresent their attached source. Missing evidence is not proof that a claim is false, and this teaching review is not factual publication approval.";

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
            let kind = block["kind"].as_str().unwrap_or_default();
            if block["body"].as_str().unwrap_or_default().trim().is_empty() {
                issues.push(
                    ReviewFinding::rule(
                        Rule::BlockBody,
                        FindingKind::Structure,
                        FindingSite::Section(index),
                        format!("Teaching block {} has no body.", index + 1),
                    )
                    .with_summary(format!("Section {} has no text.", index + 1)),
                );
            }
            if ["guided_practice", "independent_practice"].contains(&kind)
                && block["rubric"].as_array().is_none_or(|r| r.len() < 2)
            {
                issues.push(
                    ReviewFinding::rule(Rule::PracticeRubric, FindingKind::Structure, FindingSite::Section(index), format!("Practice block {} needs two to four observable, task-specific rubric criteria.", index + 1))
                        .with_summary(format!("Section {} is a practice section and needs two to four clear criteria for judging an answer.", index + 1)),
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
fn quotation_findings(quote_checks: &[Value]) -> Vec<ReviewFinding> {
    quote_checks
        .iter()
        .map(|check| {
            let path = check["path"].as_str().unwrap_or_default();
            let site = FindingSite::of_part(path).unwrap_or(FindingSite::Lesson);
            let quote = check["quote"].as_str().unwrap_or_default();
            ReviewFinding::rule_about(
                Rule::Quotation,
                FindingKind::Quotation,
                site,
                quote,
                format!(
                    "The source quotation at {path} does not match its saved reference: {} Copy a relevant exact span from the cited source, including its punctuation and formatting; quotes contain at least {MIN_QUOTE_CHARS} characters and {MIN_QUOTE_WORDS} words.",
                    check["error"].as_str().unwrap_or("it was not found.")
                ),
            )
            .with_summary(format!(
                "The quotation cited in {} could not be found in its source.",
                plain_part(site)
            ))
            .with_passage(quote)
        })
        .collect()
}

/// The structure findings a repair of the whole material answers: all but a
/// missing citation, which the edits of the repair after the review can
/// append (claim classes design §3.5).
fn whole_repairs(structure: &[ReviewFinding]) -> impl Iterator<Item = &ReviewFinding> {
    structure
        .iter()
        .filter(|finding| finding.kind != FindingKind::Quotation)
}

/// Whether the structure findings hold up the content review. Structure the
/// machine stopped repairing no longer does.
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
    let receipt = crate::content_verification::digest(&context.to_string());
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
    references: &crate::reference_collection::ReferenceCollection<'_>,
    protected: &[crate::lessons::content_verification::Protected],
) -> Result<String> {
    let review_schema = review_schema.into();
    let schema = review_schema.value();
    let mut sources: Vec<_> = references
        .sources
        .iter()
        .map(|source| {
            (
                &source.id,
                crate::content_verification::digest(&source.excerpt),
            )
        })
        .collect();
    sources.sort();
    // Teaching structure is bound to the candidate and authoring inputs. New
    // references are evaluated by the factual gate; they do not invalidate an
    // already completed pedagogical review. Changed/removed snapshots still do.
    let review_input = json!({"policy":"lesson-teaching-review","findingPolicy":crate::review_evidence::POLICY,"sectionReviewPolicy":crate::teaching_review::POLICY,"model":llm.model_name(),"system":system,"prompt":prompt,"schema":schema});
    let source_key = |candidate: &str| {
        format!(
            "teaching-sources:{}",
            crate::content_verification::digest(
                &json!({"input":review_input,"candidate":candidate}).to_string()
            )
        )
    };
    let unchanged_review = crate::lesson_drafts::checkpoint(&source_key(&candidate))
        .await?
        .and_then(|value| serde_json::from_value::<Vec<(String, String)>>(value).ok())
        .is_some_and(|saved| {
            saved.iter().all(|(id, hash)| {
                sources
                    .iter()
                    .any(|(current_id, current_hash)| *current_id == id && current_hash == hash)
            })
        });
    let receipt = crate::content_verification::digest(&json!({"policy":"lesson-teaching-review","findingPolicy":crate::review_evidence::POLICY,"sectionReviewPolicy":crate::teaching_review::POLICY,"model":llm.model_name(),"system":system,"prompt":prompt,"schema":schema,"reviewReferenceHashes":sources}).to_string());
    if unchanged_review || crate::lesson_drafts::teaching_completed(&candidate, &receipt).await? {
        if !unchanged_review {
            crate::lesson_drafts::record_checkpoint(&source_key(&candidate), json!(sources))
                .await?;
        }
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
        &mut LessonSubject::new(references, protected, held),
    )
    .await?;
    waiting::settle(&json_or_text(&candidate), &set_aside).await?;
    crate::lesson_drafts::record_teaching(&candidate, receipt).await?;
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
mod citation_tests;
#[cfg(test)]
mod contract_tests;
#[cfg(test)]
mod loop_tests;
#[cfg(test)]
mod question_tests;

#[cfg(test)]
mod tests;
