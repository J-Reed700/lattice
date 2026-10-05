//! Bounded strict-JSON prompts for the grounded practice tutor and grader.
use crate::features::learning::dto::*;
use crate::{
    application::ports::{
        llm_port::{CompletionInput, CompletionRequest},
        LLMPort,
    },
    shared::error::{AppError, Result},
};
use serde::Deserialize;
use std::time::Duration;

fn invalid(message: &str) -> AppError {
    AppError::InvalidInput(message.into())
}
fn complete_reason(reason: &str) -> Result<()> {
    if [
        "length",
        "max_tokens",
        "max_output_tokens",
        "incomplete",
        "content_filter",
    ]
    .iter()
    .any(|x| reason.to_ascii_lowercase().contains(x))
    {
        return Err(invalid("The model response was incomplete. Try again."));
    }
    Ok(())
}
async fn complete_json(
    llm: &dyn LLMPort,
    system: &str,
    prompt: String,
    schema: serde_json::Value,
    max_tokens: usize,
) -> Result<String> {
    if llm.count_tokens(system) + llm.count_tokens(&prompt) + max_tokens > llm.max_context_tokens()
    {
        return Err(invalid(
            "Practice request exceeds this model's context window.",
        ));
    }
    let output = tokio::time::timeout(Duration::from_secs(120), async {
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
                    max_output_tokens: Some(max_tokens as u32),
                    ..Default::default()
                })
                .await?;
            complete_reason(&r.finish_reason)?;
            Ok(r.text)
        } else {
            llm.generate(
                &format!("{system}\n\nReturn only JSON matching the schema: {schema}\n{prompt}"),
                &[],
                None,
            )
            .await
        }
    })
    .await
    .map_err(|_| AppError::ServiceNotAvailable("Practice model request timed out.".into()))??;
    if output.chars().count() > 20_000 {
        return Err(invalid(
            "The model response exceeded the structured-output limit.",
        ));
    }
    Ok(output)
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TutorJson {
    pub response: String,
    #[serde(default)]
    pub citations: Vec<LearningPracticeCitationDto>,
    #[serde(default)]
    pub proposals: Vec<ProposalJson>,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
#[serde(rename_all = "camelCase")]
pub struct ProposalJson {
    pub kind: LearningPracticeProposalKind,
    pub text: String,
    pub evidence_quote: Option<String>,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SolutionJson {
    pub solution: String,
    #[serde(default)]
    pub citations: Vec<LearningPracticeCitationDto>,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GradeJson {
    pub status: LearningPracticeGradeStatus,
    pub criteria: Vec<GradeCriterionJson>,
}
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GradeCriterionJson {
    pub criterion_id: String,
    pub score: Option<i64>,
    pub observation: String,
    pub evidence_quote: Option<String>,
}

fn source_context(sources: &[LearningSourceVersionDto]) -> serde_json::Value {
    serde_json::Value::Array(sources.iter().map(|s|serde_json::json!({"sourceId":s.source_id,"versionId":s.version.id,"title":s.version.title,"text":s.full_text.chars().take(12_000).collect::<String>()})).collect())
}
fn tutor_schema() -> serde_json::Value {
    serde_json::json!({
        "type":"object", "additionalProperties":false,
        "required":["response","citations","proposals"],
        "properties": {
            "response":{"type":"string","minLength":1,"maxLength":6000},
            "citations":{"type":"array","maxItems":5,"items":{
                "type":"object","additionalProperties":false,
                "required":["sourceId","versionId","quote"],
                "properties":{"sourceId":{"type":"string","maxLength":256},"versionId":{"type":"string","maxLength":256},"quote":{"type":"string","minLength":1,"maxLength":700}}
            }},
            "proposals":{"type":"array","maxItems":4,"items":{
                "type":"object","additionalProperties":false,
                "required":["kind","text","evidenceQuote"],
                "properties":{"kind":{"enum":["misconception","follow_up"]},"text":{"type":"string","minLength":1,"maxLength":700},"evidenceQuote":{"type":["string","null"],"maxLength":500}}
            }}
        }
    })
}
fn solution_schema() -> serde_json::Value {
    serde_json::json!({
        "type":"object", "additionalProperties":false,
        "required":["solution","citations"],
        "properties":{"solution":{"type":"string","minLength":1,"maxLength":6000},"citations":{"type":"array","maxItems":5,"items":{
            "type":"object","additionalProperties":false,
            "required":["sourceId","versionId","quote"],
            "properties":{"sourceId":{"type":"string"},"versionId":{"type":"string"},"quote":{"type":"string","minLength":1,"maxLength":700}}
        }}}
    })
}

pub async fn tutor(
    llm: &dyn LLMPort,
    session: &LearningPracticeSessionDto,
    request: &RequestLearningTutorResponseRequestDto,
    sources: &[LearningSourceVersionDto],
) -> Result<TutorJson> {
    let prompt=serde_json::json!({"taskPrompt":session.task_prompt,"objective":session.lesson_objective,"learnerArtifact":session.artifact.text,"rubric":session.rubric,"mode":session.summary.mode,"recentTurns":session.tutor_turns.iter().rev().take(4).map(|t|serde_json::json!({"request":t.prompt,"response":t.response,"hintLevel":t.hint_level})).collect::<Vec<_>>(),"revisionContext":session.assistance.iter().filter(|a|a.details.get("revisesSessionId").is_some()).map(|a|&a.details).collect::<Vec<_>>(),"requestKind":request.request_kind,"hintLevel":request.hint_level,"learnerRequest":request.prompt,"sourceSnapshots":source_context(sources)}).to_string();
    let raw=complete_json(llm,"You are a subject-neutral learning tutor. Do not write or replace the learner's artifact. Keep feedback actionable and tied to the visible rubric. Respond to the learner's actual work and previous tutor turns. For an orienting hint ask one question about where to start; for a concept hint point to the relevant idea; for a partial strategy give only the next step. Never give the final answer in Practice mode, hints or critiques. A critique should identify the first specific reasoning gap and ask the learner to revise it. Acknowledge correct steps without praising unsupported conclusions. Cite only exact source passages from the supplied frozen snapshots. If none are supplied, tutor from the task and general knowledge, acknowledge uncertainty, and return empty citations. Never claim mastery. Return strict JSON.",prompt,tutor_schema(),1400).await?;
    let parsed: TutorJson = serde_json::from_str(&raw)
        .map_err(|_| invalid("The tutor returned malformed structured feedback."))?;
    if parsed.response.trim().is_empty()
        || parsed.response.chars().count() > 6000
        || parsed.citations.len() > 5
        || parsed.proposals.len() > 4
    {
        return Err(invalid("Tutor response exceeded its allowed bounds."));
    }
    validate_citations(&parsed.citations, sources)?;
    for p in &parsed.proposals {
        if p.text.trim().is_empty()
            || p.text.chars().count() > 700
            || p.evidence_quote.as_ref().is_some_and(|q| {
                q.chars().count() > 500 || (!q.is_empty() && !session.artifact.text.contains(q))
            })
        {
            return Err(invalid("Tutor proposal contains invalid evidence."));
        }
    }
    Ok(parsed)
}

pub async fn solution(
    llm: &dyn LLMPort,
    session: &LearningPracticeSessionDto,
    sources: &[LearningSourceVersionDto],
) -> Result<SolutionJson> {
    let prompt=serde_json::json!({"taskPrompt":session.task_prompt,"objective":session.lesson_objective,"sourceSnapshots":source_context(sources)}).to_string();
    let raw=complete_json(llm,"Give a concise worked reference solution for the learning task. When source snapshots are supplied, use them for factual claims. Without sources, use the stated task and general knowledge, acknowledge uncertainty, and return no citations. Cite exact quotations and do not imply that viewing this solution is evidence of learner ability. Return strict JSON.",prompt,solution_schema(),1400).await?;
    let parsed: SolutionJson = serde_json::from_str(&raw)
        .map_err(|_| invalid("The model returned a malformed reference solution."))?;
    if parsed.solution.trim().is_empty()
        || parsed.solution.chars().count() > 6000
        || parsed.citations.len() > 5
    {
        return Err(invalid("Reference solution exceeded its allowed bounds."));
    }
    validate_citations(&parsed.citations, sources)?;
    Ok(parsed)
}

pub fn validate_citations(
    citations: &[LearningPracticeCitationDto],
    sources: &[LearningSourceVersionDto],
) -> Result<()> {
    for citation in citations {
        if citation.quote.trim().is_empty() || citation.quote.chars().count() > 700 {
            return Err(invalid("Citation quote must be 1–700 characters."));
        }
        let source = sources
            .iter()
            .find(|s| s.source_id == citation.source_id && s.version.id == citation.version_id)
            .ok_or_else(|| invalid("Citation refers to a source version outside this attempt."))?;
        if !source.full_text.contains(&citation.quote) {
            return Err(invalid(
                "Citation quote must occur exactly in the saved source version.",
            ));
        }
    }
    Ok(())
}

pub async fn grade(
    llm: &dyn LLMPort,
    session: &LearningPracticeSessionDto,
) -> Result<(
    LearningPracticeGradeStatus,
    Vec<LearningPracticeCriterionResultDto>,
)> {
    let rubric:Vec<_>=session.rubric.iter().map(|c|serde_json::json!({"id":c.id,"dimension":c.dimension,"title":c.title,"description":c.description,"maxPoints":c.max_points})).collect();
    let task = serde_json::json!({"taskPrompt":session.task_prompt,"artifact":session.artifact.text,"rubric":rubric});
    let schema = serde_json::json!({"type":"object","additionalProperties":false,"required":["status","criteria"],"properties":{"status":{"enum":["provisional","uncertain"]},"criteria":{"type":"array","minItems":session.rubric.len(),"maxItems":session.rubric.len(),"items":{"type":"object","additionalProperties":false,"required":["criterion_id","score","observation","evidence_quote"],"properties":{"criterion_id":{"type":"string"},"score":{"type":["integer","null"],"minimum":0,"maximum":session.rubric.iter().map(|c|c.max_points).max().unwrap_or(4)},"observation":{"type":"string","minLength":1,"maxLength":900},"evidence_quote":{"type":["string","null"],"maxLength":500}}}}}});
    let system = "Evaluate the learner artifact against each visible criterion. Scores are limited, provisional evidence, not mastery claims. For each criterion, identify what the learner did, the specific gap if any, and one actionable next revision. Each evidence_quote must be one continuous exact substring of the submitted artifact, preserving spaces and line breaks; never use ellipses, join separate excerpts, or reformat code. Choose a shorter exact quote if needed. Do not reward length or confident wording. A correct answer with flawed reasoning must lose reasoning credit. Give an observation tied to artifact text. Use null, never zero, when there is no assessable work for a criterion. Zero means an observed incorrect attempt. Use uncertain status when the artifact lacks enough evidence to judge; all scores must then be null. Return strict JSON only.";
    let mut prompt = task.to_string();
    let budget = session.rubric.len().saturating_mul(500).saturating_add(800);
    for attempt in 0..2 {
        let raw = complete_json(llm, system, prompt, schema.clone(), budget).await?;
        match validate_grade(&raw, session) {
            Ok(result) => return Ok(result),
            Err(error) if attempt == 0 => {
                prompt = serde_json::json!({"task":task,"rejectedFeedback":raw,"validationIssue":error.to_string(),"instruction":"Correct the rejected feedback against the original artifact and rubric. Copy each evidence_quote as one exact contiguous substring, with no ellipses or reformatted whitespace. Keep valid judgments. Return all criteria in the original schema. No feedback has been saved yet."}).to_string();
            }
            Err(error) => return Err(error),
        }
    }
    Err(invalid(
        "The grader could not produce validated feedback. Your draft is still saved.",
    ))
}

fn validate_grade(
    raw: &str,
    session: &LearningPracticeSessionDto,
) -> Result<(
    LearningPracticeGradeStatus,
    Vec<LearningPracticeCriterionResultDto>,
)> {
    let parsed: GradeJson = serde_json::from_str(raw)
        .map_err(|_| invalid("The grader returned malformed structured feedback."))?;
    let mut out = Vec::new();
    if parsed.criteria.len() != session.rubric.len() {
        return Err(invalid("The grader did not return every criterion."));
    }
    for mut result in parsed.criteria {
        // An uncertainty judgment cannot produce a numeric failure or success.
        if parsed.status == LearningPracticeGradeStatus::Uncertain {
            result.score = None;
        }
        let criterion = session
            .rubric
            .iter()
            .find(|c| c.id == result.criterion_id)
            .ok_or_else(|| invalid("The grader returned an unknown criterion."))?;
        if result
            .score
            .is_some_and(|s| s < 0 || s > criterion.max_points)
            || (result.score.is_some()
                && result
                    .evidence_quote
                    .as_ref()
                    .is_none_or(|q| q.trim().is_empty()))
            || result.observation.trim().is_empty()
            || result.observation.chars().count() > 900
            || result.evidence_quote.as_ref().is_some_and(|q| {
                q.chars().count() > 500 || (!q.is_empty() && !session.artifact.text.contains(q))
            })
        {
            return Err(invalid("The grader returned invalid rubric evidence."));
        }
        out.push(LearningPracticeCriterionResultDto {
            criterion_id: criterion.id.clone(),
            dimension: criterion.dimension.clone(),
            score: result.score,
            max_points: criterion.max_points,
            observation: result.observation,
            evidence_quote: result.evidence_quote,
        });
    }
    if out
        .iter()
        .map(|r| &r.criterion_id)
        .collect::<std::collections::HashSet<_>>()
        .len()
        != out.len()
    {
        return Err(invalid("The grader returned duplicate criteria."));
    }
    Ok((parsed.status, out))
}
