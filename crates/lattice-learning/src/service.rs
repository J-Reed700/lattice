//! Learning command orchestration and boundary validation.
use super::{dto::*, outline_draft::DraftRunRegistry, repository::LearningRepository};
use lattice_core::{
    application::ports::LLMPort,
    shared::error::{AppError, Result},
    shared::persistence::Database,
};

fn bounded(value: &str, name: &str, max: usize, required: bool) -> Result<()> {
    let n = value.trim().chars().count();
    if n > max || (required && n == 0) {
        return Err(AppError::InvalidInput(format!(
            "{name} must contain {}–{max} characters",
            if required { 1 } else { 0 }
        )));
    }
    Ok(())
}
fn not_blank(value: &str, name: &str) -> Result<()> {
    if value.trim().is_empty() {
        return Err(AppError::InvalidInput(format!("{name} must not be empty")));
    }
    Ok(())
}
fn uuid(value: &str, label: &str) -> Result<()> {
    uuid::Uuid::parse_str(value)
        .map(|_| ())
        .map_err(|_| AppError::InvalidInput(format!("Invalid {label} ID")))
}
pub fn validate_request(request: &GenerateLearningProgramRequestDto) -> Result<()> {
    if request.goal.trim().is_empty() {
        return Err(AppError::InvalidInput(
            "Learning goal must not be empty.".into(),
        ));
    }
    if !(10..=240).contains(&request.minutes_per_session) {
        return Err(AppError::InvalidInput(
            "Session length must be between 10 and 240 minutes.".into(),
        ));
    }
    let mut ids = std::collections::HashSet::new();
    for id in &request.document_ids {
        uuid(id, "document")?;
        if !ids.insert(id) {
            return Err(AppError::InvalidInput(
                "Select each document only once.".into(),
            ));
        }
    }
    for url in &request.source_urls {
        bounded(url, "Source URL", 2048, true)?;
        let parsed = url::Url::parse(url).map_err(|_| {
            AppError::InvalidInput("Source URLs must be valid HTTP or HTTPS addresses.".into())
        })?;
        if !matches!(parsed.scheme(), "http" | "https") {
            return Err(AppError::InvalidInput(
                "Source URLs must use HTTP or HTTPS.".into(),
            ));
        }
    }
    Ok(())
}

pub async fn generate(
    repo: &LearningRepository,
    drafts: &DraftRunRegistry,
    llm: &dyn LLMPort,
    request: GenerateLearningProgramRequestDto,
    sources: Vec<LearningSourceDto>,
) -> Result<LearningProgramDto> {
    generate_with_references(repo, drafts, llm, request, sources, &[]).await
}

pub async fn generate_with_references(
    repo: &LearningRepository,
    drafts: &DraftRunRegistry,
    llm: &dyn LLMPort,
    request: GenerateLearningProgramRequestDto,
    sources: Vec<LearningSourceDto>,
    references: &[super::sources::InitialReference],
) -> Result<LearningProgramDto> {
    generate_with_references_and_progress(
        repo,
        drafts,
        llm,
        request,
        sources,
        references,
        &super::outline_progress::OutlineProgress::default(),
        None,
    )
    .await
}

pub async fn generate_with_references_and_progress(
    repo: &LearningRepository,
    drafts: &DraftRunRegistry,
    llm: &dyn LLMPort,
    request: GenerateLearningProgramRequestDto,
    sources: Vec<LearningSourceDto>,
    references: &[super::sources::InitialReference],
    progress: &super::outline_progress::OutlineProgress,
    web: Option<&dyn lattice_core::application::ports::WebServiceTrait>,
) -> Result<LearningProgramDto> {
    validate_request(&request)?;
    if sources.is_empty() && (!request.document_ids.is_empty() || !request.source_urls.is_empty()) {
        return Err(AppError::InvalidInput(
            "No learning sources could be acquired.".into(),
        ));
    }
    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().timestamp_millis();
    // No overall deadline: a completed draft and repair must not consume the
    // time available to the final review. Cancellation and transport failures still apply.
    // Library previews are display data. Authoring must be able to retrieve
    // evidence from anywhere in each immutable capture, including later chapters.
    let mut sources = sources;
    let mut references = references.to_vec();
    let mut authoring_sources: Vec<_> = sources
        .iter()
        .map(
            |source| match references.iter().find(|r| r.source_id == source.id) {
                Some(reference) => super::outline_references::authoring_copy(source, reference),
                None => source.clone(),
            },
        )
        .collect();
    // References are planned and gathered before drafting, from public
    // subject searches, and supplement whatever the learner supplied.
    let mut plan = None;
    let mut gathered = 0;
    if let Some(web) = web {
        progress.stage(super::outline_progress::OutlineStage::GatheringReferences);
        let planned =
            super::outline_references::plan(llm, &request, &authoring_sources, progress).await?;
        let found =
            super::outline_references::gather(llm, web, &planned, &authoring_sources, progress)
                .await?;
        for (source, reference) in found.sources.iter().zip(&found.references) {
            authoring_sources.push(super::outline_references::authoring_copy(source, reference));
        }
        gathered = found.sources.len() as u32;
        sources.extend(found.sources);
        references.extend(found.references);
        plan = Some(planned);
    }
    let subtopics = plan
        .as_ref()
        .map(super::outline_references::ReferencePlan::subtopic_titles)
        .unwrap_or_default();
    // The author's citations are provisional. Restore typography and drop any
    // quote that does not match its source before the outline is decoded; a
    // bad quote costs the citation, never the draft. Evidence is linked in
    // `finish`, once the program row exists to hold the receipts.
    let settle = |raw: &str| -> Result<_> {
        let mut candidate: serde_json::Value = super::generation::parse_json(raw)?;
        super::outline_citations::restore_source_quotes(&mut candidate, &authoring_sources);
        let removed =
            super::outline_linking::strip_unmatched(&mut candidate, &authoring_sources, None);
        let modules = super::generation::decode_outline(
            &candidate.to_string(),
            &request,
            &authoring_sources,
        )?;
        validate_outline(&modules, &sources)?;
        Ok((candidate, removed, modules))
    };
    let ((candidate, removed, modules), _, _) = super::generation::draft_outline(
        llm,
        &request,
        &authoring_sources,
        &subtopics,
        progress,
        settle,
    )
    .await?;
    let mut draft =
        super::outline_draft::OutlineDraft::new(request.clone(), candidate, &authoring_sources);
    draft.reference_plan = plan;
    draft.review.references_gathered = gathered;
    draft.review.citations_removed = removed;
    let first = modules
        .first()
        .and_then(|m| m.lessons.first())
        .map(|l| l.id.clone());
    let program = LearningProgramDto {
        outline_review: Some(draft.review.clone()),
        summary: LearningProgramSummaryDto {
            id,
            title: request.goal.trim().chars().take(120).collect(),
            goal: request.goal.trim().into(),
            status: LearningProgramStatus::Draft,
            revision: 0,
            module_count: modules.len() as i64,
            lesson_count: modules.iter().map(|m| m.lessons.len() as i64).sum(),
            completed_lessons: 0,
            current_lesson_id: first,
            created_at: now,
        },
        prior_knowledge: request.prior_knowledge.trim().into(),
        minutes_per_session: request.minutes_per_session,
        model_name: llm.model_name().to_string(),
        modules,
        sources,
    };
    progress.check_cancelled()?;
    let _run = drafts.acquire(&program.summary.id)?;
    repo.create_outline_draft(&program, &references, Some(&draft))
        .await?;
    progress.saved(&program.summary.id);
    super::outline_draft::finish(
        repo,
        llm,
        program,
        draft,
        authoring_sources,
        progress,
        web,
        false,
    )
    .await
}

fn validate_outline(modules: &[LearningModuleDto], sources: &[LearningSourceDto]) -> Result<()> {
    if !(2..=10).contains(&modules.len()) {
        return Err(AppError::InvalidInput(
            "Generated outline must contain 2–10 modules.".into(),
        ));
    }
    let mut ids = std::collections::HashSet::new();
    for m in modules {
        uuid(&m.id, "module")?;
        if !ids.insert(m.id.as_str()) {
            return Err(AppError::InvalidInput(
                "Generated module IDs must be unique.".into(),
            ));
        }
        not_blank(&m.title, "Module title")?;
        not_blank(&m.summary, "Module summary")?;
        if m.outcomes.is_empty() || m.outcomes.len() > 12 {
            return Err(AppError::InvalidInput(
                "Each module needs 1–12 learning outcomes.".into(),
            ));
        }
        if !(2..=6).contains(&m.lessons.len()) {
            return Err(AppError::InvalidInput(
                "Each module must contain 2–6 lessons.".into(),
            ));
        }
        for l in &m.lessons {
            uuid(&l.id, "lesson")?;
            if !ids.insert(l.id.as_str()) {
                return Err(AppError::InvalidInput(
                    "Generated identifiers must be unique.".into(),
                ));
            }
            not_blank(&l.title, "Lesson title")?;
            not_blank(&l.objective, "Lesson objective")?;
            if !(1..=600).contains(&l.estimated_minutes) {
                return Err(AppError::InvalidInput(
                    "Lesson estimate must be between 1 and 600 minutes.".into(),
                ));
            }
            if l.preparation != LearningPreparation::Outline || !l.blocks.is_empty() {
                return Err(AppError::InvalidInput(
                    "Generated programs must begin as unprepared outlines.".into(),
                ));
            }
        }
    }
    if sources
        .iter()
        .any(|s| s.excerpt.trim().is_empty() || s.excerpt.chars().count() > 2400)
    {
        return Err(AppError::InvalidInput(
            "Source excerpts must be non-empty and bounded to 2400 characters.".into(),
        ));
    }
    Ok(())
}

pub async fn accept(
    repo: &LearningRepository,
    drafts: &DraftRunRegistry,
    request: AcceptLearningProgramRequestDto,
) -> Result<LearningProgramDto> {
    uuid(&request.program_id, "program")?;
    bounded(&request.title, "Program title", 120, true)?;
    let _run = drafts.acquire(&request.program_id)?;
    repo.accept(&request).await?;
    repo.get(&request.program_id).await
}
pub async fn prepare(
    database: &Database,
    repo: &LearningRepository,
    llm: &dyn LLMPort,
    runtimes: &super::embedded_runtime::BuiltinRuntimes,
    setups: &super::lessons::course_setup::CourseSetups,
    request: PrepareLearningLessonRequestDto,
) -> Result<LearningProgramDto> {
    uuid(&request.program_id, "program")?;
    uuid(&request.lesson_id, "lesson")?;
    let mut program = repo.get(&request.program_id).await?;
    if program.summary.revision != request.expected_revision {
        return Err(AppError::InvalidInput(
            "Program changed; reload and retry.".into(),
        ));
    }
    if program.summary.status != LearningProgramStatus::Active {
        return Err(AppError::InvalidInput(
            "Accept this program before preparing lessons.".into(),
        ));
    }
    let had_sources = repo.has_source_history(&request.program_id).await?;
    program.sources = repo.verification_sources(&request.program_id).await?;
    if had_sources && program.sources.is_empty() {
        return Err(AppError::InvalidInput(
            "This program has no active source versions available for lesson preparation.".into(),
        ));
    }
    let lesson = program
        .modules
        .iter()
        .flat_map(|m| &m.lessons)
        .find(|l| l.id == request.lesson_id)
        .ok_or_else(|| AppError::NotFound("Lesson not found in this program".into()))?;
    if lesson.preparation == LearningPreparation::Ready {
        return Err(AppError::InvalidInput(
            "Ready lessons are immutable.".into(),
        ));
    }
    let outline_revision =
        super::lessons::course_plan::outline_revision(database, &request.program_id).await?;
    let plan =
        super::lessons::course_plan::ensure(database, llm, &program, outline_revision, setups)
            .await?;
    let prepared =
        super::generation::prepare_lesson(llm, runtimes, &program, lesson, &plan).await?;
    validate_generated(&prepared, &program)?;
    repo.prepare(
        &request.program_id,
        &request.lesson_id,
        request.expected_revision,
        &prepared,
    )
    .await?;
    repo.get(&request.program_id).await
}

/// [`validate_prepared`] for a lesson the model wrote: what cannot be saved
/// is the model's answer, never the user's input.
pub(super) fn validate_generated(
    p: &PreparedLearningLesson,
    program: &LearningProgramDto,
) -> Result<()> {
    validate_prepared(p, program).map_err(|error| match error {
        AppError::InvalidInput(message) => AppError::ModelAnswerUnusable(message),
        other => other,
    })
}

pub(super) fn validate_prepared(
    p: &PreparedLearningLesson,
    program: &LearningProgramDto,
) -> Result<()> {
    if p.blocks.is_empty() || p.questions.is_empty() {
        return Err(AppError::InvalidInput(
            "Prepared lesson needs teaching blocks and assessment questions.".into(),
        ));
    }
    let source_ids: std::collections::HashSet<&str> =
        program.sources.iter().map(|s| s.id.as_str()).collect();
    let mut question_ids = std::collections::HashSet::new();
    let mut keys = std::collections::HashSet::new();
    for b in &p.blocks {
        super::teaching::validate_saved_rubric(&b.rubric)?;
        not_blank(&b.title, "Block title")?;
        not_blank(&b.body, "Block text")?;
        if (b.source_ids.is_empty() && !source_ids.is_empty())
            || b.source_ids
                .iter()
                .any(|id| !source_ids.contains(id.as_str()))
        {
            return Err(AppError::InvalidInput(
                "Lesson block references an unknown source.".into(),
            ));
        }
    }
    for q in &p.questions {
        uuid(&q.id, "question")?;
        if !question_ids.insert(q.id.as_str()) {
            return Err(AppError::InvalidInput(
                "Question identifiers must be unique.".into(),
            ));
        }
        not_blank(&q.prompt, "Question")?;
        if q.options.len() < 2
            || q.options.len() > 8
            || q.options.iter().any(|o| o.trim().is_empty())
        {
            return Err(AppError::InvalidInput(
                "Questions need 2–8 answer options.".into(),
            ));
        }
        let mut opts = std::collections::HashSet::new();
        if q.options
            .iter()
            .any(|o| !opts.insert(o.trim().to_lowercase()))
        {
            return Err(AppError::InvalidInput(
                "Question options must be unique.".into(),
            ));
        }
        if (q.source_ids.is_empty() && !source_ids.is_empty())
            || q.source_ids
                .iter()
                .any(|id| !source_ids.contains(id.as_str()))
        {
            return Err(AppError::InvalidInput(
                "Question references an unknown source.".into(),
            ));
        }
    }
    for k in &p.keys {
        if !question_ids.contains(k.question_id.as_str()) || !keys.insert(k.question_id.as_str()) {
            return Err(AppError::InvalidInput(
                "Answer keys must match distinct lesson questions.".into(),
            ));
        }
        let q = p
            .questions
            .iter()
            .find(|q| q.id == k.question_id)
            .ok_or_else(|| AppError::InvalidInput("Answer key has no matching question.".into()))?;
        if k.correct_index >= q.options.len() {
            return Err(AppError::InvalidInput(
                "Correct answer index is outside the available options.".into(),
            ));
        }
        not_blank(&k.explanation, "Answer explanation")?;
        if !k.rationales.is_empty() && k.rationales.len() != q.options.len() {
            return Err(AppError::InvalidInput(
                "Answer reasons must be one for each option.".into(),
            ));
        }
    }
    if keys.len() != question_ids.len() {
        return Err(AppError::InvalidInput(
            "Every generated question needs exactly one answer key.".into(),
        ));
    }
    let asked: Vec<LessonQuestionKind> = p.questions.iter().map(|q| q.kind.clone()).collect();
    if super::lessons::shape::policy()
        .missing_question_kind(&asked)
        .is_some()
    {
        return Err(AppError::InvalidInput(
            "Each prepared lesson needs practice, quiz, and test questions.".into(),
        ));
    }
    Ok(())
}

pub async fn complete(
    repo: &LearningRepository,
    request: CompleteLearningLessonRequestDto,
) -> Result<LearningProgramDto> {
    uuid(&request.program_id, "program")?;
    uuid(&request.lesson_id, "lesson")?;
    repo.complete(&request).await?;
    repo.get(&request.program_id).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::{fixture, prepared};

    #[test]
    fn a_generated_outline_is_checked_for_blank_text_never_for_how_long_it_is() {
        let mut program = fixture();
        assert!(validate_outline(&program.modules, &program.sources).is_ok());
        let module = &mut program.modules[0];
        module.title = "A module title that keeps going ".repeat(5);
        module.summary = "What the module is for, said at length. ".repeat(60);
        module.lessons[0].title = "A lesson title that keeps going ".repeat(7);
        module.lessons[0].objective = "What the learner can do once it is taught. ".repeat(40);
        let checked = validate_outline(&program.modules, &program.sources);
        assert!(checked.is_ok(), "{checked:?}");
    }

    #[test]
    fn a_key_carries_no_reasons_or_one_per_option_and_nothing_else() {
        let mut program = fixture();
        program.sources.clear();
        let lesson_id = program.modules[0].lessons[0].id.clone();
        let mut lesson = prepared(&lesson_id);
        for reasons in [vec![], vec![String::new(), "Unsupported fails.".into()]] {
            lesson.keys[0].rationales = reasons;
            assert!(validate_prepared(&lesson, &program).is_ok());
            assert!(validate_generated(&lesson, &program).is_ok());
        }
        for reasons in [
            vec!["Only one.".to_owned()],
            vec![String::new(), "Two.".into(), "Three.".into()],
        ] {
            lesson.keys[0].rationales = reasons;
            assert!(validate_prepared(&lesson, &program).is_err());
            assert!(matches!(
                validate_generated(&lesson, &program),
                Err(AppError::ModelAnswerUnusable(_))
            ));
        }
    }

    #[test]
    fn a_prepared_lesson_is_checked_for_blank_text_never_for_how_long_it_is() {
        let mut program = fixture();
        program.sources.clear();
        let lesson_id = program.modules[0].lessons[0].id.clone();
        let mut lesson = prepared(&lesson_id);
        assert!(validate_prepared(&lesson, &program).is_ok());
        lesson.blocks[0].title = "A block title that keeps going ".repeat(7);
        lesson.blocks[0].body = "What the block teaches, said at length. ".repeat(250);
        lesson.questions[0].prompt = "Which of these does the passage support? ".repeat(60);
        lesson.questions[0].options[0] = "An option that restates the claim in full. ".repeat(30);
        lesson.keys[0].explanation = "Why that option is the supported one. ".repeat(100);
        let checked = validate_prepared(&lesson, &program);
        assert!(checked.is_ok(), "{checked:?}");
        let saved = validate_generated(&lesson, &program);
        assert!(
            saved.is_ok(),
            "the gate a written lesson meets at save: {saved:?}"
        );
    }

    #[test]
    fn a_question_whose_options_differ_only_by_letter_case_is_accepted() {
        let mut program = fixture();
        program.sources.clear();
        let lesson_id = program.modules[0].lessons[0].id.clone();
        let mut lesson = prepared(&lesson_id);
        lesson.questions[0].options = vec!["Alpha".into(), "alpha".into()];
        let checked = validate_prepared(&lesson, &program);
        assert!(checked.is_ok(), "{checked:?}");
    }
}
