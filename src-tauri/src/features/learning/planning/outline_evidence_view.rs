//! Learner-visible citations projected from the durable raw outline draft.
//!
//! The public program DTO intentionally contains plain text. This view joins
//! that text back to the saved candidate only when the currently visible claim
//! still matches exactly. A later curriculum edit therefore loses its old
//! citation instead of inheriting evidence for words the source never backed.

use crate::features::learning::{
    dto::{LearningModuleDto, LearningProgramDto, LearningSourceDto},
    generation,
    outline_draft::{LearningOutlineReviewStatus, OutlineDraft},
    repository::LearningRepository,
};
use crate::shared::error::Result;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::SqlitePool;

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LearningOutlineCitationTarget {
    ModuleSummary,
    Outcome,
    LessonObjective,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LearningOutlineCitationDto {
    pub path: String,
    pub target: LearningOutlineCitationTarget,
    pub module_id: String,
    pub lesson_id: Option<String>,
    pub item_index: Option<usize>,
    pub claim: String,
    pub source_id: String,
    pub source_title: String,
    pub source_url: Option<String>,
    pub quote: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LearningOutlineEvidenceDto {
    pub review_status: LearningOutlineReviewStatus,
    pub content_sha256: String,
    pub citations: Vec<LearningOutlineCitationDto>,
    /// Saved citations omitted because the visible plan changed or the exact
    /// frozen passage is no longer available. They are never shown as current.
    pub unavailable_count: usize,
}

struct Target<'a> {
    kind: LearningOutlineCitationTarget,
    module_id: &'a str,
    lesson_id: Option<&'a str>,
    item_index: Option<usize>,
    path: String,
}

fn project_field(
    field: &Value,
    claim_key: &str,
    visible_claim: Option<&str>,
    target: Target<'_>,
    draft: &OutlineDraft,
    sources: &[LearningSourceDto],
) -> Option<LearningOutlineCitationDto> {
    let source_index = field.get("sourceIndex")?.as_u64()? as usize;
    let claim = field.get(claim_key)?.as_str()?.trim();
    let quote = field.get("quote")?.as_str()?.trim();
    if claim.is_empty() || visible_claim.map(str::trim) != Some(claim) {
        return None;
    }
    let source_id = draft.source_ids.get(source_index)?;
    let source = sources.iter().find(|source| &source.id == source_id)?;
    generation::validate_excerpt_quote(quote, &source.excerpt).ok()?;
    // The generation validator tolerates case/spacing differences. A visible
    // quotation labelled exact must also exist verbatim in the saved excerpt.
    if !source.excerpt.contains(quote) {
        return None;
    }
    Some(LearningOutlineCitationDto {
        path: target.path,
        target: target.kind,
        module_id: target.module_id.to_owned(),
        lesson_id: target.lesson_id.map(str::to_owned),
        item_index: target.item_index,
        claim: claim.to_owned(),
        source_id: source.id.clone(),
        source_title: source.title.clone(),
        source_url: source.url.clone(),
        quote: quote.to_owned(),
    })
}

fn cited(field: &Value) -> bool {
    field.get("sourceIndex").and_then(Value::as_u64).is_some()
}

fn matching_module<'a>(
    candidate: &Value,
    modules: &'a [LearningModuleDto],
    draft: &OutlineDraft,
    index: usize,
) -> Option<(usize, &'a LearningModuleDto)> {
    if let Some(bindings) = &draft.bindings {
        let binding = bindings.get(index)?;
        return modules
            .iter()
            .enumerate()
            .find(|(_, module)| module.id == binding.module_id);
    }
    // Legacy snapshots have no app-owned IDs. Never infer a binding for an
    // ambiguous title, and never use this fallback when an ID was saved.
    let title = candidate.get("title")?.as_str()?.trim();
    let mut matches = modules
        .iter()
        .enumerate()
        .filter(|(_, module)| module.title.trim() == title);
    let found = matches.next()?;
    matches.next().is_none().then_some(found)
}

pub(in crate::features::learning) fn project(
    program: &LearningProgramDto,
    draft: &OutlineDraft,
) -> LearningOutlineEvidenceDto {
    let mut citations = Vec::new();
    let mut unavailable_count = 0;
    let candidate_modules = draft
        .candidate
        .get("modules")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default();

    for (candidate_index, candidate) in candidate_modules.iter().enumerate() {
        let current = matching_module(candidate, &program.modules, draft, candidate_index);
        if let Some(field) = candidate.get("summary") {
            let citation = current.and_then(|(visible_module_index, module)| {
                project_field(
                    field,
                    "text",
                    Some(&module.summary),
                    Target {
                        kind: LearningOutlineCitationTarget::ModuleSummary,
                        module_id: &module.id,
                        lesson_id: None,
                        item_index: None,
                        path: format!("/modules/{visible_module_index}/summary"),
                    },
                    draft,
                    &program.sources,
                )
            });
            if let Some(citation) = citation {
                citations.push(citation);
            } else if cited(field) {
                unavailable_count += 1;
            }
        }

        for field in candidate
            .get("outcomes")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let candidate_claim = field.get("text").and_then(Value::as_str).map(str::trim);
            let current_outcome = current.and_then(|(visible_module_index, module)| {
                let mut matches = module
                    .outcomes
                    .iter()
                    .enumerate()
                    .filter(|(_, outcome)| Some(outcome.trim()) == candidate_claim);
                let (visible_outcome_index, outcome) = matches.next()?;
                matches.next().is_none().then_some((
                    visible_module_index,
                    module,
                    visible_outcome_index,
                    outcome,
                ))
            });
            let citation = current_outcome.and_then(
                |(visible_module_index, module, visible_outcome_index, outcome)| {
                    project_field(
                        field,
                        "text",
                        Some(outcome),
                        Target {
                            kind: LearningOutlineCitationTarget::Outcome,
                            module_id: &module.id,
                            lesson_id: None,
                            item_index: Some(visible_outcome_index),
                            path: format!(
                                "/modules/{visible_module_index}/outcomes/{visible_outcome_index}"
                            ),
                        },
                        draft,
                        &program.sources,
                    )
                },
            );
            if let Some(citation) = citation {
                citations.push(citation);
            } else if cited(field) {
                unavailable_count += 1;
            }
        }

        for (lesson_index, field) in candidate
            .get("lessons")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .enumerate()
        {
            let candidate_title = field.get("title").and_then(Value::as_str).map(str::trim);
            let current_lesson = current.and_then(|(visible_module_index, module)| {
                let bound_id = match &draft.bindings {
                    Some(bindings) => Some(
                        bindings
                            .get(candidate_index)?
                            .lesson_ids
                            .get(lesson_index)?,
                    ),
                    None => None,
                };
                let mut matches =
                    module
                        .lessons
                        .iter()
                        .enumerate()
                        .filter(|(_, lesson)| match bound_id {
                            Some(id) => &lesson.id == id,
                            None => Some(lesson.title.trim()) == candidate_title,
                        });
                let (visible_lesson_index, lesson) = matches.next()?;
                matches.next().is_none().then_some((
                    visible_module_index,
                    module,
                    visible_lesson_index,
                    lesson,
                ))
            });
            let citation = current_lesson.and_then(
                |(visible_module_index, module, visible_lesson_index, lesson)| {
                project_field(
                    field,
                    "objective",
                    Some(&lesson.objective),
                    Target {
                        kind: LearningOutlineCitationTarget::LessonObjective,
                        module_id: &module.id,
                        lesson_id: Some(&lesson.id),
                        item_index: Some(visible_lesson_index),
                        path: format!(
                            "/modules/{visible_module_index}/lessons/{visible_lesson_index}/objective"
                        ),
                    },
                    draft,
                    &program.sources,
                )
            });
            if let Some(citation) = citation {
                citations.push(citation);
            } else if cited(field) {
                unavailable_count += 1;
            }
        }
    }

    LearningOutlineEvidenceDto {
        review_status: draft.review.status.clone(),
        content_sha256: draft.review.content_hash.clone(),
        citations,
        unavailable_count,
    }
}

pub async fn get(
    pool: &SqlitePool,
    program_id: &str,
) -> Result<Option<LearningOutlineEvidenceDto>> {
    let repository = LearningRepository::new(pool.clone());
    let program = repository.get(program_id).await?;
    let Some(draft) = repository.outline_draft(program_id).await? else {
        return Ok(None);
    };
    Ok(Some(project(&program, &draft)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::learning::{
        dto::{
            GenerateLearningProgramRequestDto, LearningLessonDto, LearningPreparation,
            LearningProgramStatus, LearningProgramSummaryDto,
        },
        outline_draft::OutlineDraft,
    };
    use serde_json::json;

    fn program(
        candidate: Value,
        sources: Vec<LearningSourceDto>,
    ) -> (LearningProgramDto, OutlineDraft) {
        let request = GenerateLearningProgramRequestDto {
            goal: "Understand the evidence".into(),
            prior_knowledge: "None".into(),
            minutes_per_session: 30,
            document_ids: vec![],
            source_urls: vec![],
            course_depth: None,
        };
        let mut draft = OutlineDraft::new(request, candidate, &sources);
        draft.review.status = LearningOutlineReviewStatus::Passed;
        let program = LearningProgramDto {
            outline_review: Some(draft.review.clone()),
            summary: LearningProgramSummaryDto {
                id: "program".into(),
                title: "Evidence".into(),
                goal: "Understand the evidence".into(),
                status: LearningProgramStatus::Active,
                revision: 1,
                module_count: 1,
                lesson_count: 1,
                completed_lessons: 0,
                current_lesson_id: Some("lesson".into()),
                created_at: 1,
            },
            prior_knowledge: "None".into(),
            minutes_per_session: 30,
            model_name: "test".into(),
            modules: vec![LearningModuleDto {
                id: "module".into(),
                title: "Foundations".into(),
                summary: "A visible summary.".into(),
                outcomes: vec!["A visible outcome.".into()],
                lessons: vec![LearningLessonDto {
                    id: "lesson".into(),
                    title: "Read closely".into(),
                    objective: "A visible objective.".into(),
                    estimated_minutes: 30,
                    preparation: LearningPreparation::Outline,
                    blocks: vec![],
                    questions: vec![],
                    completed: false,
                }],
                prerequisite_module_ids: vec![],
                project: None,
            }],
            sources,
            attempts: vec![],
        };
        (program, draft)
    }

    #[test]
    fn exposes_only_exact_citations_for_text_still_visible_in_the_program() {
        let quote =
            "This frozen passage contains enough exact words to support the curriculum claim.";
        let source = LearningSourceDto {
            id: "source".into(),
            title: "Field guide".into(),
            url: Some("https://example.com/guide".into()),
            excerpt: format!("Introduction. {quote} Conclusion."),
            acquired_at: 1,
        };
        let field = |text: &str| json!({"text":text,"sourceIndex":0,"quote":quote});
        let candidate = json!({"modules":[{
            "title":"Foundations",
            "summary":field("A visible summary."),
            "outcomes":[field("A visible outcome.")],
            "lessons":[{"title":"Read closely","objective":"A visible objective.","estimatedMinutes":30,"sourceIndex":0,"quote":quote}]
        }]});
        let (program, draft) = program(candidate, vec![source]);
        let evidence = project(&program, &draft);
        assert_eq!(evidence.citations.len(), 3);
        assert_eq!(evidence.unavailable_count, 0);
        assert_eq!(evidence.citations[2].lesson_id.as_deref(), Some("lesson"));
        assert_eq!(evidence.citations[2].quote, quote);
    }

    #[test]
    fn maps_citations_to_the_current_position_after_an_outcome_reorder() {
        let quote =
            "This frozen passage contains enough exact words to support the curriculum claim.";
        let source = LearningSourceDto {
            id: "source".into(),
            title: "Field guide".into(),
            url: None,
            excerpt: quote.into(),
            acquired_at: 1,
        };
        let field = |text: &str| json!({"text":text,"sourceIndex":0,"quote":quote});
        let candidate = json!({"modules":[{
            "title":"Foundations",
            "summary":field("A visible summary."),
            "outcomes":[field("First outcome."), field("Second outcome.")],
            "lessons":[]
        }]});
        let (mut program, draft) = program(candidate, vec![source]);
        program.modules[0].outcomes = vec!["Second outcome.".into(), "First outcome.".into()];

        let evidence = project(&program, &draft);
        let first = evidence
            .citations
            .iter()
            .find(|citation| citation.claim == "First outcome.")
            .unwrap();
        let second = evidence
            .citations
            .iter()
            .find(|citation| citation.claim == "Second outcome.")
            .unwrap();
        assert_eq!(first.item_index, Some(1));
        assert_eq!(first.path, "/modules/0/outcomes/1");
        assert_eq!(second.item_index, Some(0));
        assert_eq!(second.path, "/modules/0/outcomes/0");
    }

    #[test]
    fn does_not_reuse_an_old_citation_after_the_visible_claim_changes() {
        let quote =
            "This frozen passage contains enough exact words to support the curriculum claim.";
        let source = LearningSourceDto {
            id: "source".into(),
            title: "Field guide".into(),
            url: None,
            excerpt: quote.into(),
            acquired_at: 1,
        };
        let candidate = json!({"modules":[{
            "title":"Foundations",
            "summary":{"text":"Original summary.","sourceIndex":0,"quote":quote},
            "outcomes":[],
            "lessons":[]
        }]});
        let (mut program, draft) = program(candidate, vec![source]);
        program.modules[0].summary = "Rewritten summary.".into();
        let evidence = project(&program, &draft);
        assert!(evidence.citations.is_empty());
        assert_eq!(evidence.unavailable_count, 1);
    }

    #[test]
    fn withholds_a_quote_that_only_matches_after_normalization() {
        let quote =
            "This frozen passage contains enough exact words to support the curriculum claim.";
        let source = LearningSourceDto {
            id: "source".into(),
            title: "Field guide".into(),
            url: None,
            excerpt: quote.into(),
            acquired_at: 1,
        };
        let candidate = json!({"modules":[{
            "title":"Foundations",
            "summary":{"text":"A visible summary.","sourceIndex":0,"quote":quote.to_uppercase()},
            "outcomes":[], "lessons":[]
        }]});
        let (program, draft) = program(candidate, vec![source]);
        let evidence = project(&program, &draft);
        assert!(evidence.citations.is_empty());
        assert_eq!(evidence.unavailable_count, 1);
    }

    fn bound_fixture() -> (LearningProgramDto, OutlineDraft) {
        let quote =
            "This frozen passage contains enough exact words to support the curriculum claim.";
        let source = LearningSourceDto {
            id: "source".into(),
            title: "Guide".into(),
            url: None,
            excerpt: quote.into(),
            acquired_at: 1,
        };
        let field = |text: &str| json!({"text":text,"sourceIndex":0,"quote":quote});
        let candidate = json!({"modules":[{
            "title":"Foundations", "summary":field("A visible summary."),
            "outcomes":[field("A visible outcome.")],
            "lessons":[{"title":"Read closely","objective":"A visible objective.","sourceIndex":0,"quote":quote}]
        }]});
        let (program, mut draft) = program(candidate, vec![source]);
        draft.bind_program(&program).unwrap();
        let saved = serde_json::from_str(&serde_json::to_string(&draft).unwrap()).unwrap();
        (program, saved)
    }

    #[test]
    fn saved_ids_survive_renaming_reordering_and_duplicate_titles() {
        let (mut program, draft) = bound_fixture();
        let mut duplicate = program.modules[0].clone();
        duplicate.id = "another-module".into();
        duplicate.lessons[0].id = "another-lesson".into();
        program.modules[0].title = "Renamed module".into();
        program.modules[0].lessons[0].title = "Renamed lesson".into();
        program.modules.insert(0, duplicate);
        let evidence = project(&program, &draft);
        assert_eq!(evidence.citations.len(), 3);
        assert!(evidence
            .citations
            .iter()
            .all(|citation| citation.module_id == "module"));
        assert_eq!(evidence.citations[2].lesson_id.as_deref(), Some("lesson"));
        assert_eq!(evidence.citations[2].path, "/modules/1/lessons/0/objective");
        program.modules[1].lessons[0].objective = "An edited objective without evidence.".into();
        let changed = project(&program, &draft);
        assert_eq!(changed.citations.len(), 2);
        assert_eq!(changed.unavailable_count, 1);
    }

    #[test]
    fn missing_bound_identity_never_falls_back_to_a_matching_title() {
        let (mut program, draft) = bound_fixture();
        program.modules[0].id = "replacement".into();
        let evidence = project(&program, &draft);
        assert!(evidence.citations.is_empty());
        assert_eq!(evidence.unavailable_count, 3);
    }

    #[tokio::test]
    async fn repository_persists_app_owned_bindings_with_the_outline() -> Result<()> {
        let pool = crate::features::learning::tests::pool().await?;
        let program = crate::features::learning::tests::fixture();
        let candidate = json!({"modules":program.modules.iter().map(|module| json!({
            "title":module.title, "lessons":module.lessons.iter().map(|lesson| json!({"title":lesson.title})).collect::<Vec<_>>()
        })).collect::<Vec<_>>()});
        let mut draft = bound_fixture().1;
        draft.bindings = None;
        draft.candidate = candidate;
        draft.source_ids = program
            .sources
            .iter()
            .map(|source| source.id.clone())
            .collect();
        let repo = LearningRepository::new(pool);
        repo.create_outline_draft(&program, &[], Some(&draft))
            .await?;
        let saved = repo.outline_draft(&program.summary.id).await?.unwrap();
        let bindings = saved.bindings.unwrap();
        assert_eq!(bindings[0].module_id, program.modules[0].id);
        assert_eq!(bindings[1].lesson_ids[1], program.modules[1].lessons[1].id);
        Ok(())
    }
}
