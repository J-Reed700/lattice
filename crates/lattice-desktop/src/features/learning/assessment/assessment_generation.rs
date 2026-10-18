//! Strict, bounded source-grounded assessment candidate authoring. Answer keys
//! remain internal and never appear in renderer DTOs.
use crate::features::learning::model_call;
use crate::features::learning::reference_collection::{ReferenceCollection, ReferencePassage};
use crate::features::learning::{assessment_engine as engine, dto::*};
use lattice_core::application::services::grounded_generation::GroundedRequest;
use lattice_core::{
    application::ports::LLMPort,
    shared::error::{AppError, Result},
};
use serde::Deserialize;
use std::collections::HashSet;

const SYSTEM: &str = "Author source-grounded assessment items. Each item needs an explanation of its rationale and, when sources are supplied, an exact quote from its sourceIndex source. Rubric has two to four task-specific criteria for open formats (title, description, dimension), or an empty array for objectively keyed items. Criteria must state observable incomplete, adequate and strong performance without leaking the answer. Include a concrete scenario and deliverable that require the outcome, not a request to repeat its title. If there are no sources, author from the supplied lesson material and outcomes, set sourceIndex to null and quote to an empty string; never invent citations. MCQ uses answerIndex; short_answer uses acceptedAnswers and empty options; ordering uses options as the shuffled values and orderedValues as the correct permutation; open formats use empty options/keys.";

/// One authoring request: `count` items for one requirement.
struct Batch {
    index: usize,
    count: usize,
    query: String,
    skeleton: serde_json::Value,
    schema: serde_json::Value,
    output_tokens: usize,
}

impl Batch {
    fn new(
        request: &CreateLearningAssessmentBlueprintRequestDto,
        outcomes: &[LearningOutcomeDefinitionDto],
        lessons: &[LearningLessonDto],
        index: usize,
        count: usize,
        source_count: usize,
    ) -> Self {
        let requirement = request.requirements.get(index);
        let outcome = requirement
            .and_then(|r| outcomes.iter().find(|o| o.id == r.outcome_id))
            .map(|o| o.title.clone())
            .or_else(|| requirement.map(|r| r.outcome_id.clone()))
            .unwrap_or_default();
        let requirements = requirement.map(|r| vec![serde_json::json!({"requirementIndex":index,"outcome":outcome,"format":r.format,"difficultyMin":r.difficulty_min,"difficultyMax":r.difficulty_max,"count":count})]).unwrap_or_default();
        let skeleton = serde_json::json!({"title":request.title,"instructions":request.instructions,"requirements":requirements,"sources":[],"returnCount":count,"lessons":lessons.iter().map(|lesson|serde_json::json!({"title":lesson.title,"objective":lesson.objective,"blocks":lesson.blocks})).collect::<Vec<_>>()});
        // An item prompt's 8000-character cap is checked by `validate_candidate`;
        // as a schema bound above 2000 it would void llama.cpp's whole grammar.
        let schema = serde_json::json!({"type":"object","additionalProperties":false,"required":["items"],"properties":{"items":{"type":"array","minItems":count,"maxItems":count,"items":{"type":"object","additionalProperties":false,"required":["requirementIndex","prompt","explanation","options","answerIndex","acceptedAnswers","orderedValues","sourceIndex","quote","rubric"],"properties":{"requirementIndex":{"type":"integer","minimum":index,"maximum":index},"prompt":{"type":"string","minLength":8},"explanation":{"type":"string","minLength":8,"maxLength":1200},"options":{"type":"array","maxItems":8,"items":{"type":"string","maxLength":500}},"answerIndex":{"type":"integer","minimum":0,"maximum":7},"acceptedAnswers":{"type":"array","maxItems":12,"items":{"type":"string","maxLength":500}},"orderedValues":{"type":"array","maxItems":16,"items":{"type":"string","maxLength":500}},"sourceIndex":if source_count == 0 {serde_json::json!({"type":"null"})} else {serde_json::json!({"type":"integer","minimum":0,"maximum":source_count-1})},"rubric":crate::features::learning::teaching::rubric_schema(),"quote":{"type":"string","minLength":if source_count == 0 {0} else {12},"maxLength":700}}}}}});
        Self {
            index,
            count,
            query: format!("{outcome} {}", request.instructions),
            skeleton,
            schema,
            output_tokens: count * 1100 + 600,
        }
    }

    /// The room this request leaves for source passages; zero when even its
    /// prompt and answer do not fit.
    fn room(&self, llm: &dyn LLMPort) -> usize {
        let mut request = GroundedRequest::new(SYSTEM, self.skeleton.to_string());
        request.output_tokens = self.output_tokens;
        model_call::source_room(llm, &request, usize::MAX)
    }

    async fn author(
        &self,
        llm: &dyn LLMPort,
        collection: &ReferenceCollection<'_>,
        sources: &[LearningSourceVersionDto],
    ) -> Result<Vec<Item>> {
        let room = self.room(llm);
        if room == 0 {
            return Err(AppError::InvalidInput(
                "Assessment authoring request exceeds the model context window.".into(),
            ));
        }
        let entry = |source_index: usize, passages: &[&str]| {
            let title = sources.get(source_index).map(|s| s.version.title.as_str());
            serde_json::json!({"sourceIndex":source_index,"title":title,"passages":passages})
        };
        let index_of = |passage: &ReferencePassage| {
            sources
                .iter()
                .position(|source| source.version.id == passage.source_id)
        };
        let passages = collection
            .retrieve_within(&self.query, |passages| {
                passages
                    .iter()
                    .map(|p| llm.count_tokens(&serde_json::json!(p.text).to_string()) + 2)
                    .sum::<usize>()
                    + passages
                        .iter()
                        .filter_map(index_of)
                        .collect::<HashSet<_>>()
                        .into_iter()
                        .map(|index| llm.count_tokens(&entry(index, &[]).to_string()) + 2)
                        .sum::<usize>()
                    <= room
            })
            .await?;
        let mut grouped: Vec<(usize, Vec<&str>)> = Vec::new();
        for passage in &passages {
            let Some(index) = index_of(passage) else {
                continue;
            };
            match grouped.iter_mut().find(|(known, _)| *known == index) {
                Some((_, texts)) => texts.push(passage.text.as_str()),
                None => grouped.push((index, vec![passage.text.as_str()])),
            }
        }
        let mut prompt = self.skeleton.clone();
        if let Some(fields) = prompt.as_object_mut() {
            fields.insert(
                "sources".into(),
                grouped
                    .iter()
                    .map(|(index, texts)| entry(*index, texts))
                    .collect(),
            );
        }
        let prompt = prompt.to_string();
        let response = model_call::send(
            llm,
            model_call::structured(
                SYSTEM,
                prompt.clone(),
                self.schema.clone(),
                self.output_tokens,
                "low",
            ),
            None,
            "Assessment authoring request exceeds the model context window.",
        )
        .await?;
        if ["length", "max_tokens", "incomplete", "content_filter"]
            .iter()
            .any(|s| response.finish_reason.to_ascii_lowercase().contains(s))
        {
            return Err(AppError::InvalidInput(
                "Assessment authoring output was incomplete.".into(),
            ));
        }
        let raw = crate::features::learning::teaching::review_and_repair(
            llm,
            SYSTEM,
            &prompt,
            &self.schema,
            response.text,
            self.output_tokens,
        )
        .await?;
        if raw.chars().count() > 80_000 {
            return Err(AppError::InvalidInput(
                "Assessment authoring response is too large.".into(),
            ));
        }
        let parsed: Response = serde_json::from_str(&raw).map_err(|_| {
            AppError::InvalidInput(
                "Assessment authoring returned malformed structured data.".into(),
            )
        })?;
        if parsed.items.len() != self.count
            || parsed
                .items
                .iter()
                .any(|item| item.requirement_index != self.index)
        {
            return Err(AppError::InvalidInput(
                "Assessment authoring did not satisfy the requested item count.".into(),
            ));
        }
        Ok(parsed.items)
    }
}

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
    rubric: Vec<crate::features::learning::teaching::GeneratedCriterion>,
}

pub async fn generate(
    llm: &dyn LLMPort,
    request: &CreateLearningAssessmentBlueprintRequestDto,
    outcomes: &[LearningOutcomeDefinitionDto],
    sources: &[LearningSourceVersionDto],
    lessons: &[LearningLessonDto],
) -> Result<Vec<LearningAssessmentCandidateWriteDto>> {
    if request.requirements.iter().all(|r| r.count == 0) {
        return Err(AppError::InvalidInput(
            "Assessment coverage or source scope is outside its allowed bounds.".into(),
        ));
    }
    // Sources are searched in full; each request carries the passages its
    // requirement retrieves, as many as its window holds.
    let documents: Vec<LearningSourceDto> = sources
        .iter()
        .map(|source| LearningSourceDto {
            id: source.version.id.clone(),
            title: source.version.title.clone(),
            url: None,
            excerpt: source.full_text.clone(),
            acquired_at: 0,
        })
        .collect();
    let collection = ReferenceCollection::lexical(&documents)?;
    let mut items = Vec::new();
    for (index, requirement) in request.requirements.iter().enumerate() {
        let mut remaining = requirement.count;
        while remaining > 0 {
            // As many of this requirement's items as one request holds.
            let mut count = remaining;
            while count > 1
                && Batch::new(request, outcomes, lessons, index, count, sources.len()).room(llm)
                    == 0
            {
                count = count.div_ceil(2);
            }
            let batch = Batch::new(request, outcomes, lessons, index, count, sources.len());
            items.extend(batch.author(llm, &collection, sources).await?);
            remaining -= count;
        }
    }
    let mut used = HashSet::new();
    let mut output = Vec::new();
    let mut coverage = vec![0usize; request.requirements.len()];
    for item in items {
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
        let task_rubric = crate::features::learning::teaching::rubric(
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
    let system="Grade learner responses provisionally against the supplied rubric. Grade every item and every criterion exactly once. Scores may be null when evidence is uncertain; never infer mastery. Artifact quotes must be exact substrings of the submitted text; use null where no quote applies.";
    let schema = serde_json::json!({"type":"object","additionalProperties":false,"required":["items"],"properties":{"items":{"type":"array","minItems":open.len(),"maxItems":open.len(),"items":{"type":"object","additionalProperties":false,"required":["itemId","criteria"],"properties":{"itemId":{"type":"string"},"criteria":{"type":"array","minItems":1,"maxItems":16,"items":{"type":"object","additionalProperties":false,"required":["criterionId","score","maxPoints","observation","artifactQuote"],"properties":{"criterionId":{"type":"string"},"score":{"type":["integer","null"]},"maxPoints":{"type":"integer"},"observation":{"type":"string"},"artifactQuote":{"type":["string","null"]}}}}}}}}});
    let response = model_call::send(
        llm,
        model_call::structured(system, prompt, schema, 1500, "low"),
        None,
        "Assessment grading exceeds the model context window.",
    )
    .await?;
    if ["length", "max_tokens", "incomplete", "content_filter"]
        .iter()
        .any(|s| response.finish_reason.to_ascii_lowercase().contains(s))
    {
        return Err(AppError::InvalidInput(
            "Assessment grading output was incomplete.".into(),
        ));
    }
    let raw = response.text;
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
