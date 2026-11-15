//! Program outlines: the outline schema, drafting one from the goal and its
//! references, and decoding a drafted outline into modules and lessons.

use super::{
    invalid, optional_citation, optional_source_index_schema, parse_json, validate_outline_request,
    validate_text, OUTLINE_OUTPUT_RESERVE,
};
use crate::dto::{
    GenerateLearningProgramRequestDto, LearningCourseDepth, LearningLessonDto, LearningModuleDto,
    LearningPreparation, LearningSourceDto,
};
use crate::lessons::structured_call::{Asking, CallKind, ReasonFirst};
use crate::model_call;
use lattice_core::application::ports::LLMPort;
use lattice_core::application::services::model_contract::ask::{self, ask, Answered, Settled};
use lattice_core::application::services::model_contract::shown::Scope;
use lattice_core::shared::error::{AppError, ErrorClass, Result};
use lattice_core::shared::text::same_text::fold;
use serde::Deserialize;
use serde_json::json;
use std::collections::{HashMap, HashSet};

mod describe;
#[cfg(test)]
pub(crate) mod fixture;
mod prerequisites;
#[cfg(test)]
mod tests;

pub(crate) use describe::OutlineDescription;
#[cfg(test)]
pub(crate) use prerequisites::OutlinePrerequisites;

/// What the prose step of an outline works through before the outline is
/// written to its schema.
const OUTLINE_DRAFT_BRIEF: &str = "Decide the final capstone and what evidence each of its deliverables needs. Then lay out the modules in order: each module's title and two-sentence summary, its outcomes, its project milestone with deliverables and success criteria, the earlier modules it builds on, and its lessons with their objectives and estimated minutes.";

#[derive(Debug, Deserialize)]
#[cfg_attr(
    any(test, feature = "test-support"),
    derive(serde::Serialize, PartialEq)
)]
#[serde(rename_all = "camelCase")]
pub(crate) struct GeneratedOutline {
    modules: Vec<GeneratedModule>,
}

#[derive(Debug, Clone, Deserialize, serde::Serialize)]
#[cfg_attr(any(test, feature = "test-support"), derive(PartialEq))]
#[serde(rename_all = "camelCase")]
pub(crate) struct GeneratedModule {
    pub(crate) prerequisite_indices: Vec<usize>,
    pub(crate) project: crate::dto::LearningProjectMilestoneDto,
    pub(crate) title: String,
    pub(crate) summary: GeneratedEvidenceText,
    pub(crate) outcomes: Vec<GeneratedEvidenceText>,
    pub(crate) lessons: Vec<GeneratedOutlineLesson>,
}

#[derive(Debug, Clone, Deserialize, serde::Serialize)]
#[cfg_attr(any(test, feature = "test-support"), derive(PartialEq))]
#[serde(rename_all = "camelCase")]
pub(crate) struct GeneratedEvidenceText {
    pub(crate) text: String,
    pub(crate) source_index: Option<usize>,
    pub(crate) quote: String,
}

#[derive(Debug, Clone, Deserialize, serde::Serialize)]
#[cfg_attr(any(test, feature = "test-support"), derive(PartialEq))]
#[serde(rename_all = "camelCase")]
pub(crate) struct GeneratedOutlineLesson {
    pub(crate) title: String,
    pub(crate) objective: String,
    pub(crate) estimated_minutes: i64,
    pub(crate) source_index: Option<usize>,
    pub(crate) quote: String,
}

/// The outline a reply holds, before it is held to the depth asked for.
fn read_outline(raw: &str) -> Result<GeneratedOutline> {
    parse_json(raw)
}

pub(crate) fn outline_schema(
    depth: Option<LearningCourseDepth>,
    source_count: usize,
) -> serde_json::Value {
    let (min_modules, max_modules, min_lessons, max_lessons) = LearningCourseDepth::bounds(depth);
    json!({
        "type":"object","additionalProperties":false,"required":["modules"],
        "properties":{"modules":{"type":"array","minItems":min_modules,"maxItems":max_modules,"items":{
            "type":"object","additionalProperties":false,
            "required":["title","summary","outcomes","lessons","prerequisiteIndices","project"],"properties":{
                "prerequisiteIndices":{"type":"array","maxItems":max_modules.saturating_sub(1),"items":{"type":"integer","minimum":0,"maximum":max_modules.saturating_sub(2)}},"project":crate::teaching::project_schema(),"title":{"type":"string"},"summary":{"type":"object","additionalProperties":false,"required":["text","sourceIndex","quote"],"properties":{"text":{"type":"string"},"sourceIndex":optional_source_index_schema(source_count),"quote":{"type":"string"}}},
                "outcomes":{"type":"array","minItems":1,"items":{
                    "type":"object","additionalProperties":false,"required":["text","sourceIndex","quote"],"properties":{
                        "text":{"type":"string"},"sourceIndex":optional_source_index_schema(source_count),"quote":{"type":"string"}
                    }
                }},
                "lessons":{"type":"array","minItems":min_lessons,"maxItems":max_lessons,"items":{
                    "type":"object","additionalProperties":false,"required":["title","objective","estimatedMinutes","sourceIndex","quote"],"properties":{
                    "title":{"type":"string"},"objective":{"type":"string"},"estimatedMinutes":{"type":"integer","minimum":1},"sourceIndex":optional_source_index_schema(source_count),"quote":{"type":"string"}
                    }
                }}
            }
        }}}
    })
}

/// Generate an outline at the requested depth, using supplied sources when present.
/// Topic-only content cannot claim citations. IDs are assigned after validation.
pub async fn generate_outline(
    llm: &dyn LLMPort,
    request: &GenerateLearningProgramRequestDto,
    sources: &[LearningSourceDto],
) -> Result<Vec<LearningModuleDto>> {
    generate_outline_with_progress(
        llm,
        request,
        sources,
        &crate::outline_progress::OutlineProgress::default(),
    )
    .await
}

pub async fn generate_outline_with_progress(
    llm: &dyn LLMPort,
    request: &GenerateLearningProgramRequestDto,
    sources: &[LearningSourceDto],
    progress: &crate::outline_progress::OutlineProgress,
) -> Result<Vec<LearningModuleDto>> {
    let (raw, context, output_tokens) =
        draft_outline(llm, request, sources, &[], progress, |raw| {
            decode_outline(raw, request, sources).map(|_| raw.to_owned())
        })
        .await?;
    let raw = crate::teaching::review_and_repair_with_progress(
        llm,
        "Design a rigorous curriculum.",
        &context.to_string(),
        &outline_schema(request.course_depth, sources.len()),
        raw,
        output_tokens,
        crate::teaching::OutlineReviewContext { progress, sources },
    )
    .await?;
    decode_outline(&raw, request, sources)
}

/// Draft the outline. `subtopics` are the reference plan's subtopic titles;
/// each becomes a retrieval query so its gathered passages reach the prompt.
/// Without a plan the goal is the only local retrieval query (it never leaves
/// the machine here). The modules the model describes are put into the stored
/// form with the prerequisites asked after them, and `accept` turns that into
/// the outline the caller keeps.
pub(crate) async fn draft_outline<T>(
    llm: &dyn LLMPort,
    request: &GenerateLearningProgramRequestDto,
    sources: &[LearningSourceDto],
    subtopics: &[String],
    progress: &crate::outline_progress::OutlineProgress,
    accept: impl Fn(&str) -> Result<T> + Send + Sync,
) -> Result<(T, serde_json::Value, usize)> {
    validate_outline_request(request)?;
    let (_, max_modules, _, max_lessons) = LearningCourseDepth::bounds(request.course_depth);
    let output_tokens = if request.course_depth.is_some() {
        max_modules * max_lessons * 240 + 1600
    } else {
        OUTLINE_OUTPUT_RESERVE
    };
    let references = crate::reference_collection::ReferenceCollection::lexical(sources)?;
    let queries: Vec<String> = if subtopics.is_empty() {
        vec![request.goal.clone()]
    } else {
        subtopics.to_vec()
    };
    let supplied = crate::outline_evidence::select(llm, &references, &queries).await?;
    let catalog = references.catalog(llm);
    let input = describe::Describe::of(
        llm,
        request,
        !sources.is_empty(),
        &supplied,
        &catalog,
        output_tokens,
    );
    progress.stage(crate::outline_progress::OutlineStage::Drafting);
    let call = model_call::answer_options();
    let settled = ask::<OutlineDescription>(
        llm,
        &Asking {
            progress: Some(progress),
            reasoning: Some(ReasonFirst {
                kind: CallKind::Authoring,
                subject: "the course outline",
                brief: OUTLINE_DRAFT_BRIEF,
            }),
        },
        &mut Scope::for_question(),
        &input,
        &call,
    )
    .await
    .map_err(|error| description_too_long(llm, error))?;
    let outline = match settled {
        Settled::Answered(outline) => outline,
        Settled::Unanswered(why) => return Err(ask::failed(&why)),
    };
    let mut modules = input.stored(&outline);
    let prerequisites =
        prerequisites::asked(llm, progress, &request.goal, &modules).await?;
    for (module, builds) in modules.iter_mut().zip(prerequisites) {
        module.prerequisite_indices = builds;
    }
    let accepted = accept(&json!({"modules": modules}).to_string())?;
    let context = shown_context(llm, &input, &call)?;
    Ok((accepted, context, output_tokens))
}

/// The task of the outline request as the model saw it, with the reference material that fit.
fn shown_context(
    llm: &dyn LLMPort,
    input: &describe::Describe,
    call: &lattice_core::application::services::grounded_generation::CallOptions,
) -> Result<serde_json::Value> {
    let room = ask::room::<OutlineDescription>(llm, &input.within_room(0), call)?;
    let held = input.within_room(room);
    let mut scope = Scope::for_question();
    let says = OutlineDescription::says(&held, &mut scope.request())?;
    serde_json::from_str(&says.task).map_err(|error| AppError::InternalError(error.to_string()))
}

/// A window refusal on a model whose window meets the outline's own minimum
/// is the learner's text being too long, not the window being too small.
fn description_too_long(llm: &dyn LLMPort, error: AppError) -> AppError {
    let window_needed = crate::model_requirements::outline()
        .min_context_tokens
        .unwrap_or(0);
    if llm.max_context_tokens() < window_needed {
        return error;
    }
    if ErrorClass::of(&error) == ErrorClass::DoesNotFit {
        return AppError::InvalidInput(
            "Your course description is too long for the Learning model's context window. Shorten the course description and try again."
                .into(),
        );
    }
    error
}

/// Normalized titles seen so far, with the module or lesson that first used each.
#[derive(Debug, Default)]
pub(crate) struct Titles {
    modules: HashMap<String, usize>,
    lessons: HashMap<String, (usize, usize)>,
}

impl Titles {
    /// Record module `number`'s title, refusing one an earlier module has.
    pub(crate) fn module(&mut self, number: usize, title: &str) -> Result<()> {
        match self.modules.insert(fold(title), number) {
            Some(earlier) => Err(invalid(format!(
                "Modules {earlier} and {number} share the title \"{}\"; give each module its own title.",
                title.trim()
            ))),
            None => Ok(()),
        }
    }

    /// Record the title of lesson `place` (module, lesson), refusing one an earlier lesson has.
    pub(crate) fn lesson(&mut self, place: (usize, usize), title: &str) -> Result<()> {
        match self.lessons.insert(fold(title), place) {
            Some((m, l)) => Err(invalid(format!(
                "Lessons {m}.{l} and {}.{} share the title \"{}\"; give each lesson its own title.",
                place.0,
                place.1,
                title.trim()
            ))),
            None => Ok(()),
        }
    }
}

/// The rules one module of an outline keeps, whoever wrote it, with the lesson count it is asked to have.
pub(crate) fn module_rules(
    number: usize,
    module: &GeneratedModule,
    depth: Option<LearningCourseDepth>,
    titles: &mut Titles,
) -> Result<()> {
    let (_, _, min_lessons, max_lessons) = LearningCourseDepth::bounds(depth);
    module_checks(number, module, Some(min_lessons..=max_lessons), titles)
}

/// The rules of [`module_rules`], holding the module to a lesson count only when `lessons` is given.
pub(super) fn module_checks(
    number: usize,
    module: &GeneratedModule,
    lessons: Option<std::ops::RangeInclusive<usize>>,
    titles: &mut Titles,
) -> Result<()> {
    let module_index = number.saturating_sub(1);
    crate::teaching::validate_project(&module.project)?;
    let unique_prerequisites: HashSet<_> = module.prerequisite_indices.iter().collect();
    if unique_prerequisites.len() != module.prerequisite_indices.len()
        || module
            .prerequisite_indices
            .iter()
            .any(|i| *i >= module_index)
    {
        return Err(invalid(match module_index {
            0 => "Module 1 comes first, so its prerequisiteIndices must be empty.".to_owned(),
            _ => format!(
                "Module {number}'s prerequisiteIndices must be distinct zero-based positions of earlier modules, 0 to {}.",
                module_index.saturating_sub(1)
            ),
        }));
    }
    if !validate_text(&module.title) {
        return Err(invalid(format!("Module {number} needs a title.")));
    }
    if !validate_text(&module.summary.text) {
        return Err(invalid(format!("Module {number} needs a summary.")));
    }
    if module.outcomes.is_empty() {
        return Err(invalid(format!(
            "Module {number} needs at least one outcome."
        )));
    }
    if let Some(range) = lessons {
        if !range.contains(&module.lessons.len()) {
            return Err(invalid(format!(
                "Module {number} needs {} to {} lessons; it has {}.",
                range.start(),
                range.end(),
                module.lessons.len()
            )));
        }
    }
    titles.module(number, &module.title)?;
    for (outcome_index, outcome) in module.outcomes.iter().enumerate() {
        if !validate_text(&outcome.text) {
            return Err(invalid(format!(
                "Outcome {} of module {number} must not be blank.",
                outcome_index + 1
            )));
        }
    }
    for (lesson_index, lesson) in module.lessons.iter().enumerate() {
        let place = (number, lesson_index + 1);
        let at = format!("Lesson {}.{}", place.0, place.1);
        if !validate_text(&lesson.title) {
            return Err(invalid(format!("{at} needs a title.")));
        }
        if !validate_text(&lesson.objective) {
            return Err(invalid(format!("{at} needs an objective.")));
        }
        if lesson.estimated_minutes < 1 {
            return Err(invalid(format!(
                "{at} needs an estimate of at least one minute."
            )));
        }
        titles.lesson(place, &lesson.title)?;
    }
    Ok(())
}

// Drafts must be structurally usable. Citations are optional; a present one
// must match its source exactly, which is the same rule the author, the
// repairer and the citation check all apply. A refusal names the module or
// lesson and the rule it broke, so the model asked again can fix exactly that.
pub(crate) fn decode_outline(
    raw: &str,
    request: &GenerateLearningProgramRequestDto,
    sources: &[LearningSourceDto],
) -> Result<Vec<LearningModuleDto>> {
    let (min_modules, max_modules, _, _) = LearningCourseDepth::bounds(request.course_depth);
    let generated = read_outline(raw)?;
    if !(min_modules..=max_modules).contains(&generated.modules.len()) {
        return Err(invalid(format!(
            "The outline needs {min_modules} to {max_modules} modules; this answer has {}.",
            generated.modules.len()
        )));
    }
    let mut titles = Titles::default();
    let mut modules = Vec::with_capacity(generated.modules.len());
    let module_ids: Vec<_> = generated
        .modules
        .iter()
        .map(|_| uuid::Uuid::new_v4().to_string())
        .collect();
    for (module_index, module) in generated.modules.into_iter().enumerate() {
        let number = module_index + 1;
        module_checks(number, &module, None, &mut titles)?;
        optional_citation(sources, module.summary.source_index, &module.summary.quote)?;
        let mut outcomes = Vec::with_capacity(module.outcomes.len());
        for outcome in module.outcomes {
            optional_citation(sources, outcome.source_index, &outcome.quote)?;
            outcomes.push(outcome.text.trim().to_owned());
        }
        let mut lessons = Vec::with_capacity(module.lessons.len());
        for lesson in module.lessons {
            optional_citation(sources, lesson.source_index, &lesson.quote)?;
            lessons.push(LearningLessonDto {
                id: uuid::Uuid::new_v4().to_string(),
                title: lesson.title.trim().to_owned(),
                objective: lesson.objective.trim().to_owned(),
                estimated_minutes: lesson.estimated_minutes,
                preparation: LearningPreparation::Outline,
                blocks: Vec::new(),
                completed: false,
            });
        }
        modules.push(LearningModuleDto {
            id: module_ids
                .get(module_index)
                .cloned()
                .ok_or_else(|| invalid("Missing module identity"))?,
            prerequisite_module_ids: module
                .prerequisite_indices
                .iter()
                .filter_map(|i| module_ids.get(*i).cloned())
                .collect(),
            project: Some(module.project),
            title: module.title.trim().to_owned(),
            summary: module.summary.text.trim().to_owned(),
            outcomes,
            lessons,
        });
    }
    Ok(modules)
}
