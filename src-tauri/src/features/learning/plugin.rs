use super::{
    curriculum::LearningGenerationJob, dto::*, plan_dto::*, portability_dto::*, practical_dto::*,
    repository::LearningRepository, service,
};
use crate::features::web::traits::WebServiceTrait;
use crate::{interfaces::di::Container, shared::api_result::ApiError};
use tauri::{
    plugin::{Builder, TauriPlugin},
    Manager, Runtime, State,
};

pub(super) fn source_request_hash<T: serde::Serialize>(
    request: &T,
) -> crate::shared::error::Result<String> {
    use sha2::Digest;
    let bytes = serde_json::to_vec(request)
        .map_err(|error| crate::shared::error::AppError::Serialization(error.to_string()))?;
    Ok(format!("{:x}", sha2::Sha256::digest(bytes)))
}

fn source_library(container: &Container) -> super::source_library::LearningSourceLibraryRepository {
    super::source_library::LearningSourceLibraryRepository::new(container.db_pool().clone())
}

fn validate_source_ids(
    program_id: &str,
    source_id: &str,
    version_id: &str,
    operation_id: &str,
) -> crate::shared::error::Result<()> {
    for (value, label) in [
        (program_id, "program"),
        (source_id, "source"),
        (version_id, "source version"),
        (operation_id, "operation"),
    ] {
        uuid::Uuid::parse_str(value).map_err(|_| {
            crate::shared::error::AppError::InvalidInput(format!("Invalid {label} ID"))
        })?;
    }
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn list_learning_programs(
    container: State<'_, Container>,
) -> Result<Vec<LearningProgramSummaryDto>, ApiError> {
    LearningRepository::new(container.db_pool().clone())
        .list()
        .await
        .map_err(ApiError::from)
}
#[tauri::command]
#[specta::specta]
pub async fn get_learning_program(
    id: String,
    container: State<'_, Container>,
) -> Result<LearningProgramDto, ApiError> {
    LearningRepository::new(container.db_pool().clone())
        .get(&id)
        .await
        .map_err(ApiError::from)
}
#[tauri::command]
#[specta::specta]
pub async fn generate_learning_program(
    request: GenerateLearningProgramRequestDto,
    container: State<'_, Container>,
) -> Result<LearningProgramDto, ApiError> {
    service::validate_request(&request).map_err(ApiError::from)?;
    let sources = super::sources::acquire(&container, &request)
        .await
        .map_err(ApiError::from)?;
    let llm = container.get_or_load_llm().await.map_err(ApiError::from)?;
    service::generate(
        &LearningRepository::new(container.db_pool().clone()),
        llm.as_ref(),
        request,
        sources,
    )
    .await
    .map_err(ApiError::from)
}
#[tauri::command]
#[specta::specta]
pub async fn accept_learning_program(
    request: AcceptLearningProgramRequestDto,
    container: State<'_, Container>,
) -> Result<LearningProgramDto, ApiError> {
    service::accept(
        &LearningRepository::new(container.db_pool().clone()),
        request,
    )
    .await
    .map_err(ApiError::from)
}
#[tauri::command]
#[specta::specta]
pub async fn prepare_learning_lesson(
    request: PrepareLearningLessonRequestDto,
    container: State<'_, Container>,
) -> Result<LearningProgramDto, ApiError> {
    let program = LearningRepository::new(container.db_pool().clone())
        .get(&request.program_id)
        .await
        .map_err(ApiError::from)?;
    if program.summary.revision != request.expected_revision {
        return Err(ApiError::from(
            crate::shared::error::AppError::InvalidInput(
                "Program changed; reload and retry.".into(),
            ),
        ));
    }
    if program.summary.status != LearningProgramStatus::Active {
        return Err(ApiError::from(
            crate::shared::error::AppError::InvalidInput(
                "Accept this program before preparing lessons.".into(),
            ),
        ));
    }
    ensure_before_use_sources(&container, &request.program_id)
        .await
        .map_err(ApiError::from)?;
    let operation_id = stable_job_operation_id(
        &request.program_id,
        &request.lesson_id,
        request.expected_revision,
    );
    let job_request = StartLearningGenerationJobRequestDto {
        operation_id,
        program_id: request.program_id.clone(),
        expected_revision: request.expected_revision,
        kind: super::curriculum::LearningGenerationJobKind::LessonPreparation,
        request_json: serde_json::json!({"lessonIds":[request.lesson_id]}).to_string(),
        progress_total: 1,
    };
    let job_repo = super::curriculum_repository::LearningCurriculumRepository::new(
        container.db_pool().clone(),
    );
    let job = job_repo
        .start_job(&job_request)
        .await
        .map_err(ApiError::from)?;
    let owned = container.inner().clone();
    let job_id = job.id.clone();
    tauri::async_runtime::spawn(async move {
        run_generation_job(owned, job_id).await;
    });
    let result = tokio::time::timeout(std::time::Duration::from_secs(185), async {
        loop {
            let latest = job_repo.job(&job.id).await?;
            match latest.status {
                super::curriculum::LearningGenerationJobStatus::Completed => return Ok(()),
                super::curriculum::LearningGenerationJobStatus::Failed => {
                    return Err(crate::shared::error::AppError::ServiceNotAvailable(
                        latest
                            .error
                            .unwrap_or_else(|| "Lesson preparation failed.".into()),
                    ))
                }
                super::curriculum::LearningGenerationJobStatus::Cancelled => {
                    return Err(crate::shared::error::AppError::InvalidState(
                        "Lesson preparation was cancelled.".into(),
                    ))
                }
                super::curriculum::LearningGenerationJobStatus::Interrupted => {
                    return Err(crate::shared::error::AppError::ServiceNotAvailable(
                        "Lesson preparation was interrupted. Retry the generation job.".into(),
                    ))
                }
                _ => tokio::time::sleep(std::time::Duration::from_millis(100)).await,
            }
        }
    })
    .await
    .map_err(|_| {
        ApiError::from(crate::shared::error::AppError::ServiceNotAvailable(
            "Lesson preparation timed out.".into(),
        ))
    })?;
    result.map_err(ApiError::from)?;
    LearningRepository::new(container.db_pool().clone())
        .get(&request.program_id)
        .await
        .map_err(ApiError::from)
}

fn stable_job_operation_id(program_id: &str, lesson_id: &str, revision: i64) -> String {
    use sha2::Digest;
    let digest = sha2::Sha256::digest(
        format!("learning-prepare:{program_id}:{lesson_id}:{revision}").as_bytes(),
    );
    let mut bytes = [0u8; 16];
    if let Some(prefix) = digest.get(..bytes.len()) {
        bytes.copy_from_slice(prefix);
    }
    if let Some(version_byte) = bytes.get_mut(6) {
        *version_byte = (*version_byte & 0x0f) | 0x50;
    }
    if let Some(variant_byte) = bytes.get_mut(8) {
        *variant_byte = (*variant_byte & 0x3f) | 0x80;
    }
    uuid::Uuid::from_bytes(bytes).to_string()
}

async fn ensure_before_use_sources(
    container: &Container,
    program_id: &str,
) -> crate::shared::error::Result<()> {
    let repo = source_library(container);
    let workspace = repo.workspace(program_id).await?;
    for source in workspace.sources.iter().filter(|s| {
        s.kind == LearningSourceKind::Web && s.freshness_policy == LearningSourcePolicy::BeforeUse
    }) {
        if source.pending_version_id.is_some() {
            return Err(crate::shared::error::AppError::InvalidInput(format!("A newer version of '{}' is available. Adopt it or change the freshness policy before preparing a lesson.",source.active_version.as_ref().map(|v|v.title.as_str()).unwrap_or("this source"))));
        }
        let operation_id = uuid::Uuid::new_v4().to_string();
        let request = RefreshLearningSourceRequestDto {
            operation_id: operation_id.clone(),
            program_id: program_id.into(),
            source_id: source.id.clone(),
            expected_revision: source.revision,
        };
        let refreshed = async {
            container
                .security_context()
                .rate_limiters()
                .web_ingest
                .check_rate_limit("learning_source_before_use")
                .await
                .map_err(|e| crate::shared::error::AppError::RateLimitExceeded(e.to_string()))?;
            let url = source.requested_url.as_deref().ok_or_else(|| {
                crate::shared::error::AppError::Database("Web source has no requested URL".into())
            })?;
            let article = container
                .web_service()
                .fetch_url_content(url)
                .await
                .map_err(|e| crate::shared::error::AppError::Network(e.to_string()))?;
            Ok::<_, crate::shared::error::AppError>(super::source_library::CapturedLearningSource {
                title: article.title.unwrap_or_else(|| url.into()),
                publisher: None,
                requested_url: Some(url.into()),
                resolved_url: Some(article.url),
                text: article.content,
                truncated: article.content_truncated,
                extraction_version: "web_article_v1".into(),
            })
        }
        .await;
        let (captured, failure) = match refreshed {
            Ok(value) => (Some(value), None),
            Err(error) => (None, Some(error.to_string())),
        };
        repo.refresh(&request, captured, failure).await?;
        let refreshed = repo.workspace(program_id).await?;
        let latest = refreshed
            .sources
            .iter()
            .find(|s| s.id == source.id)
            .and_then(|s| s.latest_check.as_ref())
            .filter(|check| check.operation_id == operation_id);
        match latest.map(|check| &check.status) {
            Some(status) => {
                super::source_library::LearningSourceLibraryRepository::before_use_allows(status)?
            }
            None => {
                return Err(crate::shared::error::AppError::ServiceNotAvailable(format!(
                    "Could not verify the before-use source '{}'. Retry after the source is reachable.",
                    source.active_version.as_ref().map(|v|v.title.as_str()).unwrap_or("web source")
                )))
            }
        }
    }
    Ok(())
}
#[tauri::command]
#[specta::specta]
pub async fn complete_learning_lesson(
    request: CompleteLearningLessonRequestDto,
    container: State<'_, Container>,
) -> Result<LearningProgramDto, ApiError> {
    service::complete(
        &LearningRepository::new(container.db_pool().clone()),
        request,
    )
    .await
    .map_err(ApiError::from)
}
#[tauri::command]
#[specta::specta]
pub async fn submit_learning_attempt(
    request: SubmitLearningAttemptRequestDto,
    container: State<'_, Container>,
) -> Result<LearningProgramDto, ApiError> {
    service::submit(
        &LearningRepository::new(container.db_pool().clone()),
        request,
    )
    .await
    .map_err(ApiError::from)
}
#[tauri::command]
#[specta::specta]
pub async fn delete_learning_program(
    id: String,
    container: State<'_, Container>,
) -> Result<(), ApiError> {
    uuid::Uuid::parse_str(&id).map_err(|_| {
        ApiError::from(crate::shared::error::AppError::InvalidInput(
            "Invalid program ID".into(),
        ))
    })?;
    LearningRepository::new(container.db_pool().clone())
        .delete(&id)
        .await
        .map_err(ApiError::from)
}

async fn learning_memory(
    container: &Container,
    program_id: &str,
) -> crate::shared::error::Result<LearningMemoryDto> {
    let repo = LearningRepository::new(container.db_pool().clone());
    let state = repo.memory_state(program_id).await?;
    let notes_repo = crate::features::daily_notes::repository::DailyNotesRepository::new(
        container.db_pool().clone(),
    );
    let conversations = crate::features::conversation::repository::ConversationRepository::new(
        container.db_pool().clone(),
    );
    let mut lesson_notes = Vec::with_capacity(state.lesson_notes.len());
    for (lesson_id, note_id) in state.lesson_notes {
        let row = notes_repo.get(&note_id).await?;
        let note = crate::features::daily_notes::commands::hydrate_workspace_note(
            &notes_repo,
            &conversations,
            row,
        )
        .await?;
        lesson_notes.push(LearningLessonNoteDto { lesson_id, note });
    }
    let now = chrono::Utc::now().timestamp_millis();
    let (study_deck, due_count) = match state.deck_id.as_deref() {
        Some(deck_id) => {
            let deck = crate::features::study::repository::StudyRepository::new(
                container.db_pool().clone(),
            )
            .get(deck_id)
            .await?;
            let due = deck.cards.iter().filter(|card| card.due_at <= now).count() as i64;
            (Some(deck), due)
        }
        None => (None, 0),
    };
    Ok(LearningMemoryDto {
        program_id: program_id.into(),
        journal_id: state.journal_id,
        lesson_notes,
        study_deck,
        drafts: state.drafts,
        accepted_cards: state.accepted_cards,
        due_count,
        scheduler_version: "expanding_v1".into(),
    })
}

#[tauri::command]
#[specta::specta]
pub async fn get_learning_memory(
    id: String,
    container: State<'_, Container>,
) -> Result<LearningMemoryDto, ApiError> {
    learning_memory(&container, &id)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn ensure_learning_lesson_note(
    request: EnsureLearningLessonNoteRequestDto,
    container: State<'_, Container>,
) -> Result<LearningMemoryDto, ApiError> {
    let repo = LearningRepository::new(container.db_pool().clone());
    let program = repo
        .get(&request.program_id)
        .await
        .map_err(ApiError::from)?;
    let lesson = program
        .modules
        .iter()
        .flat_map(|m| &m.lessons)
        .find(|l| l.id == request.lesson_id)
        .ok_or_else(|| {
            ApiError::from(crate::shared::error::AppError::NotFound(
                "Lesson does not belong to this learning program.".into(),
            ))
        })?;
    let mut content = "# Lesson notes\n\n## Sources\n".to_owned();
    for source in &program.sources {
        content.push_str(&format!(
            "- **{}** — {}\n",
            source.title,
            source.url.as_deref().unwrap_or("Selected document")
        ));
    }
    content.push_str("\n## Key ideas\n\n_Add notes in your own words._\n\n## Reflection\n\n");
    content.push_str(&format!(
        "How does this lesson help with the objective: {}?\n",
        lesson.objective
    ));
    let title = format!("{} — {}", program.summary.title, lesson.title);
    let (note_id, created) = repo
        .ensure_lesson_note(&request.program_id, &request.lesson_id, &title, &content)
        .await
        .map_err(ApiError::from)?;
    if created {
        crate::features::vault::writeback::spawn_sync_workspace_note(&container);
    }
    let _ = note_id;
    learning_memory(&container, &request.program_id)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn generate_learning_card_drafts(
    request: GenerateLearningCardDraftsRequestDto,
    container: State<'_, Container>,
) -> Result<LearningMemoryDto, ApiError> {
    let repo = LearningRepository::new(container.db_pool().clone());
    let program = repo
        .get(&request.program_id)
        .await
        .map_err(ApiError::from)?;
    let llm = container.get_or_load_llm().await.map_err(ApiError::from)?;
    let drafts = super::generation::generate_recall_drafts(
        llm.as_ref(),
        &program,
        &request.lesson_id,
        request.count,
    )
    .await
    .map_err(ApiError::from)?;
    repo.save_generated_drafts(&request.program_id, &request.lesson_id, &drafts)
        .await
        .map_err(ApiError::from)?;
    learning_memory(&container, &request.program_id)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn save_learning_card_draft(
    request: SaveLearningCardDraftRequestDto,
    container: State<'_, Container>,
) -> Result<LearningMemoryDto, ApiError> {
    for (value, name, max) in [
        (&request.question, "Question", 2000),
        (&request.answer, "Answer", 1000),
        (&request.explanation, "Explanation", 3000),
    ] {
        let n = value.trim().chars().count();
        if n == 0 || n > max {
            return Err(ApiError::from(
                crate::shared::error::AppError::InvalidInput(format!(
                    "{name} must contain 1–{max} characters."
                )),
            ));
        }
    }
    if request.source_ids.len() > 24 {
        return Err(ApiError::from(
            crate::shared::error::AppError::InvalidInput(
                "A card may cite no more than 24 sources.".into(),
            ),
        ));
    }
    let repo = LearningRepository::new(container.db_pool().clone());
    repo.save_manual_draft(&request)
        .await
        .map_err(ApiError::from)?;
    learning_memory(&container, &request.program_id)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn accept_learning_card_draft(
    request: LearningCardDraftActionRequestDto,
    container: State<'_, Container>,
) -> Result<LearningMemoryDto, ApiError> {
    LearningRepository::new(container.db_pool().clone())
        .accept_card_draft(&request.program_id, &request.draft_id)
        .await
        .map_err(ApiError::from)?;
    learning_memory(&container, &request.program_id)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn discard_learning_card_draft(
    request: LearningCardDraftActionRequestDto,
    container: State<'_, Container>,
) -> Result<LearningMemoryDto, ApiError> {
    LearningRepository::new(container.db_pool().clone())
        .discard_card_draft(&request.program_id, &request.draft_id)
        .await
        .map_err(ApiError::from)?;
    learning_memory(&container, &request.program_id)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn get_learning_canvas_workspace(
    id: String,
    container: State<'_, Container>,
) -> Result<LearningCanvasWorkspaceDto, ApiError> {
    super::canvas_repository::LearningCanvasRepository::new(container.db_pool().clone())
        .workspace(&id)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn create_learning_canvas(
    request: CreateLearningCanvasRequestDto,
    container: State<'_, Container>,
) -> Result<LearningCanvasWorkspaceDto, ApiError> {
    let scene =
        super::canvas_repository::validate_scene(&request.scene_json).map_err(ApiError::from)?;
    let repo = super::canvas_repository::LearningCanvasRepository::new(container.db_pool().clone());
    repo.create(&request, &scene)
        .await
        .map_err(ApiError::from)?;
    repo.workspace(&request.program_id)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn save_learning_canvas(
    request: SaveLearningCanvasRequestDto,
    container: State<'_, Container>,
) -> Result<LearningCanvasWorkspaceDto, ApiError> {
    let scene =
        super::canvas_repository::validate_scene(&request.scene_json).map_err(ApiError::from)?;
    let repo = super::canvas_repository::LearningCanvasRepository::new(container.db_pool().clone());
    repo.save(&request, &scene).await.map_err(ApiError::from)?;
    repo.workspace(&request.program_id)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn create_learning_canvas_snapshot(
    request: CreateLearningCanvasSnapshotRequestDto,
    container: State<'_, Container>,
) -> Result<LearningCanvasWorkspaceDto, ApiError> {
    let repo = super::canvas_repository::LearningCanvasRepository::new(container.db_pool().clone());
    repo.create_snapshot(&request)
        .await
        .map_err(ApiError::from)?;
    repo.workspace(&request.program_id)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn restore_learning_canvas_snapshot(
    request: RestoreLearningCanvasSnapshotRequestDto,
    container: State<'_, Container>,
) -> Result<LearningCanvasWorkspaceDto, ApiError> {
    let repo = super::canvas_repository::LearningCanvasRepository::new(container.db_pool().clone());
    repo.restore(&request).await.map_err(ApiError::from)?;
    repo.workspace(&request.program_id)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn get_learning_source_workspace(
    id: String,
    container: State<'_, Container>,
) -> Result<LearningSourceWorkspaceDto, ApiError> {
    source_library(&container)
        .workspace(&id)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn get_learning_source_version(
    request: GetLearningSourceVersionRequestDto,
    container: State<'_, Container>,
) -> Result<LearningSourceVersionDto, ApiError> {
    source_library(&container)
        .get_version(&request)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn search_learning_sources(
    request: SearchLearningSourcesRequestDto,
    container: State<'_, Container>,
) -> Result<Vec<LearningSourceSearchResultDto>, ApiError> {
    source_library(&container)
        .search(&request)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn add_learning_web_source(
    request: AddLearningWebSourceRequestDto,
    container: State<'_, Container>,
) -> Result<LearningSourceWorkspaceDto, ApiError> {
    validate_source_ids(
        &request.program_id,
        &request.source_id,
        &request.version_id,
        &request.operation_id,
    )
    .map_err(ApiError::from)?;
    let url = url::Url::parse(&request.url).map_err(|_| {
        ApiError::from(crate::shared::error::AppError::InvalidInput(
            "Source URL must be a valid HTTP or HTTPS address.".into(),
        ))
    })?;
    if request.url.chars().count() > 2048 {
        return Err(ApiError::from(
            crate::shared::error::AppError::InvalidInput(
                "Source URL must not exceed 2048 characters.".into(),
            ),
        ));
    }
    if !matches!(url.scheme(), "http" | "https") {
        return Err(ApiError::from(
            crate::shared::error::AppError::InvalidInput(
                "Source URL must use HTTP or HTTPS.".into(),
            ),
        ));
    }
    let requested_url = request.url.trim();
    let repo = source_library(&container);
    let hash = source_request_hash(&request).map_err(ApiError::from)?;
    if repo
        .preflight_replay(
            &request.operation_id,
            &request.program_id,
            &request.source_id,
            "add_web",
            &hash,
        )
        .await
        .map_err(ApiError::from)?
    {
        return repo
            .workspace(&request.program_id)
            .await
            .map_err(ApiError::from);
    }
    repo.preflight_new_source(&request.program_id, &request.source_id, &request.version_id)
        .await
        .map_err(ApiError::from)?;
    let safe_web = container.web_service();
    container
        .security_context()
        .rate_limiters()
        .web_ingest
        .check_rate_limit("learning_source_fetch")
        .await
        .map_err(|error| {
            ApiError::from(crate::shared::error::AppError::RateLimitExceeded(
                error.to_string(),
            ))
        })?;
    let article = safe_web
        .fetch_url_content(request.url.trim())
        .await
        .map_err(|error| {
            ApiError::from(crate::shared::error::AppError::Network(error.to_string()))
        })?;
    if article
        .title
        .as_deref()
        .unwrap_or_default()
        .trim()
        .is_empty()
        || article.content.trim().is_empty()
    {
        return Err(ApiError::from(
            crate::shared::error::AppError::InvalidInput(
                "The page did not contain usable article text.".into(),
            ),
        ));
    }
    let captured = super::source_library::CapturedLearningSource {
        title: article.title.unwrap_or_default(),
        publisher: None,
        requested_url: Some(requested_url.into()),
        resolved_url: Some(article.url),
        text: article.content,
        truncated: article.content_truncated,
        extraction_version: "web_article_v1".into(),
    };
    repo.add(
        &request.operation_id,
        &request.source_id,
        &request.version_id,
        &request.program_id,
        LearningSourceKind::Web,
        "add_web",
        requested_url,
        Some(requested_url),
        request.freshness_policy,
        captured,
        hash,
    )
    .await
    .map_err(ApiError::from)?;
    repo.workspace(&request.program_id)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn add_learning_document_source(
    request: AddLearningDocumentSourceRequestDto,
    container: State<'_, Container>,
) -> Result<LearningSourceWorkspaceDto, ApiError> {
    validate_source_ids(
        &request.program_id,
        &request.source_id,
        &request.version_id,
        &request.operation_id,
    )
    .map_err(ApiError::from)?;
    uuid::Uuid::parse_str(&request.document_id).map_err(|_| {
        ApiError::from(crate::shared::error::AppError::InvalidInput(
            "Invalid document ID".into(),
        ))
    })?;
    let repo = source_library(&container);
    let hash = source_request_hash(&request).map_err(ApiError::from)?;
    if repo
        .preflight_replay(
            &request.operation_id,
            &request.program_id,
            &request.source_id,
            "add_document",
            &hash,
        )
        .await
        .map_err(ApiError::from)?
    {
        return repo
            .workspace(&request.program_id)
            .await
            .map_err(ApiError::from);
    }
    repo.preflight_new_source(&request.program_id, &request.source_id, &request.version_id)
        .await
        .map_err(ApiError::from)?;
    let Some((title, chunks)) = repo
        .library_document_text(&request.document_id)
        .await
        .map_err(ApiError::from)?
    else {
        return Err(ApiError::from(
            crate::shared::error::AppError::InvalidInput(
                "The selected document has no indexed text yet. Let its import finish first."
                    .into(),
            ),
        ));
    };
    let mut text = String::new();
    let mut truncated = chunks.len() == super::source_library::LIBRARY_CAPTURE_CHUNKS;
    for chunk in &chunks {
        let remaining = 64_001_usize.saturating_sub(text.chars().count());
        if remaining == 0 {
            truncated = true;
            break;
        }
        if !text.is_empty() {
            text.push_str("\n\n");
        }
        let excerpt: String = chunk.chars().take(remaining).collect();
        if excerpt.chars().count() < chunk.chars().count() {
            truncated = true;
        }
        text.push_str(&excerpt);
        if text.chars().count() > 64_000 {
            truncated = true;
            break;
        }
    }
    let captured = super::source_library::CapturedLearningSource {
        title,
        publisher: None,
        requested_url: None,
        resolved_url: None,
        text,
        truncated,
        extraction_version: "document_chunks_v1".into(),
    };
    repo.add(
        &request.operation_id,
        &request.source_id,
        &request.version_id,
        &request.program_id,
        LearningSourceKind::Document,
        "add_document",
        &request.document_id,
        None,
        LearningSourcePolicy::Fixed,
        captured,
        hash,
    )
    .await
    .map_err(ApiError::from)?;
    repo.workspace(&request.program_id)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn add_learning_text_source(
    request: AddLearningTextSourceRequestDto,
    container: State<'_, Container>,
) -> Result<LearningSourceWorkspaceDto, ApiError> {
    validate_source_ids(
        &request.program_id,
        &request.source_id,
        &request.version_id,
        &request.operation_id,
    )
    .map_err(ApiError::from)?;
    let hash = source_request_hash(&request).map_err(ApiError::from)?;
    let captured = super::source_library::CapturedLearningSource {
        title: request.title.clone(),
        publisher: request.publisher.clone(),
        requested_url: None,
        resolved_url: None,
        text: request.text.clone(),
        truncated: false,
        extraction_version: "pasted_text_v1".into(),
    };
    source_library(&container)
        .add(
            &request.operation_id,
            &request.source_id,
            &request.version_id,
            &request.program_id,
            LearningSourceKind::Pasted,
            "add_text",
            &request.title,
            None,
            LearningSourcePolicy::Fixed,
            captured,
            hash,
        )
        .await
        .map_err(ApiError::from)?;
    source_library(&container)
        .workspace(&request.program_id)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn refresh_learning_source(
    request: RefreshLearningSourceRequestDto,
    container: State<'_, Container>,
) -> Result<LearningSourceWorkspaceDto, ApiError> {
    let repo = source_library(&container);
    let hash = source_request_hash(&request).map_err(ApiError::from)?;
    if repo
        .preflight_replay(
            &request.operation_id,
            &request.program_id,
            &request.source_id,
            "refresh",
            &hash,
        )
        .await
        .map_err(ApiError::from)?
    {
        return repo
            .workspace(&request.program_id)
            .await
            .map_err(ApiError::from);
    }
    let (revision, url, _) = repo
        .source_for_refresh(&request.program_id, &request.source_id)
        .await
        .map_err(ApiError::from)?;
    if revision != request.expected_revision {
        return Err(ApiError::from(
            crate::shared::error::AppError::InvalidInput(
                "Source changed; reload and retry.".into(),
            ),
        ));
    }
    let captured_result = async {
        container
            .security_context()
            .rate_limiters()
            .web_ingest
            .check_rate_limit("learning_source_refresh")
            .await
            .map_err(|e| crate::shared::error::AppError::RateLimitExceeded(e.to_string()))?;
        let article = container
            .web_service()
            .fetch_url_content(&url)
            .await
            .map_err(|e| crate::shared::error::AppError::Network(e.to_string()))?;
        Ok::<_, crate::shared::error::AppError>(super::source_library::CapturedLearningSource {
            title: article.title.unwrap_or_else(|| url.clone()),
            publisher: None,
            requested_url: Some(url.clone()),
            resolved_url: Some(article.url),
            text: article.content,
            truncated: article.content_truncated,
            extraction_version: "web_article_v1".into(),
        })
    }
    .await;
    let (captured, failure) = match captured_result {
        Ok(c) => (Some(c), None),
        Err(e) => (None, Some(e.to_string())),
    };
    repo.refresh(&request, captured, failure)
        .await
        .map_err(ApiError::from)?;
    repo.workspace(&request.program_id)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn adopt_learning_source_version(
    request: AdoptLearningSourceVersionRequestDto,
    container: State<'_, Container>,
) -> Result<LearningSourceWorkspaceDto, ApiError> {
    let repo = source_library(&container);
    repo.adopt(&request).await.map_err(ApiError::from)?;
    repo.workspace(&request.program_id)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn update_learning_source_policy(
    request: UpdateLearningSourcePolicyRequestDto,
    container: State<'_, Container>,
) -> Result<LearningSourceWorkspaceDto, ApiError> {
    let repo = source_library(&container);
    repo.update_policy(&request).await.map_err(ApiError::from)?;
    repo.workspace(&request.program_id)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn delete_learning_source(
    request: DeleteLearningSourceRequestDto,
    container: State<'_, Container>,
) -> Result<LearningSourceWorkspaceDto, ApiError> {
    source_library(&container)
        .delete_source(&request)
        .await
        .map_err(ApiError::from)?;
    source_library(&container)
        .workspace(&request.program_id)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn reimport_learning_source(
    request: ReimportLearningSourceRequestDto,
    container: State<'_, Container>,
) -> Result<LearningSourceWorkspaceDto, ApiError> {
    source_library(&container)
        .reimport_source(&request)
        .await
        .map_err(ApiError::from)?;
    source_library(&container)
        .workspace(&request.program_id)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn create_learning_source_selector(
    request: CreateLearningSourceSelectorRequestDto,
    container: State<'_, Container>,
) -> Result<LearningSourceSelectorDto, ApiError> {
    super::recall_repository::LearningRecallRepository::new(container.db_pool().clone())
        .create_selector(&request)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn get_learning_source_selector(
    program_id: String,
    selector_id: String,
    container: State<'_, Container>,
) -> Result<LearningSourceSelectorDto, ApiError> {
    super::recall_repository::LearningRecallRepository::new(container.db_pool().clone())
        .get_selector(&program_id, &selector_id)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn match_learning_source_selector(
    request: MatchLearningSourceSelectorRequestDto,
    container: State<'_, Container>,
) -> Result<super::source_selector::LearningQuoteMatch, ApiError> {
    super::recall_repository::LearningRecallRepository::new(container.db_pool().clone())
        .match_selector(&request)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn search_learning_sources_semantically(
    request: SearchLearningSourcesSemanticallyRequestDto,
    container: State<'_, Container>,
) -> Result<Vec<LearningSourceSemanticSearchResultDto>, ApiError> {
    let embedding = container.get_or_load_embedding().await.ok();
    super::recall_repository::LearningRecallRepository::new(container.db_pool().clone())
        .search_sources(&request, embedding.as_deref())
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn get_learning_recall_workspace(
    program_id: String,
    container: State<'_, Container>,
) -> Result<LearningRecallWorkspaceDto, ApiError> {
    super::recall_repository::LearningRecallRepository::new(container.db_pool().clone())
        .workspace(&program_id)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn save_learning_recall_card(
    request: SaveLearningRecallCardRequestDto,
    container: State<'_, Container>,
) -> Result<LearningRecallWorkspaceDto, ApiError> {
    super::recall_repository::LearningRecallRepository::new(container.db_pool().clone())
        .save_card(&request)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn decide_learning_recall_duplicate(
    request: DecideLearningRecallDuplicateRequestDto,
    container: State<'_, Container>,
) -> Result<LearningRecallWorkspaceDto, ApiError> {
    super::recall_repository::LearningRecallRepository::new(container.db_pool().clone())
        .decide_duplicate(&request)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn change_learning_recall_scheduler(
    request: ChangeLearningRecallSchedulerRequestDto,
    container: State<'_, Container>,
) -> Result<LearningRecallWorkspaceDto, ApiError> {
    super::recall_repository::LearningRecallRepository::new(container.db_pool().clone())
        .change_scheduler(&request)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn review_learning_recall_card(
    request: ReviewLearningRecallCardRequestDto,
    container: State<'_, Container>,
) -> Result<LearningRecallWorkspaceDto, ApiError> {
    super::recall_repository::LearningRecallRepository::new(container.db_pool().clone())
        .review_card(&request)
        .await
        .map_err(ApiError::from)
}

fn practice_repo(container: &Container) -> super::practice_repository::LearningPracticeRepository {
    super::practice_repository::LearningPracticeRepository::new(container.db_pool().clone())
}

fn practice_request_hash<T: serde::Serialize>(request: &T) -> crate::shared::error::Result<String> {
    use sha2::Digest;
    let mut value = serde_json::to_value(request)
        .map_err(|e| crate::shared::error::AppError::Serialization(e.to_string()))?;
    if let Some(fields) = value.as_object_mut() {
        fields.remove("operationId");
        fields.remove("expectedRevision");
        fields.remove("expectedProgramRevision");
    }
    let bytes = serde_json::to_vec(&value)
        .map_err(|e| crate::shared::error::AppError::Serialization(e.to_string()))?;
    Ok(format!("{:x}", sha2::Sha256::digest(bytes)))
}

#[tauri::command]
#[specta::specta]
pub async fn get_learning_practice_workspace(
    id: String,
    container: State<'_, Container>,
) -> Result<LearningPracticeWorkspaceDto, ApiError> {
    practice_repo(&container)
        .workspace(&id)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn get_learning_practice_session(
    id: String,
    container: State<'_, Container>,
) -> Result<LearningPracticeSessionDto, ApiError> {
    practice_repo(&container)
        .get_session(&id)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn start_learning_practice_session(
    request: StartLearningPracticeSessionRequestDto,
    container: State<'_, Container>,
) -> Result<LearningPracticeWorkspaceDto, ApiError> {
    let hash = practice_request_hash(&request).map_err(ApiError::from)?;
    practice_repo(&container)
        .start(&request, &hash)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn save_learning_practice_artifact(
    request: SaveLearningPracticeArtifactRequestDto,
    container: State<'_, Container>,
) -> Result<LearningPracticeWorkspaceDto, ApiError> {
    let hash = practice_request_hash(&request).map_err(ApiError::from)?;
    practice_repo(&container)
        .save_artifact(&request, &hash)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn change_learning_practice_mode(
    request: ChangeLearningPracticeModeRequestDto,
    container: State<'_, Container>,
) -> Result<LearningPracticeWorkspaceDto, ApiError> {
    let hash = practice_request_hash(&request).map_err(ApiError::from)?;
    practice_repo(&container)
        .change_mode(&request, &hash)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn open_learning_practice_source(
    request: OpenLearningPracticeSourceRequestDto,
    container: State<'_, Container>,
) -> Result<LearningPracticeWorkspaceDto, ApiError> {
    let hash = practice_request_hash(&request).map_err(ApiError::from)?;
    practice_repo(&container)
        .open_source(&request, &hash)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn request_learning_tutor_response(
    request: RequestLearningTutorResponseRequestDto,
    container: State<'_, Container>,
) -> Result<LearningPracticeWorkspaceDto, ApiError> {
    let repo = practice_repo(&container);
    let payload_hash = practice_request_hash(&request).map_err(ApiError::from)?;
    if repo
        .preflight_replay(
            &request.operation_id,
            &request.program_id,
            &request.session_id,
            "tutor",
            &payload_hash,
        )
        .await
        .map_err(ApiError::from)?
    {
        return repo
            .workspace(&request.program_id)
            .await
            .map_err(ApiError::from);
    }
    let session = repo
        .validate_live_session(
            &request.program_id,
            &request.session_id,
            request.expected_revision,
            false,
        )
        .await
        .map_err(ApiError::from)?;
    super::practice_repository::validate_tutor_bounds(&request.prompt, "placeholder response")
        .map_err(ApiError::from)?;
    match (&request.request_kind, &request.hint_level) {
        (LearningTutorRequestKind::Hint, None) => {
            return Err(ApiError::from(
                crate::shared::error::AppError::InvalidInput(
                    "Choose an explicit hint level.".into(),
                ),
            ));
        }
        (LearningTutorRequestKind::Hint, Some(LearningPracticeHintLevel::WorkedExplanation))
            if session.summary.mode == LearningPracticeMode::Practice =>
        {
            return Err(ApiError::from(
                crate::shared::error::AppError::InvalidInput(
                    "Worked explanations are available only in Explore mode.".into(),
                ),
            ));
        }
        (LearningTutorRequestKind::Question | LearningTutorRequestKind::Critique, Some(_)) => {
            return Err(ApiError::from(
                crate::shared::error::AppError::InvalidInput(
                    "Hint levels apply only to hint requests.".into(),
                ),
            ));
        }
        _ => {}
    }
    let sources = repo
        .frozen_sources(&request.program_id, &session.summary.source_version_ids)
        .await
        .map_err(ApiError::from)?;
    let llm = container.get_or_load_llm().await.map_err(ApiError::from)?;
    let generated = super::practice_generation::tutor(llm.as_ref(), &session, &request, &sources)
        .await
        .map_err(ApiError::from)?;
    let turn = super::practice_repository::TutorTurnWrite {
        id: uuid::Uuid::new_v4().to_string(),
        operation_id: request.operation_id.clone(),
        payload_hash,
        prompt: request.prompt.clone(),
        request_kind: request.request_kind.clone(),
        response: generated.response,
        hint_level: request.hint_level.clone(),
        citations: generated.citations,
        proposals: generated
            .proposals
            .into_iter()
            .map(|p| LearningPracticeProposalDto {
                id: uuid::Uuid::new_v4().to_string(),
                kind: p.kind,
                text: p.text,
                evidence_quote: p.evidence_quote,
                tutor_turn_id: None,
                status: LearningPracticeProposalStatus::Pending,
                created_at: chrono::Utc::now().timestamp_millis(),
                decided_at: None,
            })
            .collect(),
        model_name: llm.model_name().to_string(),
    };
    repo.commit_tutor_turn(
        &request.program_id,
        &request.session_id,
        request.expected_revision,
        turn,
    )
    .await
    .map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn reveal_learning_practice_solution(
    request: RevealLearningPracticeSolutionRequestDto,
    container: State<'_, Container>,
) -> Result<LearningPracticeWorkspaceDto, ApiError> {
    let repo = practice_repo(&container);
    let payload_hash = practice_request_hash(&request).map_err(ApiError::from)?;
    if repo
        .preflight_replay(
            &request.operation_id,
            &request.program_id,
            &request.session_id,
            "reveal_solution",
            &payload_hash,
        )
        .await
        .map_err(ApiError::from)?
    {
        return repo
            .workspace(&request.program_id)
            .await
            .map_err(ApiError::from);
    }
    let session = repo
        .validate_live_session(
            &request.program_id,
            &request.session_id,
            request.expected_revision,
            false,
        )
        .await
        .map_err(ApiError::from)?;
    if session.summary.mode != LearningPracticeMode::Explore {
        return Err(ApiError::from(
            crate::shared::error::AppError::InvalidInput(
                "Worked solutions are available only in Explore mode.".into(),
            ),
        ));
    }
    let sources = repo
        .frozen_sources(&request.program_id, &session.summary.source_version_ids)
        .await
        .map_err(ApiError::from)?;
    let llm = container.get_or_load_llm().await.map_err(ApiError::from)?;
    let generated = super::practice_generation::solution(llm.as_ref(), &session, &sources)
        .await
        .map_err(ApiError::from)?;
    repo.commit_solution(
        &request,
        &payload_hash,
        &generated.solution,
        &generated.citations,
    )
    .await
    .map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn submit_learning_practice_attempt(
    request: SubmitLearningPracticeAttemptRequestDto,
    container: State<'_, Container>,
) -> Result<LearningPracticeWorkspaceDto, ApiError> {
    let repo = practice_repo(&container);
    let payload_hash = practice_request_hash(&request).map_err(ApiError::from)?;
    if repo
        .preflight_replay(
            &request.operation_id,
            &request.program_id,
            &request.session_id,
            "submit",
            &payload_hash,
        )
        .await
        .map_err(ApiError::from)?
    {
        return repo
            .workspace(&request.program_id)
            .await
            .map_err(ApiError::from);
    }
    let session = repo
        .validate_live_session(
            &request.program_id,
            &request.session_id,
            request.expected_revision,
            true,
        )
        .await
        .map_err(ApiError::from)?;
    let llm = container.get_or_load_llm().await.map_err(ApiError::from)?;
    let (grade_status, criteria) = match super::practice_generation::grade(llm.as_ref(), &session)
        .await
    {
        Ok(value) => value,
        Err(_) => {
            let criteria = session
                .rubric
                .iter()
                .map(|c| LearningPracticeCriterionResultDto {
                    criterion_id: c.id.clone(),
                    dimension: c.dimension.clone(),
                    score: None,
                    max_points: c.max_points,
                    observation:
                        "Evidence could not be graded reliably; review this response with a person."
                            .into(),
                    evidence_quote: None,
                })
                .collect();
            (LearningPracticeGradeStatus::Uncertain, criteria)
        }
    };
    let mut assistance_kinds = Vec::new();
    for event in &session.assistance {
        if !assistance_kinds.contains(&event.kind) {
            assistance_kinds.push(event.kind.clone());
        }
    }
    let evidence = session
        .rubric
        .iter()
        .map(|c| {
            let r = criteria.iter().find(|r| r.criterion_id == c.id);
            LearningPracticeEvidenceEventDto {
                dimension: c.dimension.clone(),
                observed: r.and_then(|x| x.score).is_some_and(|score| score > 0),
                observation: r
                    .map(|x| x.observation.clone())
                    .unwrap_or_else(|| "No criterion-specific evidence was returned.".into()),
                evidence_quote: r.and_then(|x| x.evidence_quote.clone()),
                assistance_kinds: assistance_kinds.clone(),
            }
        })
        .collect();
    let write = super::practice_repository::SubmissionWrite {
        payload_hash,
        operation_id: request.operation_id.clone(),
        grade_status,
        criteria,
        evidence,
        grader_model: llm.model_name().to_string(),
    };
    repo.submit(&request, write, &session.rubric)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn accept_learning_practice_proposal(
    request: DecideLearningPracticeProposalRequestDto,
    container: State<'_, Container>,
) -> Result<LearningPracticeWorkspaceDto, ApiError> {
    let hash = practice_request_hash(&request).map_err(ApiError::from)?;
    practice_repo(&container)
        .decide_proposal(&request, true, &hash)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn reject_learning_practice_proposal(
    request: DecideLearningPracticeProposalRequestDto,
    container: State<'_, Container>,
) -> Result<LearningPracticeWorkspaceDto, ApiError> {
    let hash = practice_request_hash(&request).map_err(ApiError::from)?;
    practice_repo(&container)
        .decide_proposal(&request, false, &hash)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn get_learning_plan(
    id: String,
    container: State<'_, Container>,
) -> Result<LearningPlanDto, ApiError> {
    super::curriculum_repository::LearningCurriculumRepository::new(container.db_pool().clone())
        .plan(&id)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn preview_learning_curriculum_revision(
    request: PreviewLearningCurriculumRevisionRequestDto,
    container: State<'_, Container>,
) -> Result<LearningPlanDto, ApiError> {
    super::curriculum_repository::LearningCurriculumRepository::new(container.db_pool().clone())
        .preview(&request)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn accept_learning_curriculum_revision(
    request: LearningCurriculumRevisionActionRequestDto,
    container: State<'_, Container>,
) -> Result<LearningPlanDto, ApiError> {
    super::curriculum_repository::LearningCurriculumRepository::new(container.db_pool().clone())
        .accept_revision(&request)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn discard_learning_curriculum_revision(
    request: DiscardLearningCurriculumRevisionRequestDto,
    container: State<'_, Container>,
) -> Result<LearningPlanDto, ApiError> {
    super::curriculum_repository::LearningCurriculumRepository::new(container.db_pool().clone())
        .discard_revision(&request)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn get_learning_portability_workspace(
    program_id: String,
    container: State<'_, Container>,
) -> Result<LearningPortabilityWorkspaceDto, ApiError> {
    super::pack_repository::LearningPackRepository::new(container.db_pool().clone())
        .workspace(&program_id)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn export_learning_pack(
    request: ExportLearningPackRequestDto,
    container: State<'_, Container>,
) -> Result<LearningPortabilityWorkspaceDto, ApiError> {
    super::pack_repository::LearningPackRepository::new(container.db_pool().clone())
        .export(&container, &request)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn preview_learning_pack_import(
    request: PreviewLearningPackImportRequestDto,
    container: State<'_, Container>,
) -> Result<LearningPortabilityWorkspaceDto, ApiError> {
    super::pack_repository::LearningPackRepository::new(container.db_pool().clone())
        .preview(&request)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn apply_learning_pack_import(
    request: ApplyLearningPackImportRequestDto,
    container: State<'_, Container>,
) -> Result<LearningPortabilityWorkspaceDto, ApiError> {
    super::pack_repository::LearningPackRepository::new(container.db_pool().clone())
        .apply(&container, &request)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn cancel_learning_pack_import_preview(
    request: CancelLearningPackImportPreviewRequestDto,
    container: State<'_, Container>,
) -> Result<LearningPortabilityWorkspaceDto, ApiError> {
    super::pack_repository::LearningPackRepository::new(container.db_pool().clone())
        .cancel(&request)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn start_learning_diagnostic(
    request: StartLearningDiagnosticRequestDto,
    container: State<'_, Container>,
) -> Result<LearningDiagnosticAttemptDto, ApiError> {
    super::curriculum_repository::LearningCurriculumRepository::new(container.db_pool().clone())
        .start_diagnostic(&request)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn submit_learning_diagnostic(
    request: SubmitLearningDiagnosticRequestDto,
    container: State<'_, Container>,
) -> Result<LearningDiagnosticAttemptDto, ApiError> {
    super::curriculum_repository::LearningCurriculumRepository::new(container.db_pool().clone())
        .submit_diagnostic(&request)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn skip_learning_diagnostic(
    request: SkipLearningDiagnosticRequestDto,
    container: State<'_, Container>,
) -> Result<LearningDiagnosticAttemptDto, ApiError> {
    super::curriculum_repository::LearningCurriculumRepository::new(container.db_pool().clone())
        .skip_diagnostic(&request)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn start_learning_generation_job(
    request: StartLearningGenerationJobRequestDto,
    container: State<'_, Container>,
) -> Result<LearningGenerationJob, ApiError> {
    let repo = super::curriculum_repository::LearningCurriculumRepository::new(
        container.db_pool().clone(),
    );
    let job = repo.start_job(&request).await.map_err(ApiError::from)?;
    let owned = container.inner().clone();
    let id = job.id.clone();
    tauri::async_runtime::spawn(async move {
        run_generation_job(owned, id).await;
    });
    Ok(job)
}

#[tauri::command]
#[specta::specta]
pub async fn cancel_learning_generation_job(
    request: LearningGenerationJobActionRequestDto,
    container: State<'_, Container>,
) -> Result<LearningGenerationJob, ApiError> {
    super::curriculum_repository::LearningCurriculumRepository::new(container.db_pool().clone())
        .cancel_job(&request)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn retry_learning_generation_job(
    request: LearningGenerationJobActionRequestDto,
    container: State<'_, Container>,
) -> Result<LearningGenerationJob, ApiError> {
    let repo = super::curriculum_repository::LearningCurriculumRepository::new(
        container.db_pool().clone(),
    );
    let job = repo.retry_job(&request).await.map_err(ApiError::from)?;
    let owned = container.inner().clone();
    let id = job.id.clone();
    tauri::async_runtime::spawn(async move {
        run_generation_job(owned, id).await;
    });
    Ok(job)
}

#[tauri::command]
#[specta::specta]
pub async fn get_learning_generation_job(
    id: String,
    container: State<'_, Container>,
) -> Result<LearningGenerationJob, ApiError> {
    super::curriculum_repository::LearningCurriculumRepository::new(container.db_pool().clone())
        .job(&id)
        .await
        .map_err(ApiError::from)
}

async fn run_generation_job(container: Container, job_id: String) {
    let repo = super::curriculum_repository::LearningCurriculumRepository::new(
        container.db_pool().clone(),
    );
    if let Err(error) = run_generation_job_inner(&container, &repo, &job_id).await {
        let _ = repo.fail_job(&job_id, &error.to_string()).await;
    }
}

async fn run_generation_job_inner(
    container: &Container,
    repo: &super::curriculum_repository::LearningCurriculumRepository,
    job_id: &str,
) -> crate::shared::error::Result<()> {
    use super::curriculum::LearningGenerationJobKind;
    if !repo.begin_job(job_id).await? {
        return Ok(());
    }
    let job = repo.job(job_id).await?;
    if job.kind != LearningGenerationJobKind::LessonPreparation {
        return Err(crate::shared::error::AppError::InvalidInput(
            "This worker currently accepts bounded lesson-preparation jobs only.".into(),
        ));
    }
    let body: serde_json::Value = serde_json::from_str(&repo.job_request_json(job_id).await?)
        .map_err(|e| crate::shared::error::AppError::Serialization(e.to_string()))?;
    let requested_ids = body
        .get("lessonIds")
        .and_then(serde_json::Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(serde_json::Value::as_str)
                .map(str::to_string)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let program_repo = LearningRepository::new(container.db_pool().clone());
    let mut program = program_repo.get(&job.program_id).await?;
    if program.summary.revision as u32 != job.base_revision_number {
        return Err(crate::shared::error::AppError::InvalidState(
            "Program changed before generation started.".into(),
        ));
    }
    if program.summary.status != LearningProgramStatus::Active {
        return Err(crate::shared::error::AppError::InvalidState(
            "Only active programs can prepare lessons.".into(),
        ));
    }
    ensure_before_use_sources(container, &job.program_id).await?;
    program.sources = program_repo.active_sources(&job.program_id).await?;
    if program.sources.is_empty() {
        return Err(crate::shared::error::AppError::InvalidState(
            "No active source snapshots are available for lesson preparation.".into(),
        ));
    }
    let mut candidates = program
        .modules
        .iter()
        .flat_map(|m| m.lessons.iter())
        .filter(|lesson| lesson.preparation == LearningPreparation::Outline && !lesson.completed)
        .collect::<Vec<_>>();
    if !requested_ids.is_empty() {
        if requested_ids.len() > 3 {
            return Err(crate::shared::error::AppError::InvalidInput(
                "A preparation job may target at most three upcoming lessons.".into(),
            ));
        }
        candidates.retain(|lesson| requested_ids.contains(&lesson.id));
        if candidates.len() != requested_ids.len() {
            return Err(crate::shared::error::AppError::InvalidInput(
                "A requested lesson is not an upcoming outline in this program.".into(),
            ));
        }
    } else {
        candidates.truncate(job.progress_total as usize);
    }
    if candidates.is_empty() || candidates.len() != job.progress_total as usize {
        return Err(crate::shared::error::AppError::InvalidInput(
            "The job must target one to three outline lessons.".into(),
        ));
    }
    let llm = container.get_or_load_llm().await?;
    let mut staged = Vec::new();
    for (index, lesson) in candidates.into_iter().enumerate() {
        let generated = tokio::time::timeout(
            std::time::Duration::from_secs(180),
            super::generation::prepare_lesson(llm.as_ref(), &program, lesson),
        )
        .await
        .map_err(|_| {
            crate::shared::error::AppError::ServiceNotAvailable(
                "Lesson preparation timed out.".into(),
            )
        })??;
        service::validate_prepared(&generated, &program)?;
        staged.push((lesson.id.clone(), generated));
        if !repo
            .advance_job(
                job_id,
                index as u32 + 1,
                &format!("Prepared {} of {} lessons", index + 1, job.progress_total),
            )
            .await?
        {
            return Ok(());
        }
    }
    repo.publish_prepared_lessons(job_id, program.summary.revision, &staged)
        .await
}

fn assessment_repo(
    container: &Container,
) -> super::assessment_repository::LearningAssessmentRepository {
    super::assessment_repository::LearningAssessmentRepository::new(container.db_pool().clone())
}

#[tauri::command]
#[specta::specta]
pub async fn get_learning_assessment_workspace(
    id: String,
    container: State<'_, Container>,
) -> Result<LearningAssessmentWorkspaceDto, ApiError> {
    assessment_repo(&container)
        .workspace(&id)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn create_learning_assessment_blueprint(
    request: CreateLearningAssessmentBlueprintRequestDto,
    container: State<'_, Container>,
) -> Result<LearningAssessmentWorkspaceDto, ApiError> {
    let repo = assessment_repo(&container);
    let hash = source_request_hash(&request).map_err(ApiError::from)?;
    if let Some(workspace) = repo
        .preflight_blueprint(&request, &hash)
        .await
        .map_err(ApiError::from)?
    {
        return Ok(workspace);
    }
    let assessment_workspace = repo
        .workspace(&request.program_id)
        .await
        .map_err(ApiError::from)?;
    let source_workspace = source_library(&container)
        .workspace(&request.program_id)
        .await
        .map_err(ApiError::from)?;
    let library =
        super::source_library::LearningSourceLibraryRepository::new(container.db_pool().clone());
    let mut sources = Vec::new();
    for version_id in &request.source_version_ids {
        let source = source_workspace
            .sources
            .iter()
            .find(|s| s.versions.iter().any(|v| &v.id == version_id))
            .ok_or_else(|| {
                ApiError::from(crate::shared::error::AppError::InvalidInput(
                    "Assessment source is not part of this program.".into(),
                ))
            })?;
        sources.push(
            library
                .get_version(&GetLearningSourceVersionRequestDto {
                    program_id: request.program_id.clone(),
                    source_id: source.id.clone(),
                    version_id: version_id.clone(),
                })
                .await
                .map_err(ApiError::from)?,
        );
    }
    let llm = container.get_or_load_llm().await.map_err(ApiError::from)?;
    let candidates = super::assessment_generation::generate(
        llm.as_ref(),
        &request,
        &assessment_workspace.outcomes,
        &sources,
    )
    .await
    .map_err(ApiError::from)?;
    repo.create_blueprint(&request, &candidates, llm.model_name(), &hash)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn start_learning_assessment_form(
    request: StartLearningAssessmentFormRequestDto,
    container: State<'_, Container>,
) -> Result<LearningAssessmentFormDto, ApiError> {
    let hash = source_request_hash(&request).map_err(ApiError::from)?;
    assessment_repo(&container)
        .start_form(&request, &hash)
        .await
        .map_err(ApiError::from)
}
#[tauri::command]
#[specta::specta]
pub async fn get_learning_assessment_form(
    id: String,
    container: State<'_, Container>,
) -> Result<LearningAssessmentFormDto, ApiError> {
    assessment_repo(&container)
        .get_form(&id)
        .await
        .map_err(ApiError::from)
}
#[tauri::command]
#[specta::specta]
pub async fn save_learning_assessment_response(
    request: SaveLearningAssessmentResponseRequestDto,
    container: State<'_, Container>,
) -> Result<LearningAssessmentFormDto, ApiError> {
    let hash = source_request_hash(&request).map_err(ApiError::from)?;
    assessment_repo(&container)
        .save_response(&request, &hash)
        .await
        .map_err(ApiError::from)
}
#[tauri::command]
#[specta::specta]
pub async fn interrupt_learning_assessment_form(
    request: MutateLearningAssessmentFormRequestDto,
    container: State<'_, Container>,
) -> Result<LearningAssessmentFormDto, ApiError> {
    let hash = source_request_hash(&request).map_err(ApiError::from)?;
    assessment_repo(&container)
        .interrupt(&request, &hash)
        .await
        .map_err(ApiError::from)
}
#[tauri::command]
#[specta::specta]
pub async fn submit_learning_assessment_form(
    request: MutateLearningAssessmentFormRequestDto,
    container: State<'_, Container>,
) -> Result<LearningAssessmentFormDto, ApiError> {
    let repo = assessment_repo(&container);
    let hash = source_request_hash(&request).map_err(ApiError::from)?;
    if let Some(form) = repo
        .preflight_submit(&request, &hash)
        .await
        .map_err(ApiError::from)?
    {
        return Ok(form);
    }
    let form = repo
        .get_form(&request.form_id)
        .await
        .map_err(ApiError::from)?;
    if form.items.iter().any(|item| {
        item.selected_index.is_none()
            && item
                .text_response
                .as_deref()
                .is_none_or(|s| s.trim().is_empty())
            && item.artifact_json.is_none()
            && item.ordered_values.is_empty()
    }) {
        return Err(ApiError::from(
            crate::shared::error::AppError::InvalidInput(
                "Save a response for every assessment item before submitting.".into(),
            ),
        ));
    }
    let (graded, model) = if form.items.iter().any(|i| {
        matches!(
            i.format,
            LearningItemFormat::Explanation | LearningItemFormat::Artifact
        )
    }) {
        let llm = container.get_or_load_llm().await.map_err(ApiError::from)?;
        match super::assessment_generation::grade_open(llm.as_ref(), &form).await {
            Ok(v) => (v.0, Some(v.1)),
            Err(_) => {
                let mut out = std::collections::HashMap::new();
                for item in form.items.iter().filter(|i| {
                    matches!(
                        i.format,
                        LearningItemFormat::Explanation | LearningItemFormat::Artifact
                    )
                }) {
                    out.insert(item.id.clone(),(LearningAssessmentGradeStatus::Uncertain,item.rubric.iter().map(|c|LearningAssessmentCriterionResultDto{criterion_id:c.id.clone(),score:None,max_points:c.max_points,observation:"Response could not be graded reliably; review with a person.".into(),artifact_quote:None}).collect()));
                }
                (out, Some(llm.model_name().into()))
            }
        }
    } else {
        (std::collections::HashMap::new(), None)
    };
    repo.submit(&request, &hash, graded, model)
        .await
        .map_err(ApiError::from)
}
#[tauri::command]
#[specta::specta]
pub async fn accept_learning_follow_up(
    request: DecideLearningFollowUpRequestDto,
    container: State<'_, Container>,
) -> Result<LearningAssessmentWorkspaceDto, ApiError> {
    let hash = source_request_hash(&(&request, true)).map_err(ApiError::from)?;
    assessment_repo(&container)
        .decide_follow_up(&request, true, &hash)
        .await
        .map_err(ApiError::from)
}
#[tauri::command]
#[specta::specta]
pub async fn dismiss_learning_follow_up(
    request: DecideLearningFollowUpRequestDto,
    container: State<'_, Container>,
) -> Result<LearningAssessmentWorkspaceDto, ApiError> {
    let hash = source_request_hash(&(&request, false)).map_err(ApiError::from)?;
    assessment_repo(&container)
        .decide_follow_up(&request, false, &hash)
        .await
        .map_err(ApiError::from)
}

fn practical_repo(
    container: &Container,
) -> super::practical_repository::LearningPracticalRepository {
    super::practical_repository::LearningPracticalRepository::new(container.db_pool().clone())
}

async fn practical_source_versions(
    container: &Container,
    program_id: &str,
    version_ids: &[String],
) -> crate::shared::error::Result<Vec<LearningSourceVersionDto>> {
    let library = source_library(container);
    let workspace = library.workspace(program_id).await?;
    let mut sources = Vec::with_capacity(version_ids.len());
    for version_id in version_ids {
        let source = workspace
            .sources
            .iter()
            .find(|source| {
                source
                    .versions
                    .iter()
                    .any(|version| &version.id == version_id)
            })
            .ok_or_else(|| {
                crate::shared::error::AppError::InvalidInput(
                    "A practical source version is outside this program.".into(),
                )
            })?;
        sources.push(
            library
                .get_version(&GetLearningSourceVersionRequestDto {
                    program_id: program_id.into(),
                    source_id: source.id.clone(),
                    version_id: version_id.clone(),
                })
                .await?,
        );
    }
    Ok(sources)
}

#[tauri::command]
#[specta::specta]
pub async fn get_learning_practical_workspace(
    program_id: String,
    container: State<'_, Container>,
) -> Result<LearningPracticalWorkspaceDto, ApiError> {
    practical_repo(&container)
        .workspace(&program_id)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn get_learning_practical_draft(
    request: GetLearningPracticalDraftRequestDto,
    container: State<'_, Container>,
) -> Result<LearningPracticalDraftDto, ApiError> {
    practical_repo(&container)
        .get_draft(&request)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn save_learning_practical_draft(
    request: SaveLearningPracticalDraftRequestDto,
    container: State<'_, Container>,
) -> Result<LearningPracticalDraftDto, ApiError> {
    let hash = source_request_hash(&request).map_err(ApiError::from)?;
    practical_repo(&container)
        .save_draft(&request, &hash)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub fn get_learning_runtime_catalog() -> Vec<super::runtime_catalog::LearningRuntimePresetDto> {
    super::runtime_catalog::learning_runtime_catalog()
}

#[tauri::command]
#[specta::specta]
pub async fn prepare_learning_runtime_preset(
    request: PrepareLearningRuntimePresetRequestDto,
    container: State<'_, Container>,
) -> Result<LearningPracticalWorkspaceDto, ApiError> {
    let hash =
        source_request_hash(&("prepare_runtime_preset", &request)).map_err(ApiError::from)?;
    let repo = practical_repo(&container);
    if let Some(workspace) = repo
        .preflight_runtime_setup(&request, &hash)
        .await
        .map_err(ApiError::from)?
    {
        return Ok(workspace);
    }
    let prepared = super::runtime_catalog::prepare_container_preset(request.engine, request.preset)
        .await
        .map_err(ApiError::from)?;
    repo.save_runtime_profile(
        &SaveLearningRuntimeProfileRequestDto {
            operation_id: request.operation_id,
            program_id: request.program_id,
            profile_id: request.profile_id,
            expected_revision: None,
            name: prepared.preset.name,
            engine: request.engine,
            image_id: prepared.image_id,
            command: prepared.preset.command,
            limits: prepared.preset.limits,
        },
        &hash,
    )
    .await
    .map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn save_learning_runtime_profile(
    request: SaveLearningRuntimeProfileRequestDto,
    container: State<'_, Container>,
) -> Result<LearningPracticalWorkspaceDto, ApiError> {
    let hash = source_request_hash(&request).map_err(ApiError::from)?;
    practical_repo(&container)
        .save_runtime_profile(&request, &hash)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn generate_learning_practical_activity(
    request: GenerateLearningPracticalActivityRequestDto,
    container: State<'_, Container>,
) -> Result<LearningPracticalWorkspaceDto, ApiError> {
    let hash = source_request_hash(&request).map_err(ApiError::from)?;
    let repo = practical_repo(&container);
    if let Some(replayed) = repo
        .replay_activity_operation(&request.program_id, &request.operation_id, &hash)
        .await
        .map_err(ApiError::from)?
    {
        return Ok(replayed);
    }
    let program = LearningRepository::new(container.db_pool().clone())
        .get(&request.program_id)
        .await
        .map_err(ApiError::from)?;
    if program.summary.revision != request.expected_program_revision
        || program.summary.status != LearningProgramStatus::Active
    {
        return Err(ApiError::from(
            crate::shared::error::AppError::InvalidInput(
                "The learning program changed; reload before generating this activity.".into(),
            ),
        ));
    }
    let lesson = program
        .modules
        .iter()
        .flat_map(|module| module.lessons.iter())
        .find(|lesson| lesson.id == request.lesson_id)
        .cloned()
        .ok_or_else(|| {
            ApiError::from(crate::shared::error::AppError::NotFound(
                "Learning lesson not found".into(),
            ))
        })?;
    let source_workspace = source_library(&container)
        .workspace(&request.program_id)
        .await
        .map_err(ApiError::from)?;
    let source_ids = source_workspace
        .sources
        .iter()
        .filter_map(|source| source.active_version_id.clone())
        .take(12)
        .collect::<Vec<_>>();
    if source_ids.is_empty() {
        return Err(ApiError::from(
            crate::shared::error::AppError::InvalidInput(
                "Add or adopt at least one source before generating a practical activity.".into(),
            ),
        ));
    }
    let sources = practical_source_versions(&container, &request.program_id, &source_ids)
        .await
        .map_err(ApiError::from)?;
    let runtime = repo
        .runtime_generation_context(
            request.runtime_profile_id.as_deref(),
            request.builtin_runtime,
        )
        .await
        .map_err(ApiError::from)?;
    let llm = container.get_or_load_llm().await.map_err(ApiError::from)?;
    let generated = super::practical_generation::generate_activity(
        llm.as_ref(),
        request.kind,
        &request.learner_brief,
        &lesson,
        &sources,
        runtime.as_ref(),
    )
    .await
    .map_err(ApiError::from)?;
    repo.save_generated_activity(&request, &generated, llm.model_name(), &hash)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn start_learning_practical_run(
    request: StartLearningPracticalRunRequestDto,
    container: State<'_, Container>,
) -> Result<LearningPracticalRunDto, ApiError> {
    let hash = source_request_hash(&request).map_err(ApiError::from)?;
    practical_repo(&container)
        .start_run(&request, &hash)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn cancel_learning_practical_run(
    request: CancelLearningPracticalRunRequestDto,
    container: State<'_, Container>,
) -> Result<LearningPracticalRunDto, ApiError> {
    let hash = source_request_hash(&request).map_err(ApiError::from)?;
    practical_repo(&container)
        .cancel_run(&request, &hash)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn start_learning_simulation(
    request: StartLearningSimulationRequestDto,
    container: State<'_, Container>,
) -> Result<LearningSimulationSessionDto, ApiError> {
    let hash = source_request_hash(&request).map_err(ApiError::from)?;
    practical_repo(&container)
        .start_simulation(&request, &hash)
        .await
        .map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn send_learning_simulation_turn(
    request: SendLearningSimulationTurnRequestDto,
    container: State<'_, Container>,
) -> Result<LearningSimulationSessionDto, ApiError> {
    let hash = source_request_hash(&request).map_err(ApiError::from)?;
    let repo = practical_repo(&container);
    if let Some(replayed) = repo
        .replay_simulation_operation(
            &request.program_id,
            &request.session_id,
            &request.operation_id,
            &hash,
        )
        .await
        .map_err(ApiError::from)?
    {
        return Ok(replayed);
    }
    let mut session = repo
        .simulation(&request.session_id)
        .await
        .map_err(ApiError::from)?;
    if session.program_id != request.program_id || session.revision != request.expected_revision {
        return Err(ApiError::from(
            crate::shared::error::AppError::InvalidInput(
                "Simulation changed; reload and retry.".into(),
            ),
        ));
    }
    let (activity, source_ids) = repo
        .simulation_generation_context(&request.program_id, &session.activity_id)
        .await
        .map_err(ApiError::from)?;
    session.turns.push(LearningSimulationTurnDto {
        id: uuid::Uuid::new_v4().to_string(),
        ordinal: session.turns.len() as i64,
        speaker: LearningSimulationSpeaker::Learner,
        content: request.content.clone(),
        citations: vec![],
        model_name: None,
        created_at: chrono::Utc::now().timestamp_millis(),
    });
    let sources = practical_source_versions(&container, &request.program_id, &source_ids)
        .await
        .map_err(ApiError::from)?;
    let llm = container.get_or_load_llm().await.map_err(ApiError::from)?;
    let reply = super::practical_generation::generate_simulation_turn(
        llm.as_ref(),
        &activity,
        &session,
        &sources,
        false,
    )
    .await
    .map_err(ApiError::from)?;
    repo.append_simulation_turn(
        &request,
        &super::practical_repository::SimulationTurnWrite {
            content: reply.content,
            citations: reply.citations,
            model_name: llm.model_name().into(),
        },
        &hash,
    )
    .await
    .map_err(ApiError::from)
}

#[tauri::command]
#[specta::specta]
pub async fn finish_learning_simulation(
    request: FinishLearningSimulationRequestDto,
    container: State<'_, Container>,
) -> Result<LearningSimulationSessionDto, ApiError> {
    let hash = source_request_hash(&request).map_err(ApiError::from)?;
    let repo = practical_repo(&container);
    if let Some(replayed) = repo
        .replay_simulation_operation(
            &request.program_id,
            &request.session_id,
            &request.operation_id,
            &hash,
        )
        .await
        .map_err(ApiError::from)?
    {
        return Ok(replayed);
    }
    let session = repo
        .simulation(&request.session_id)
        .await
        .map_err(ApiError::from)?;
    if session.program_id != request.program_id || session.revision != request.expected_revision {
        return Err(ApiError::from(
            crate::shared::error::AppError::InvalidInput(
                "Simulation changed; reload and retry.".into(),
            ),
        ));
    }
    let (activity, source_ids) = repo
        .simulation_generation_context(&request.program_id, &session.activity_id)
        .await
        .map_err(ApiError::from)?;
    let sources = practical_source_versions(&container, &request.program_id, &source_ids)
        .await
        .map_err(ApiError::from)?;
    let llm = container.get_or_load_llm().await.map_err(ApiError::from)?;
    let feedback = super::practical_generation::generate_simulation_turn(
        llm.as_ref(),
        &activity,
        &session,
        &sources,
        true,
    )
    .await
    .map_err(ApiError::from)?;
    repo.finish_simulation(
        &request,
        &super::practical_repository::SimulationTurnWrite {
            content: feedback.content,
            citations: feedback.citations,
            model_name: llm.model_name().into(),
        },
        &hash,
    )
    .await
    .map_err(ApiError::from)
}

pub fn init<R: Runtime>() -> TauriPlugin<R> {
    Builder::new("learning")
        .setup(|app, _api| {
            let container = app.state::<Container>().inner().clone();
            let repo = super::curriculum_repository::LearningCurriculumRepository::new(
                container.db_pool().clone(),
            );
            tauri::async_runtime::block_on(repo.recover_running_jobs())?;
            let practical = super::practical_repository::LearningPracticalRepository::new(
                container.db_pool().clone(),
            );
            tauri::async_runtime::block_on(practical.recover_running_runs())?;
            let pending = tauri::async_runtime::block_on(repo.pending_jobs())?;
            for job in pending {
                let owned = container.clone();
                let id = job.id;
                tauri::async_runtime::spawn(async move {
                    run_generation_job(owned, id).await;
                });
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_learning_plan,
            get_learning_portability_workspace,
            export_learning_pack,
            preview_learning_pack_import,
            apply_learning_pack_import,
            cancel_learning_pack_import_preview,
            preview_learning_curriculum_revision,
            accept_learning_curriculum_revision,
            discard_learning_curriculum_revision,
            start_learning_diagnostic,
            submit_learning_diagnostic,
            skip_learning_diagnostic,
            start_learning_generation_job,
            cancel_learning_generation_job,
            retry_learning_generation_job,
            get_learning_generation_job,
            list_learning_programs,
            get_learning_program,
            generate_learning_program,
            accept_learning_program,
            prepare_learning_lesson,
            complete_learning_lesson,
            submit_learning_attempt,
            delete_learning_program,
            get_learning_memory,
            ensure_learning_lesson_note,
            generate_learning_card_drafts,
            save_learning_card_draft,
            accept_learning_card_draft,
            discard_learning_card_draft,
            get_learning_canvas_workspace,
            create_learning_canvas,
            save_learning_canvas,
            create_learning_canvas_snapshot,
            restore_learning_canvas_snapshot,
            get_learning_source_workspace,
            get_learning_source_version,
            search_learning_sources,
            add_learning_web_source,
            add_learning_document_source,
            add_learning_text_source,
            refresh_learning_source,
            adopt_learning_source_version,
            update_learning_source_policy,
            delete_learning_source,
            reimport_learning_source,
            create_learning_source_selector,
            get_learning_source_selector,
            match_learning_source_selector,
            search_learning_sources_semantically,
            get_learning_recall_workspace,
            save_learning_recall_card,
            decide_learning_recall_duplicate,
            change_learning_recall_scheduler,
            review_learning_recall_card,
            get_learning_practice_workspace,
            get_learning_practice_session,
            start_learning_practice_session,
            save_learning_practice_artifact,
            change_learning_practice_mode,
            open_learning_practice_source,
            request_learning_tutor_response,
            reveal_learning_practice_solution,
            submit_learning_practice_attempt,
            accept_learning_practice_proposal,
            reject_learning_practice_proposal,
            get_learning_assessment_workspace,
            create_learning_assessment_blueprint,
            start_learning_assessment_form,
            get_learning_assessment_form,
            save_learning_assessment_response,
            interrupt_learning_assessment_form,
            submit_learning_assessment_form,
            accept_learning_follow_up,
            dismiss_learning_follow_up,
            get_learning_practical_workspace,
            get_learning_practical_draft,
            save_learning_practical_draft,
            get_learning_runtime_catalog,
            prepare_learning_runtime_preset,
            save_learning_runtime_profile,
            generate_learning_practical_activity,
            start_learning_practical_run,
            cancel_learning_practical_run,
            start_learning_simulation,
            send_learning_simulation_turn,
            finish_learning_simulation
        ])
        .build()
}
