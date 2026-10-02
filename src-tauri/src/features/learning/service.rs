//! Learning command orchestration and boundary validation.
use super::{dto::*, repository::LearningRepository};
use crate::{
    application::ports::LLMPort,
    shared::error::{AppError, Result},
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
fn uuid(value: &str, label: &str) -> Result<()> {
    uuid::Uuid::parse_str(value)
        .map(|_| ())
        .map_err(|_| AppError::InvalidInput(format!("Invalid {label} ID")))
}
pub fn validate_request(request: &GenerateLearningProgramRequestDto) -> Result<()> {
    bounded(&request.goal, "Learning goal", 500, true)?;
    bounded(&request.prior_knowledge, "Prior knowledge", 2000, false)?;
    if !(10..=240).contains(&request.minutes_per_session) {
        return Err(AppError::InvalidInput(
            "Session length must be between 10 and 240 minutes.".into(),
        ));
    }
    if request.document_ids.is_empty() && request.source_urls.is_empty() {
        return Err(AppError::InvalidInput(
            "Select at least one document or source URL.".into(),
        ));
    }
    if request.document_ids.len() > 8 || request.source_urls.len() > 8 {
        return Err(AppError::InvalidInput(
            "Select no more than eight documents and eight source URLs.".into(),
        ));
    }
    if request.document_ids.len() + request.source_urls.len() > 12 {
        return Err(AppError::InvalidInput(
            "Select no more than twelve sources in total.".into(),
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
    llm: &dyn LLMPort,
    request: GenerateLearningProgramRequestDto,
    sources: Vec<LearningSourceDto>,
) -> Result<LearningProgramDto> {
    validate_request(&request)?;
    if sources.is_empty() {
        return Err(AppError::InvalidInput(
            "No learning sources could be acquired.".into(),
        ));
    }
    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().timestamp_millis();
    let modules = tokio::time::timeout(
        std::time::Duration::from_secs(180),
        super::generation::generate_outline(llm, &request, &sources),
    )
    .await
    .map_err(|_| AppError::ServiceNotAvailable("Program outline generation timed out.".into()))??;
    validate_outline(&modules, &sources)?;
    let first = modules
        .first()
        .and_then(|m| m.lessons.first())
        .map(|l| l.id.clone());
    let program = LearningProgramDto {
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
        attempts: vec![],
    };
    repo.create(&program).await?;
    Ok(program)
}

fn validate_outline(modules: &[LearningModuleDto], sources: &[LearningSourceDto]) -> Result<()> {
    if !(2..=6).contains(&modules.len()) {
        return Err(AppError::InvalidInput(
            "Generated outline must contain 2–6 modules.".into(),
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
        bounded(&m.title, "Module title", 120, true)?;
        bounded(&m.summary, "Module summary", 2000, true)?;
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
            bounded(&l.title, "Lesson title", 160, true)?;
            bounded(&l.objective, "Lesson objective", 1200, true)?;
            if !(1..=600).contains(&l.estimated_minutes) {
                return Err(AppError::InvalidInput(
                    "Lesson estimate must be between 1 and 600 minutes.".into(),
                ));
            }
            if l.preparation != LearningPreparation::Outline
                || !l.blocks.is_empty()
                || !l.questions.is_empty()
            {
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
    request: AcceptLearningProgramRequestDto,
) -> Result<LearningProgramDto> {
    uuid(&request.program_id, "program")?;
    bounded(&request.title, "Program title", 120, true)?;
    repo.accept(&request).await?;
    repo.get(&request.program_id).await
}
pub async fn prepare(
    repo: &LearningRepository,
    llm: &dyn LLMPort,
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
    program.sources = repo.active_sources(&request.program_id).await?;
    if program.sources.is_empty() {
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
    let prepared = tokio::time::timeout(
        std::time::Duration::from_secs(180),
        super::generation::prepare_lesson(llm, &program, lesson),
    )
    .await
    .map_err(|_| AppError::ServiceNotAvailable("Lesson preparation timed out.".into()))??;
    validate_prepared(&prepared, &program)?;
    repo.prepare(
        &request.program_id,
        &request.lesson_id,
        request.expected_revision,
        &prepared,
    )
    .await?;
    repo.get(&request.program_id).await
}

pub(super) fn validate_prepared(
    p: &PreparedLearningLesson,
    program: &LearningProgramDto,
) -> Result<()> {
    if p.blocks.is_empty() || p.blocks.len() > 32 || p.questions.is_empty() {
        return Err(AppError::InvalidInput(
            "Prepared lesson needs bounded teaching blocks and assessment questions.".into(),
        ));
    }
    let source_ids: std::collections::HashSet<&str> =
        program.sources.iter().map(|s| s.id.as_str()).collect();
    let mut question_ids = std::collections::HashSet::new();
    let mut keys = std::collections::HashSet::new();
    for b in &p.blocks {
        bounded(&b.title, "Block title", 160, true)?;
        bounded(&b.body, "Block text", 8000, true)?;
        if b.source_ids.is_empty()
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
        bounded(&q.prompt, "Question", 2000, true)?;
        if q.options.len() < 2
            || q.options.len() > 8
            || q.options
                .iter()
                .any(|o| o.trim().is_empty() || o.chars().count() > 1000)
        {
            return Err(AppError::InvalidInput(
                "Questions need 2–8 bounded answer options.".into(),
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
        if q.source_ids.is_empty()
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
        bounded(&k.explanation, "Answer explanation", 3000, true)?;
    }
    if keys.len() != question_ids.len() {
        return Err(AppError::InvalidInput(
            "Every generated question needs exactly one answer key.".into(),
        ));
    }
    let counts = [
        LearningAssessmentKind::Practice,
        LearningAssessmentKind::Quiz,
        LearningAssessmentKind::Test,
    ]
    .map(|kind| p.questions.iter().filter(|q| q.kind == kind).count());
    if counts.contains(&0) {
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
pub async fn submit(
    repo: &LearningRepository,
    request: SubmitLearningAttemptRequestDto,
) -> Result<LearningProgramDto> {
    uuid(&request.attempt_id, "attempt")?;
    uuid(&request.program_id, "program")?;
    uuid(&request.module_id, "module")?;
    if let Some(id) = &request.lesson_id {
        uuid(id, "lesson")?;
    }
    if request.answers.is_empty() || request.answers.len() > 128 {
        return Err(AppError::InvalidInput(
            "An attempt must contain 1–128 answers.".into(),
        ));
    }
    let mut unique = std::collections::HashSet::new();
    for a in &request.answers {
        uuid(&a.question_id, "question")?;
        if !unique.insert(&a.question_id) {
            return Err(AppError::InvalidInput(
                "Duplicate question answers are not allowed.".into(),
            ));
        }
    }
    repo.submit(&request).await?;
    repo.get(&request.program_id).await
}
