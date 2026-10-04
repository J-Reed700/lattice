//! Strict, bounded source-grounded assessment candidate authoring. Answer keys
//! remain internal and never appear in renderer DTOs.
use super::{assessment_engine as engine, dto::*};
use crate::{
    application::ports::{
        llm_port::{CompletionInput, CompletionRequest},
        LLMPort,
    },
    shared::error::{AppError, Result},
};
use serde::Deserialize;
use std::{collections::HashSet, time::Duration};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Response {
    items: Vec<Item>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Item {
    requirement_index: usize,
    explanation: String,
    prompt: String,
    options: Vec<String>,
    answer_index: usize,
    accepted_answers: Vec<String>,
    ordered_values: Vec<String>,
    source_index: Option<usize>,
    quote: String,
    rubric: Vec<super::teaching::GeneratedCriterion>,
}

pub async fn generate(
    llm: &dyn LLMPort,
    request: &CreateLearningAssessmentBlueprintRequestDto,
    outcomes: &[LearningOutcomeDefinitionDto],
    sources: &[LearningSourceVersionDto],
    lessons: &[LearningLessonDto],
) -> Result<Vec<LearningAssessmentCandidateWriteDto>> {
    let mut slots = Vec::new();
    for (i, r) in request.requirements.iter().enumerate() {
        for _ in 0..r.count {
            slots.push((i, r));
        }
    }
    if slots.is_empty() || slots.len() > 64 || sources.len() > 24 {
        return Err(AppError::InvalidInput(
            "Assessment coverage or source scope is outside its allowed bounds.".into(),
        ));
    }
    let source_context=sources.iter().map(|s|serde_json::json!({"id":s.version.id,"title":s.version.title,"text":s.full_text.chars().take(8000).collect::<String>()})).collect::<Vec<_>>();
    let requirements=request.requirements.iter().map(|r|serde_json::json!({"outcome":outcomes.iter().find(|o|o.id==r.outcome_id).map(|o|o.title.as_str()).unwrap_or(r.outcome_id.as_str()),"format":r.format,"difficultyMin":r.difficulty_min,"difficultyMax":r.difficulty_max,"count":r.count})).collect::<Vec<_>>();
    let prompt=serde_json::json!({"title":request.title,"instructions":request.instructions,"requirements":requirements,"sources":source_context,"returnCount":slots.len(),"lessons":lessons.iter().map(|lesson|serde_json::json!({"title":lesson.title,"objective":lesson.objective,"blocks":lesson.blocks})).collect::<Vec<_>>()}).to_string();
    let system="Author source-grounded assessment items. Return only JSON. For each item give requirementIndex, prompt, an explanation/rationale, options, answerIndex, acceptedAnswers, orderedValues, sourceIndex, an exact quote from that source, and rubric. Rubric has two to four task-specific criteria for open formats (title, description, dimension), or an empty array for objectively keyed items. Criteria must state observable incomplete, adequate and strong performance without leaking the answer. Include a concrete scenario and deliverable that require the outcome, not a request to repeat its title. If there are no sources, author from the supplied lesson material and outcomes, set sourceIndex to null and quote to an empty string; never invent citations. MCQ uses answerIndex; short_answer uses acceptedAnswers and empty options; ordering uses options as the shuffled values and orderedValues as the correct permutation; open formats use empty options/keys. Do not include extra fields.";
    let schema = serde_json::json!({"type":"object","additionalProperties":false,"required":["items"],"properties":{"items":{"type":"array","minItems":slots.len(),"maxItems":slots.len(),"items":{"type":"object","additionalProperties":false,"required":["requirementIndex","prompt","explanation","options","answerIndex","acceptedAnswers","orderedValues","sourceIndex","quote","rubric"],"properties":{"requirementIndex":{"type":"integer","minimum":0,"maximum":request.requirements.len().saturating_sub(1)},"prompt":{"type":"string","minLength":8,"maxLength":8000},"explanation":{"type":"string","minLength":8,"maxLength":1200},"options":{"type":"array","maxItems":8,"items":{"type":"string","maxLength":500}},"answerIndex":{"type":"integer","minimum":0,"maximum":7},"acceptedAnswers":{"type":"array","maxItems":12,"items":{"type":"string","maxLength":500}},"orderedValues":{"type":"array","maxItems":16,"items":{"type":"string","maxLength":500}},"sourceIndex":if sources.is_empty() {serde_json::json!({"type":"null"})} else {serde_json::json!({"type":"integer","minimum":0,"maximum":sources.len()-1})},"rubric":super::teaching::rubric_schema(),"quote":{"type":"string","minLength":if sources.is_empty(){0}else{12},"maxLength":700}}}}}});
    let output_tokens = (slots.len() * 1100 + 600).min(16_000);
    let raw = tokio::time::timeout(Duration::from_secs(120), async {
        if llm.count_tokens(system) + llm.count_tokens(&prompt) + output_tokens
            > llm.max_context_tokens()
        {
            return Err(AppError::InvalidInput(
                "Assessment authoring request exceeds the model context window.".into(),
            ));
        }
        if llm.supports_typed_completions() {
            let response = llm
                .complete(&CompletionRequest {
                    input: vec![
                        CompletionInput::Message {
                            role: "system".into(),
                            content: system.into(),
                        },
                        CompletionInput::Message {
                            role: "user".into(),
                            content: prompt.clone(),
                        },
                    ],
                    json_schema: Some(schema.clone()),
                    reasoning_effort: Some("low".into()),
                    max_output_tokens: Some(output_tokens as u32),
                    ..Default::default()
                })
                .await?;
            if ["length", "max_tokens", "incomplete", "content_filter"]
                .iter()
                .any(|s| response.finish_reason.to_ascii_lowercase().contains(s))
            {
                return Err(AppError::InvalidInput(
                    "Assessment authoring output was incomplete.".into(),
                ));
            }
            Ok(response.text)
        } else {
            llm.generate(
                &format!("{system}\nReturn strict JSON matching this schema: {schema}\n{prompt}"),
                &[],
                None,
            )
            .await
        }
    })
    .await
    .map_err(|_| AppError::ServiceNotAvailable("Assessment authoring timed out.".into()))??;
    let raw = super::teaching::review_and_repair(llm, system, &prompt, &schema, raw, output_tokens)
        .await?;
    if raw.chars().count() > 80_000 {
        return Err(AppError::InvalidInput(
            "Assessment authoring response is too large.".into(),
        ));
    }
    let parsed: Response = serde_json::from_str(&raw).map_err(|_| {
        AppError::InvalidInput("Assessment authoring returned malformed structured data.".into())
    })?;
    if parsed.items.len() != slots.len() {
        return Err(AppError::InvalidInput(
            "Assessment authoring did not satisfy the requested item count.".into(),
        ));
    }
    let mut used = HashSet::new();
    let mut output = Vec::new();
    let mut coverage = vec![0usize; request.requirements.len()];
    for item in parsed.items {
        let Some(requirement) = request.requirements.get(item.requirement_index) else {
            return Err(AppError::InvalidInput(
                "Assessment authoring referenced invalid coverage.".into(),
            ));
        };
        let Some(requirement_coverage) = coverage.get_mut(item.requirement_index) else {
            return Err(AppError::InvalidInput(
                "Assessment authoring referenced invalid coverage.".into(),
            ));
        };
        *requirement_coverage += 1;
        let source = match item.source_index {
            Some(index) => Some(sources.get(index).ok_or_else(|| {
                AppError::InvalidInput(
                    "Assessment authoring referenced an unavailable source.".into(),
                )
            })?),
            None if sources.is_empty() && item.quote.is_empty() => None,
            None => {
                return Err(AppError::InvalidInput(
                    "Source-backed assessments require a valid source quote.".into(),
                ))
            }
        };
        if item.explanation.trim().chars().count() < 8
            || item.explanation.chars().count() > 1200
            || source.is_some_and(|source| {
                item.quote.trim().chars().count() < 12
                    || !source
                        .full_text
                        .to_lowercase()
                        .contains(&item.quote.trim().to_lowercase())
            })
        {
            return Err(AppError::InvalidInput(
                "Assessment item has invalid reasoning or source evidence.".into(),
            ));
        }
        let key = match requirement.format {
            LearningItemFormat::MultipleChoice => {
                if item.options.len() < 2
                    || item.options.len() > 8
                    || item.answer_index >= item.options.len()
                {
                    return Err(AppError::InvalidInput(
                        "Generated choice item has an invalid key.".into(),
                    ));
                }
                engine::LearningAssessmentKey::Choice(item.answer_index)
            }
            LearningItemFormat::ShortAnswer => {
                if item.accepted_answers.is_empty() {
                    return Err(AppError::InvalidInput(
                        "Generated short-answer item has no accepted answer.".into(),
                    ));
                }
                engine::LearningAssessmentKey::TextVariants(item.accepted_answers)
            }
            LearningItemFormat::Ordering => {
                if item.ordered_values.len() < 2 {
                    return Err(AppError::InvalidInput(
                        "Generated ordering item has no answer order.".into(),
                    ));
                }
                engine::LearningAssessmentKey::Ordering(item.ordered_values)
            }
            _ => engine::LearningAssessmentKey::Rubric,
        };
        let task_rubric = super::teaching::rubric(
            item.rubric,
            matches!(
                requirement.format,
                LearningItemFormat::Explanation | LearningItemFormat::Artifact
            ),
        )?;
        let candidate = engine::LearningAssessmentCandidate {
            id: uuid::Uuid::new_v4().to_string(),
            outcome_ids: vec![requirement.outcome_id.clone()],
            format: requirement.format,
            difficulty: requirement.difficulty_min,
            prompt: item.prompt.trim().into(),
            options: item.options,
            source_version_ids: source
                .map(|source| vec![source.version.id.clone()])
                .unwrap_or_default(),
            rubric: task_rubric
                .into_iter()
                .map(|c| engine::LearningRubricCriterion {
                    id: c.id,
                    title: c.title,
                    description: c.description,
                    max_points: c.max_points as u32,
                })
                .collect(),
            key: key.clone(),
        };
        engine::validate_candidate(&candidate)?;
        let digest = candidate
            .prompt
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .to_lowercase();
        if !used.insert(digest) {
            return Err(AppError::InvalidInput(
                "Assessment authoring returned duplicate items.".into(),
            ));
        }
        output.push(LearningAssessmentCandidateWriteDto {
            id: candidate.id,
            outcome_ids: candidate.outcome_ids,
            format: candidate.format,
            difficulty: candidate.difficulty,
            prompt: candidate.prompt,
            options: candidate.options,
            artifact_kind: None,
            rubric: candidate.rubric,
            source_version_ids: candidate.source_version_ids,
            answer_explanation: item.explanation.trim().into(),
            answer_key: serde_json::to_value(key)
                .map_err(|e| AppError::Serialization(e.to_string()))?,
        });
    }
    if request
        .requirements
        .iter()
        .enumerate()
        .any(|(i, r)| coverage.get(i).copied().unwrap_or(0) != r.count)
    {
        return Err(AppError::InvalidInput(
            "Assessment authoring did not satisfy coverage counts.".into(),
        ));
    }
    Ok(output)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct OpenGradeResponse {
    items: Vec<OpenGradeItem>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct OpenGradeItem {
    item_id: String,
    criteria: Vec<OpenGradeCriterion>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct OpenGradeCriterion {
    criterion_id: String,
    score: Option<u32>,
    max_points: u32,
    observation: String,
    artifact_quote: Option<String>,
}

pub async fn grade_open(
    llm: &dyn LLMPort,
    form: &LearningAssessmentFormDto,
) -> Result<(
    std::collections::HashMap<
        String,
        (
            LearningAssessmentGradeStatus,
            Vec<LearningAssessmentCriterionResultDto>,
        ),
    >,
    String,
)> {
    let open = form
        .items
        .iter()
        .filter(|i| {
            matches!(
                i.format,
                LearningItemFormat::Explanation | LearningItemFormat::Artifact
            )
        })
        .collect::<Vec<_>>();
    if open.is_empty() {
        return Ok((std::collections::HashMap::new(), llm.model_name().into()));
    }
    let items=open.iter().map(|item|serde_json::json!({"id":item.id,"prompt":item.prompt,"rubric":item.rubric,"answer":item.text_response,"artifact":item.artifact_json})).collect::<Vec<_>>();
    let prompt = serde_json::json!({"items":items}).to_string();
    let system="Grade learner responses provisionally against the supplied rubric. Return strict JSON with every item and every criterion exactly once. Scores may be null when evidence is uncertain; never infer mastery. Artifact quotes must be exact substrings of the submitted text; use null where no quote applies.";
    let schema = serde_json::json!({"type":"object","additionalProperties":false,"required":["items"],"properties":{"items":{"type":"array","minItems":open.len(),"maxItems":open.len(),"items":{"type":"object","additionalProperties":false,"required":["itemId","criteria"],"properties":{"itemId":{"type":"string"},"criteria":{"type":"array","minItems":1,"maxItems":16,"items":{"type":"object","additionalProperties":false,"required":["criterionId","score","maxPoints","observation","artifactQuote"],"properties":{"criterionId":{"type":"string"},"score":{"type":["integer","null"]},"maxPoints":{"type":"integer"},"observation":{"type":"string"},"artifactQuote":{"type":["string","null"]}}}}}}}}});
    if llm.count_tokens(system) + llm.count_tokens(&prompt) + 1500 > llm.max_context_tokens() {
        return Err(AppError::InvalidInput(
            "Assessment grading exceeds the model context window.".into(),
        ));
    }

    let raw = tokio::time::timeout(Duration::from_secs(120), async {
        if llm.supports_typed_completions() {
            let r = llm
                .complete(&CompletionRequest {
                    input: vec![
                        CompletionInput::Message {
                            role: "system".into(),
                            content: system.into(),
                        },
                        CompletionInput::Message {
                            role: "user".into(),
                            content: prompt,
                        },
                    ],
                    json_schema: Some(schema),
                    reasoning_effort: Some("low".into()),
                    max_output_tokens: Some(1500),
                    ..Default::default()
                })
                .await?;
            if ["length", "max_tokens", "incomplete", "content_filter"]
                .iter()
                .any(|s| r.finish_reason.to_ascii_lowercase().contains(s))
            {
                return Err(AppError::InvalidInput(
                    "Assessment grading output was incomplete.".into(),
                ));
            }
            Ok(r.text)
        } else {
            llm.generate(&format!("{system}\nReturn JSON.\n{prompt}"), &[], None)
                .await
        }
    })
    .await
    .map_err(|_| AppError::ServiceNotAvailable("Assessment grading timed out.".into()))??;
    if raw.chars().count() > 30_000 {
        return Err(AppError::InvalidInput(
            "Assessment grading output is too large.".into(),
        ));
    }
    let parsed: OpenGradeResponse = serde_json::from_str(&raw).map_err(|_| {
        AppError::InvalidInput("Assessment grading returned malformed structured data.".into())
    })?;
    if parsed.items.len() != open.len() {
        return Err(AppError::InvalidInput(
            "Assessment grading omitted open responses.".into(),
        ));
    }
    let mut out = std::collections::HashMap::new();
    for graded in parsed.items {
        let item = open
            .iter()
            .find(|i| i.id == graded.item_id)
            .ok_or_else(|| {
                AppError::InvalidInput("Assessment grading referenced an unknown response.".into())
            })?;
        let internal = graded
            .criteria
            .iter()
            .map(|c| engine::LearningProvisionalCriterionResult {
                criterion_id: c.criterion_id.clone(),
                score: c.score,
                max_points: c.max_points,
                observation: c.observation.clone(),
                artifact_quote: c.artifact_quote.clone(),
            })
            .collect::<Vec<_>>();
        let artifact_json = item
            .artifact_json
            .as_ref()
            .map(serde_json::Value::to_string)
            .unwrap_or_default();
        let artifact = format!(
            "{} {}",
            item.text_response.as_deref().unwrap_or(""),
            artifact_json
        );
        engine::validate_provisional_rubric_results(&item.rubric, &artifact, &internal)?;
        let uncertain = graded
            .criteria
            .iter()
            .any(|criterion| criterion.score.is_none());
        let grade_status = if uncertain {
            LearningAssessmentGradeStatus::Uncertain
        } else {
            LearningAssessmentGradeStatus::Provisional
        };
        let dto = graded
            .criteria
            .into_iter()
            .map(|c| LearningAssessmentCriterionResultDto {
                criterion_id: c.criterion_id,
                score: c.score,
                max_points: c.max_points,
                observation: c.observation,
                artifact_quote: c.artifact_quote,
            })
            .collect();
        if out.insert(graded.item_id, (grade_status, dto)).is_some() {
            return Err(AppError::InvalidInput(
                "Assessment grading repeated an item.".into(),
            ));
        }
    }
    Ok((out, llm.model_name().into()))
}
