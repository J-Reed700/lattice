//! Bounded strict-JSON prompts for the grounded practice tutor and grader.
use crate::features::learning::dto::*;
use crate::features::learning::model_call;
use lattice_core::{
    application::ports::LLMPort,
    shared::error::{AppError, Result},
};
use serde::Deserialize;

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
    let response = model_call::send(
        llm,
        model_call::structured(system, prompt, schema, max_tokens, "low"),
        None,
        "Practice request exceeds this model's context window.",
    )
    .await?;
    complete_reason(&response.finish_reason)?;
    let output = response.text;
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

const TUTOR_SYSTEM: &str = "You are a subject-neutral learning tutor. Do not write or replace the learner's artifact. Keep feedback actionable and tied to the visible rubric. Respond to the learner's actual work and previous tutor turns. For an orienting hint ask one question about where to start; for a concept hint point to the relevant idea; for a partial strategy give only the next step. Never give the final answer in Practice mode, hints or critiques. A critique should identify the first specific reasoning gap and ask the learner to revise it. Acknowledge correct steps without praising unsupported conclusions. Cite only exact source passages from the supplied frozen snapshots. If none are supplied, tutor from the task and general knowledge, acknowledge uncertainty, and return empty citations. Never claim mastery.";
const SOLUTION_SYSTEM: &str = "Give a concise worked reference solution for the learning task. When source snapshots are supplied, use them for factual claims. Without sources, use the stated task and general knowledge, acknowledge uncertainty, and return no citations. Cite exact quotations and do not imply that viewing this solution is evidence of learner ability.";

/// Tokens a practice call may still add to `prompt`: what its model's window
/// leaves for material once `system`, `prompt` and the answer are paid for.
fn window_room(
    llm: &dyn LLMPort,
    system: &str,
    prompt: &serde_json::Value,
    output_tokens: usize,
) -> usize {
    crate::features::learning::reference_collection::snapshot_room(
        llm,
        system,
        prompt,
        output_tokens,
    )
}

/// `prompt` with the newest whole tutor turns, newest first, while they fit
/// half the room the tutor call's window leaves (the snapshots take the
/// rest), and a line saying how many earlier turns are not shown.
pub(in crate::features::learning) fn with_recent_turns(
    llm: &dyn LLMPort,
    mut prompt: serde_json::Value,
    turns: &[LearningPracticeTutorTurnDto],
) -> serde_json::Value {
    let room = window_room(llm, TUTOR_SYSTEM, &prompt, 1400) / 2;
    let mut shown = Vec::new();
    let mut used = 0;
    for turn in turns.iter().rev() {
        let entry = serde_json::json!({"request":turn.prompt,"response":turn.response,"hintLevel":turn.hint_level});
        let cost = llm.count_tokens(&entry.to_string()) + 1;
        if used + cost > room {
            break;
        }
        used += cost;
        shown.push(entry);
    }
    let earlier = turns.len() - shown.len();
    if let Some(fields) = prompt.as_object_mut() {
        fields.insert("recentTurns".into(), serde_json::Value::Array(shown));
        fields.insert(
            "earlierTurns".into(),
            serde_json::json!(if earlier == 0 {
                String::new()
            } else {
                format!("{earlier} earlier turns not shown")
            }),
        );
    }
    prompt
}

/// `prompt` with the snapshot passages `query` retrieves, as many as the
/// request's window leaves room for, under `sourceSnapshots`.
async fn with_snapshots(
    llm: &dyn LLMPort,
    system: &str,
    mut prompt: serde_json::Value,
    sources: &[LearningSourceVersionDto],
    query: &str,
    output_tokens: usize,
) -> Result<String> {
    let room = window_room(llm, system, &prompt, output_tokens);
    let passages = crate::features::learning::reference_collection::snapshot_passages(
        llm, sources, query, room,
    )
    .await?;
    if let Some(fields) = prompt.as_object_mut() {
        fields.insert("sourceSnapshots".into(), passages);
    }
    Ok(prompt.to_string())
}
fn tutor_schema() -> serde_json::Value {
    // Length caps above 2000 are checked after parsing: as schema bounds they
    // would void llama.cpp's whole grammar.
    serde_json::json!({
        "type":"object", "additionalProperties":false,
        "required":["response","citations","proposals"],
        "properties": {
            "response":{"type":"string","minLength":1},
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
    // Length caps above 2000 are checked after parsing: as schema bounds they
    // would void llama.cpp's whole grammar.
    serde_json::json!({
        "type":"object", "additionalProperties":false,
        "required":["solution","citations"],
        "properties":{"solution":{"type":"string","minLength":1},"citations":{"type":"array","maxItems":5,"items":{
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
    let prompt = serde_json::json!({"taskPrompt":session.task_prompt,"objective":session.lesson_objective,"learnerArtifact":session.artifact.text,"rubric":session.rubric,"mode":session.summary.mode,"recentTurns":[],"earlierTurns":"","revisionContext":session.assistance.iter().filter(|a|a.details.get("revisesSessionId").is_some()).map(|a|&a.details).collect::<Vec<_>>(),"requestKind":request.request_kind,"hintLevel":request.hint_level,"learnerRequest":request.prompt,"sourceSnapshots":[]});
    let query = format!("{} {}", session.task_prompt, request.prompt);
    let prompt = with_recent_turns(llm, prompt, &session.tutor_turns);
    let prompt = with_snapshots(llm, TUTOR_SYSTEM, prompt, sources, &query, 1400).await?;
    let raw = complete_json(llm, TUTOR_SYSTEM, prompt, tutor_schema(), 1400).await?;
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

/// A worked solution, its snapshots sized from the model's window as the
/// tutor's are.
pub async fn solution(
    llm: &dyn LLMPort,
    session: &LearningPracticeSessionDto,
    sources: &[LearningSourceVersionDto],
) -> Result<SolutionJson> {
    let prompt = serde_json::json!({"taskPrompt":session.task_prompt,"objective":session.lesson_objective,"sourceSnapshots":[]});
    let query = format!("{} {}", session.task_prompt, session.lesson_objective);
    let prompt = with_snapshots(llm, SOLUTION_SYSTEM, prompt, sources, &query, 1400).await?;
    let raw = complete_json(llm, SOLUTION_SYSTEM, prompt, solution_schema(), 1400).await?;
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
    let system = "Evaluate the learner artifact against each visible criterion. Scores are limited, provisional evidence, not mastery claims. For each criterion, identify what the learner did, the specific gap if any, and one actionable next revision. Each evidence_quote must be one continuous exact substring of the submitted artifact, preserving spaces and line breaks; never use ellipses, join separate excerpts, or reformat code. Choose a shorter exact quote if needed. Do not reward length or confident wording. A correct answer with flawed reasoning must lose reasoning credit. Give an observation tied to artifact text. Use null, never zero, when there is no assessable work for a criterion. Zero means an observed incorrect attempt. Use uncertain status when the artifact lacks enough evidence to judge; all scores must then be null.";
    let mut prompt = task.to_string();
    let budget = session.rubric.len().saturating_mul(500).saturating_add(800);
    for attempt in 0..2 {
        let raw = complete_json(llm, system, prompt, schema.clone(), budget).await?;
        match validate_grade(&raw, session) {
            Ok(result) => return Ok(result),
            Err(error) if attempt == 0 => {
                prompt = serde_json::json!({"task":task,"rejectedFeedback":raw,"validationIssue":error.to_string(),"instruction":"Correct the rejected feedback against the original artifact and rubric. Copy each evidence_quote as one exact contiguous substring, with no ellipses or reformatted whitespace. Keep valid judgments and correct every criterion. No feedback has been saved yet."}).to_string();
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

#[cfg(test)]
mod window_tests {
    use super::*;
    use lattice_core::application::ports::llm_port::{CompletionRequest, CompletionResponse};

    struct Words;

    #[async_trait::async_trait]
    impl LLMPort for Words {
        async fn complete(&self, _request: &CompletionRequest) -> Result<CompletionResponse> {
            Ok(CompletionResponse::from_text("{}"))
        }
        fn model_name(&self) -> &str {
            "words"
        }
        fn max_context_tokens(&self) -> usize {
            131_072
        }
        fn count_tokens(&self, text: &str) -> usize {
            text.split_whitespace().count()
        }
        async fn is_ready(&self) -> Result<bool> {
            Ok(true)
        }
    }

    fn turns(count: usize) -> Vec<LearningPracticeTutorTurnDto> {
        (0..count)
            .map(|turn| LearningPracticeTutorTurnDto {
                id: format!("turn-{turn}"),
                prompt: format!("turn {turn}"),
                response: "word ".repeat(200),
                request_kind: LearningTutorRequestKind::Question,
                hint_level: None,
                citations: vec![],
                proposal_ids: vec![],
                model_name: "words".into(),
                created_at: turn as i64,
            })
            .collect()
    }

    fn shown(prompt: &serde_json::Value) -> usize {
        prompt["recentTurns"].as_array().map_or(0, Vec::len)
    }

    /// The tutor's history is sized from its model's window alone: the
    /// newest whole turns that fit half the room the window leaves, the older
    /// ones counted, not cut. No reading-speed estimate shrinks it.
    #[test]
    fn tutor_history_fills_half_the_window_room() {
        let turns = turns(1_000);
        let skeleton =
            serde_json::json!({"taskPrompt":"fixed prompt","recentTurns":[],"earlierTurns":""});
        let window_half = crate::features::learning::reference_collection::snapshot_room(
            &Words,
            TUTOR_SYSTEM,
            &skeleton,
            1400,
        ) / 2;
        let shown_prompt = with_recent_turns(&Words, skeleton, &turns);
        let kept = shown(&shown_prompt);
        assert!(kept > 0 && kept < turns.len());
        assert_eq!(
            shown_prompt["earlierTurns"],
            format!("{} earlier turns not shown", turns.len() - kept)
        );
        let turn_cost = |turn: &serde_json::Value| Words.count_tokens(&turn.to_string()) + 1;
        let cost: usize = shown_prompt["recentTurns"]
            .as_array()
            .unwrap()
            .iter()
            .map(turn_cost)
            .sum();
        assert!(cost <= window_half, "{cost} of a {window_half}-token half");
        // The next older turn would not have fit: the window, not a pace,
        // is what stopped the walk.
        let next =
            serde_json::json!({"request":"turn","response":"word ".repeat(200),"hintLevel":null});
        assert!(cost + turn_cost(&next) > window_half);
    }

    /// The snapshots take what the window leaves, so a long source fills the
    /// window rather than a fraction of it.
    #[tokio::test]
    async fn snapshots_fill_the_window_room() {
        let text = (0..20_000)
            .map(|word| format!("passage w{word}"))
            .collect::<Vec<_>>()
            .join(" ");
        let sources = vec![LearningSourceVersionDto {
            source_id: "source".into(),
            version: LearningSourceVersionSummaryDto {
                id: "version".into(),
                version_number: 1,
                title: "Long source".into(),
                publisher: None,
                resolved_url: None,
                excerpt: String::new(),
                content_sha256: String::new(),
                word_count: 20_000,
                truncated: false,
                extraction_version: "test".into(),
                acquired_at: 0,
                snapshot_hash: None,
            },
            full_text: text,
            usage: vec![],
        }];
        let skeleton = serde_json::json!({"taskPrompt":"fixed prompt","sourceSnapshots":[]});
        let room = window_room(&Words, TUTOR_SYSTEM, &skeleton, 1400);
        let prompt = with_snapshots(&Words, TUTOR_SYSTEM, skeleton, &sources, "passage", 1400)
            .await
            .unwrap();
        let sent = Words.count_tokens(TUTOR_SYSTEM) + Words.count_tokens(&prompt);
        assert!(
            sent + 1400 <= Words.max_context_tokens(),
            "{sent} tokens sent"
        );
        // A 131K window leaves room for the 40,000-word source; the
        // reading-speed budget used to hold the prompt near 7,200 tokens,
        // under a quarter of the window.
        assert!(room > 40_000);
        assert!(sent > Words.max_context_tokens() / 4, "{sent} tokens sent");
    }
}
