//! Strict, source-scoped generation for practical activities.
//!
//! Public activity material and hidden evaluator files are generated in two
//! independent model calls. The evaluator never receives a reference solution,
//! and this module does not generate or persist one. Deterministic/container
//! checks therefore remain evidence about the learner artifact rather than a
//! comparison with model-authored answer text.

use crate::features::learning::{
    assessment_engine::LearningRubricCriterion,
    dto::{LearningLessonDto, LearningSourceVersionDto},
    lab_runtime::validate_lab_path,
    practical_dto::{
        LearningPracticalActivityKind, LearningSimulationSessionDto, LearningSimulationSpeaker,
    },
};
use crate::{
    application::ports::LLMPort,
    shared::error::{AppError, Result},
};
use serde::{Deserialize, Serialize};
use std::{collections::HashSet, time::Duration};

const MAX_RESPONSE_CHARS: usize = 80_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PracticalFileRole {
    Starter,
    Check,
    Reference,
}

#[derive(Debug, Clone)]
pub struct GeneratedPracticalFile {
    pub path: String,
    pub role: PracticalFileRole,
    pub content: String,
    pub editable: bool,
}

#[derive(Debug, Clone)]
pub struct GeneratedPracticalActivity {
    pub title: String,
    pub brief: String,
    pub source_version_ids: Vec<String>,
    pub rubric: Vec<LearningRubricCriterion>,
    pub files: Vec<GeneratedPracticalFile>,
}

#[derive(Debug, Clone)]
pub struct SimulationActivityContext {
    pub title: String,
    pub brief: String,
    pub rubric: Vec<LearningRubricCriterion>,
}

#[derive(Debug, Clone)]
pub struct GeneratedSimulationTurn {
    pub content: String,
    pub citations: Vec<crate::features::learning::dto::LearningPracticeCitationDto>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SimulationJson {
    content: String,
    citations: Vec<crate::features::learning::dto::LearningPracticeCitationDto>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ActivityJson {
    title: String,
    brief: String,
    source_version_ids: Vec<String>,
    rubric: Vec<CriterionJson>,
    starter_files: Vec<FileJson>,
    reference_files: Vec<FileJson>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CriterionJson {
    title: String,
    description: String,
    max_points: u32,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct FileJson {
    path: String,
    content: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ChecksJson {
    check_files: Vec<FileJson>,
}

fn invalid(message: impl Into<String>) -> AppError {
    AppError::InvalidInput(message.into())
}

fn bounded(value: &str, label: &str, maximum: usize) -> Result<()> {
    let length = value.trim().chars().count();
    if length == 0 || length > maximum {
        return Err(invalid(format!(
            "{label} must contain 1–{maximum} characters."
        )));
    }
    Ok(())
}

async fn complete_json(
    llm: &dyn LLMPort,
    system: &str,
    prompt: String,
    schema: serde_json::Value,
    max_tokens: u32,
) -> Result<String> {
    let response = crate::features::learning::model_call::send(
        llm,
        crate::features::learning::model_call::structured(
            system,
            prompt,
            schema,
            max_tokens as usize,
            "medium",
        ),
        None,
        Some(crate::features::learning::model_call::Deadline {
            after: Duration::from_secs(120),
            message: "Practical generation timed out.",
        }),
        "The practical activity exceeds this model's context window.",
    )
    .await?;
    let finish = response.finish_reason.to_ascii_lowercase();
    if ["length", "max_tokens", "max_output_tokens", "incomplete"]
        .iter()
        .any(|reason| finish.contains(reason))
    {
        return Err(invalid(
            "The model returned an incomplete practical activity.",
        ));
    }
    let output = response.text;
    if output.chars().count() > MAX_RESPONSE_CHARS {
        return Err(invalid("The generated practical activity is too large."));
    }
    Ok(output)
}

fn source_context(sources: &[LearningSourceVersionDto]) -> serde_json::Value {
    serde_json::Value::Array(
        sources
            .iter()
            .take(12)
            .map(|source| {
                serde_json::json!({
                    "sourceId": source.source_id,
                    "versionId": source.version.id,
                    "title": source.version.title,
                    "text": source.full_text.chars().take(10_000).collect::<String>(),
                })
            })
            .collect(),
    )
}

fn activity_schema() -> serde_json::Value {
    let file = serde_json::json!({
        "type":"object","additionalProperties":false,"required":["path","content"],
        "properties":{
            "path":{"type":"string","minLength":1,"maxLength":240},
            "content":{"type":"string","maxLength":262144}
        }
    });
    serde_json::json!({
        "type":"object","additionalProperties":false,
        "required":["title","brief","sourceVersionIds","rubric","starterFiles","referenceFiles"],
        "properties":{
            "title":{"type":"string","minLength":1,"maxLength":160},
            "brief":{"type":"string","minLength":1,"maxLength":12000},
            "sourceVersionIds":{"type":"array","maxItems":12,"items":{"type":"string"}},
            "rubric":{"type":"array","minItems":2,"maxItems":8,"items":{
                "type":"object","additionalProperties":false,
                "required":["title","description","maxPoints"],
                "properties":{
                    "title":{"type":"string","minLength":1,"maxLength":120},
                    "description":{"type":"string","minLength":1,"maxLength":900},
                    "maxPoints":{"type":"integer","minimum":1,"maximum":10}
                }
            }},
            "starterFiles":{"type":"array","maxItems":24,"items":file.clone()},
            "referenceFiles":{"type":"array","maxItems":12,"items":file}
        }
    })
}

fn checks_schema() -> serde_json::Value {
    serde_json::json!({
        "type":"object","additionalProperties":false,"required":["checkFiles"],
        "properties":{"checkFiles":{"type":"array","minItems":1,"maxItems":16,"items":{
            "type":"object","additionalProperties":false,"required":["path","content"],
            "properties":{
                "path":{"type":"string","minLength":1,"maxLength":240},
                "content":{"type":"string","minLength":1,"maxLength":262144}
            }
        }}}
    })
}

fn kind_name(kind: LearningPracticalActivityKind) -> &'static str {
    match kind {
        LearningPracticalActivityKind::CodeLab => "code lab",
        LearningPracticalActivityKind::Debugging => "debugging investigation",
        LearningPracticalActivityKind::CodeReview => "code review",
        LearningPracticalActivityKind::Incident => "incident response simulation",
        LearningPracticalActivityKind::SystemDesign => "system design exercise",
        LearningPracticalActivityKind::Project => "project revision",
        LearningPracticalActivityKind::Interview => "interview simulation",
        LearningPracticalActivityKind::Conversation => "conversation simulation",
        LearningPracticalActivityKind::WritingRevision => "writing revision",
        LearningPracticalActivityKind::Custom => "custom practical exercise",
    }
}

fn validate_files(
    files: impl IntoIterator<Item = (FileJson, PracticalFileRole)>,
) -> Result<Vec<GeneratedPracticalFile>> {
    let mut paths = HashSet::new();
    let mut total = 0usize;
    let mut output = Vec::new();
    for (file, role) in files {
        validate_lab_path(&file.path)?;
        if !paths.insert(file.path.clone()) {
            return Err(invalid(
                "Generated practical files contain a duplicate path.",
            ));
        }
        if file.content.len() > 256 * 1024 {
            return Err(invalid("A generated practical file exceeds 256 KiB."));
        }
        total = total.saturating_add(file.content.len());
        if total > 2 * 1024 * 1024 {
            return Err(invalid("Generated practical files exceed 2 MiB."));
        }
        let editable = role == PracticalFileRole::Starter;
        output.push(GeneratedPracticalFile {
            path: file.path,
            role,
            content: file.content,
            editable,
        });
    }
    Ok(output)
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PracticalRuntimeContext {
    pub name: String,
    pub command: Vec<String>,
    pub contract: String,
}

pub async fn generate_activity(
    llm: &dyn LLMPort,
    kind: LearningPracticalActivityKind,
    learner_brief: &str,
    lesson: &LearningLessonDto,
    sources: &[LearningSourceVersionDto],
    runtime: Option<&PracticalRuntimeContext>,
) -> Result<GeneratedPracticalActivity> {
    bounded(learner_brief, "Practical request", 4_000)?;
    let context = serde_json::json!({
        "activityKind": kind_name(kind),
        "learnerRequest": learner_brief,
        "lesson": {
            "title": lesson.title,
            "objective": lesson.objective,
            "blocks": lesson.blocks,
        },
        "runtimeEvaluationEnabled": runtime.is_some(),
        "executionEnvironment": runtime,
        "sourceSnapshots": source_context(sources),
    });
    let raw = complete_json(
        llm,
        "Design a demanding, subject-neutral practical exercise. It must require the learner to make decisions or create an artifact, state constraints and deliverables, and use a visible analytic rubric. When source snapshots are supplied, use them for factual claims. Without sources, build from the lesson and general knowledge, acknowledge uncertainty, and leave sourceVersionIds empty. Do not include a worked solution or hidden answer. For code work, starter files may contain incomplete code but never a completed answer. Return strict JSON.",
        context.to_string(),
        activity_schema(),
        4_000,
    )
    .await?;
    let parsed: ActivityJson = serde_json::from_str(&raw)
        .map_err(|_| invalid("The model returned malformed practical activity JSON."))?;
    bounded(&parsed.title, "Practical title", 160)?;
    bounded(&parsed.brief, "Practical brief", 12_000)?;
    if parsed.rubric.len() < 2 || parsed.rubric.len() > 8 {
        return Err(invalid("A practical rubric must contain 2–8 criteria."));
    }
    let available = sources
        .iter()
        .map(|source| source.version.id.as_str())
        .collect::<HashSet<_>>();
    let mut source_ids = Vec::new();
    for source_id in parsed.source_version_ids {
        if !available.contains(source_id.as_str()) {
            return Err(invalid(
                "The practical activity cited a source version outside its frozen context.",
            ));
        }
        if !source_ids.contains(&source_id) {
            source_ids.push(source_id);
        }
    }
    let mut rubric = Vec::with_capacity(parsed.rubric.len());
    for criterion in parsed.rubric {
        bounded(&criterion.title, "Rubric title", 120)?;
        bounded(&criterion.description, "Rubric description", 900)?;
        if !(1..=10).contains(&criterion.max_points) {
            return Err(invalid("Practical rubric points must be between 1 and 10."));
        }
        rubric.push(LearningRubricCriterion {
            id: uuid::Uuid::new_v4().to_string(),
            title: criterion.title,
            description: criterion.description,
            max_points: criterion.max_points,
        });
    }
    let mut files = validate_files(
        parsed
            .starter_files
            .into_iter()
            .map(|file| (file, PracticalFileRole::Starter))
            .chain(
                parsed
                    .reference_files
                    .into_iter()
                    .map(|file| (file, PracticalFileRole::Reference)),
            ),
    )?;
    if runtime.is_some() {
        if files.iter().all(|file| !file.editable) {
            return Err(invalid(
                "An executable practical activity needs at least one editable starter file.",
            ));
        }
        // Generate checks independently. This call receives the exercise brief
        // and starter surface, never a reference solution or answer artifact.
        let public_files = files
            .iter()
            .map(|file| serde_json::json!({"path":file.path,"content":file.content,"editable":file.editable}))
            .collect::<Vec<_>>();
        let raw = complete_json(
            llm,
            "Author deterministic evaluator files for the supplied practical brief and exact executionEnvironment contract. Use its required evaluator entrypoint, supported language version, and only dependencies explicitly available there. Test stated behavior and edge cases. Throw an uncaught assertion or exit nonzero on failure; zero checks must never count as success. Do not include a reference implementation, derive expected output from hidden solution text, use the network, or execute a host shell. Return strict JSON.",
            serde_json::json!({"brief":&parsed.brief,"publicFiles":public_files,"executionEnvironment":runtime}).to_string(),
            checks_schema(),
            2_500,
        )
        .await?;
        let checks: ChecksJson = serde_json::from_str(&raw)
            .map_err(|_| invalid("The model returned malformed evaluator JSON."))?;
        let check_files = validate_files(
            checks
                .check_files
                .into_iter()
                .map(|file| (file, PracticalFileRole::Check)),
        )?;
        let existing = files
            .iter()
            .map(|file| file.path.as_str())
            .collect::<HashSet<_>>();
        if check_files
            .iter()
            .any(|file| existing.contains(file.path.as_str()))
        {
            return Err(invalid(
                "Evaluator files cannot replace public practical files.",
            ));
        }
        files.extend(check_files);
    }
    Ok(GeneratedPracticalActivity {
        title: parsed.title,
        brief: parsed.brief,
        source_version_ids: source_ids,
        rubric,
        files,
    })
}

pub async fn generate_simulation_turn(
    llm: &dyn LLMPort,
    activity: &SimulationActivityContext,
    session: &LearningSimulationSessionDto,
    sources: &[LearningSourceVersionDto],
    final_feedback: bool,
) -> Result<GeneratedSimulationTurn> {
    if session.turns.len() > 80 {
        return Err(invalid("This simulation has reached its turn limit."));
    }
    let transcript = session
        .turns
        .iter()
        .map(|turn| {
            serde_json::json!({
                "speaker": match turn.speaker {
                    LearningSimulationSpeaker::Learner => "learner",
                    LearningSimulationSpeaker::Counterpart => "counterpart",
                    LearningSimulationSpeaker::Coach => "coach",
                },
                "content": turn.content,
            })
        })
        .collect::<Vec<_>>();
    let prompt = serde_json::json!({
        "scenario":{"title":activity.title,"brief":activity.brief},
        "roles":{"learner":session.learner_role,"counterpart":session.counterpart_role},
        "rubric":activity.rubric,
        "transcript":transcript,
        "sourceSnapshots":source_context(sources),
        "finalFeedback":final_feedback,
    })
    .to_string();
    let schema = serde_json::json!({
        "type":"object","additionalProperties":false,"required":["content","citations"],
        "properties":{
            "content":{"type":"string","minLength":1,"maxLength":6000},
            "citations":{"type":"array","maxItems":5,"items":{
                "type":"object","additionalProperties":false,
                "required":["sourceId","versionId","quote"],
                "properties":{
                    "sourceId":{"type":"string"},"versionId":{"type":"string"},
                    "quote":{"type":"string","minLength":1,"maxLength":700}
                }
            }}
        }
    });
    let system = if final_feedback {
        "You are the coach closing a role simulation. Evaluate only behavior visible in the learner's turns against the supplied rubric. Quote the learner when making an observation, state uncertainty where evidence is missing, never claim mastery or readiness, and cite only exact supplied source passages for factual guidance. Return strict JSON."
    } else {
        "Continue the role simulation as the named counterpart. Stay inside the scenario, introduce one realistic decision or complication, do not solve the task for the learner, and cite only exact supplied source passages when making a factual claim. Return strict JSON."
    };
    let raw = complete_json(llm, system, prompt, schema, 1_500).await?;
    let parsed: SimulationJson = serde_json::from_str(&raw)
        .map_err(|_| invalid("The simulation returned malformed structured output."))?;
    bounded(&parsed.content, "Simulation turn", 6_000)?;
    crate::features::learning::practice_generation::validate_citations(&parsed.citations, sources)?;
    Ok(GeneratedSimulationTurn {
        content: parsed.content,
        citations: parsed.citations,
    })
}
