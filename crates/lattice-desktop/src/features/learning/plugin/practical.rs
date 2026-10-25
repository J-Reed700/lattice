use super::*;

pub(super) fn practical_repo(
    container: &Container,
) -> crate::features::learning::practical_repository::LearningPracticalRepository {
    crate::features::learning::practical_repository::LearningPracticalRepository::new(
        container.db_pool().clone(),
    )
}

pub(super) async fn practical_source_versions(
    container: &Container,
    program_id: &str,
    version_ids: &[String],
) -> lattice_core::shared::error::Result<Vec<LearningSourceVersionDto>> {
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
                lattice_core::shared::error::AppError::InvalidInput(
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
    runtimes: State<'_, BuiltinRuntimes>,
) -> Result<LearningPracticalWorkspaceDto, ApiError> {
    let workspace = practical_repo(&container)
        .workspace(&program_id)
        .await
        .map_err(ApiError::from)?;
    Ok(workspace.resolve_runtime(&runtimes).await)
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
pub fn get_learning_runtime_catalog(
) -> Vec<crate::features::learning::runtime_catalog::LearningRuntimePresetDto> {
    crate::features::learning::runtime_catalog::learning_runtime_catalog()
}

#[tauri::command]
#[specta::specta]
pub async fn prepare_learning_runtime_preset(
    request: PrepareLearningRuntimePresetRequestDto,
    container: State<'_, Container>,
    runtimes: State<'_, BuiltinRuntimes>,
) -> Result<LearningPracticalWorkspaceDto, ApiError> {
    let hash =
        source_request_hash(&("prepare_runtime_preset", &request)).map_err(ApiError::from)?;
    let repo = practical_repo(&container);
    if let Some(workspace) = repo
        .preflight_runtime_setup(&request, &hash)
        .await
        .map_err(ApiError::from)?
    {
        return Ok(workspace.resolve_runtime(&runtimes).await);
    }
    let prepared = crate::features::learning::runtime_catalog::prepare_container_preset(
        request.engine,
        request.preset,
    )
    .await
    .map_err(ApiError::from)?;
    let workspace = repo
        .save_runtime_profile(
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
        .map_err(ApiError::from)?;
    Ok(workspace.resolve_runtime(&runtimes).await)
}

#[tauri::command]
#[specta::specta]
pub async fn save_learning_runtime_profile(
    request: SaveLearningRuntimeProfileRequestDto,
    container: State<'_, Container>,
    runtimes: State<'_, BuiltinRuntimes>,
) -> Result<LearningPracticalWorkspaceDto, ApiError> {
    let hash = source_request_hash(&request).map_err(ApiError::from)?;
    let workspace = practical_repo(&container)
        .save_runtime_profile(&request, &hash)
        .await
        .map_err(ApiError::from)?;
    Ok(workspace.resolve_runtime(&runtimes).await)
}

#[tauri::command]
#[specta::specta]
pub async fn generate_learning_practical_activity(
    request: GenerateLearningPracticalActivityRequestDto,
    container: State<'_, Container>,
    runtimes: State<'_, BuiltinRuntimes>,
) -> Result<LearningPracticalWorkspaceDto, ApiError> {
    let hash = source_request_hash(&request).map_err(ApiError::from)?;
    let repo = practical_repo(&container);
    if let Some(replayed) = repo
        .replay_activity_operation(
            &request.program_id,
            &request.activity_id,
            &request.operation_id,
            &hash,
        )
        .await
        .map_err(ApiError::from)?
    {
        return Ok(replayed.resolve_runtime(&runtimes).await);
    }
    for (value, label) in [
        (&request.operation_id, "operation"),
        (&request.activity_id, "activity"),
        (&request.lesson_id, "lesson"),
    ] {
        if uuid::Uuid::parse_str(value).is_err() {
            return Err(ApiError::from(
                lattice_core::shared::error::AppError::InvalidInput(format!("Invalid {label} ID.")),
            ));
        }
    }
    if request.allowed_aids.len() > 24
        || request
            .allowed_aids
            .iter()
            .any(|aid| aid.trim().is_empty() || aid.chars().count() > 160)
    {
        return Err(ApiError::from(
            lattice_core::shared::error::AppError::InvalidInput(
                "Practical aid descriptions must be bounded.".into(),
            ),
        ));
    }
    if request.learner_brief.trim().is_empty() || request.learner_brief.chars().count() > 4_000 {
        return Err(ApiError::from(
            lattice_core::shared::error::AppError::InvalidInput(
                "Practical request must contain 1–4000 characters.".into(),
            ),
        ));
    }
    let interview_target = if request.kind == LearningPracticalActivityKind::Interview {
        if request.runtime_profile_id.is_some() || request.builtin_runtime.is_some() {
            return Err(ApiError::from(
                lattice_core::shared::error::AppError::InvalidInput(
                    "Interview activities do not use an execution environment.".into(),
                ),
            ));
        }
        Some(
            crate::features::learning::interview_research::validate_target(
                &request.interview_target.clone().unwrap_or_default(),
            )
            .map_err(ApiError::from)?,
        )
    } else {
        if request.interview_target.is_some() {
            return Err(ApiError::from(
                lattice_core::shared::error::AppError::InvalidInput(
                    "Company research is only available for interview activities.".into(),
                ),
            ));
        }
        None
    };
    let program = LearningRepository::new(container.db_pool().clone())
        .get(&request.program_id)
        .await
        .map_err(ApiError::from)?;
    if program.summary.revision != request.expected_program_revision
        || program.summary.status != LearningProgramStatus::Active
    {
        return Err(ApiError::from(
            lattice_core::shared::error::AppError::InvalidInput(
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
            ApiError::from(lattice_core::shared::error::AppError::NotFound(
                "Learning lesson not found".into(),
            ))
        })?;
    let pin = learning_model(
        &container,
        model_requirements::practice(),
        "Writing this practical activity",
    )
    .await?;
    let llm = &pin.llm;
    let mut researched_source_ids = Vec::new();
    if let Some(target) = interview_target.as_ref() {
        let web = container.web_service();
        let library = source_library(&container);
        let research =
            crate::features::learning::practice::interview_research::research_interview_target(
                llm.as_ref(),
                web.as_ref(),
                &library,
                &request.program_id,
                &request.operation_id,
                &lesson.title,
                &request.learner_brief,
                target,
            )
            .await;
        if let Some(courses) = container.course_indexes() {
            courses.refresh(&request.program_id).await;
        }
        researched_source_ids = research.map_err(ApiError::from)?;
    }
    let source_workspace = source_library(&container)
        .workspace(&request.program_id)
        .await
        .map_err(ApiError::from)?;
    let source_ids = source_workspace
        .sources
        .iter()
        .filter_map(|source| source.active_version_id.clone())
        .collect::<Vec<_>>();
    if source_ids.is_empty()
        && LearningRepository::new(container.db_pool().clone())
            .has_source_history(&request.program_id)
            .await
            .map_err(ApiError::from)?
    {
        return Err(ApiError::from(
            lattice_core::shared::error::AppError::InvalidInput(
                "Add or adopt at least one source before generating a practical activity.".into(),
            ),
        ));
    }
    let sources = practical_source_versions(&container, &request.program_id, &source_ids)
        .await
        .map_err(ApiError::from)?;
    let runtime = crate::features::learning::practical_workspace::runtime_generation_context(
        &repo,
        &runtimes,
        request.runtime_profile_id.as_deref(),
        request.builtin_runtime,
    )
    .await
    .map_err(ApiError::from)?;
    let generated = crate::features::learning::practical_generation::generate_activity(
        llm.as_ref(),
        request.kind,
        &request.learner_brief,
        &lesson,
        &sources,
        runtime.as_ref(),
        interview_target.as_ref(),
        &researched_source_ids,
    )
    .await
    .map_err(ApiError::from)?;
    let workspace = repo
        .save_generated_activity(&request, &generated, llm.model_name(), &hash)
        .await
        .map_err(ApiError::from)?;
    Ok(workspace.resolve_runtime(&runtimes).await)
}

#[tauri::command]
#[specta::specta]
pub async fn start_learning_practical_run(
    request: StartLearningPracticalRunRequestDto,
    container: State<'_, Container>,
    runtimes: State<'_, BuiltinRuntimes>,
) -> Result<LearningPracticalRunDto, ApiError> {
    let hash = source_request_hash(&request).map_err(ApiError::from)?;
    crate::features::learning::practical_runs::start_run(
        &practical_repo(&container),
        &container.practical_run_registry(),
        container.core.background(),
        &runtimes,
        &request,
        &hash,
    )
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
    crate::features::learning::practical_runs::cancel_run(
        &practical_repo(&container),
        &container.practical_run_registry(),
        &request,
        &hash,
    )
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
            "turn",
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
            lattice_core::shared::error::AppError::InvalidInput(
                "Simulation changed; reload and retry.".into(),
            ),
        ));
    }
    if session.status != LearningSimulationStatus::Active
        || request.content.trim().is_empty()
        || request.content.chars().count() > 8_000
    {
        return Err(ApiError::from(
            lattice_core::shared::error::AppError::InvalidInput(
                "Active simulations accept responses of 1–8000 characters.".into(),
            ),
        ));
    }
    let (activity, source_ids) = repo
        .simulation_session_generation_context(&request.program_id, &session.id)
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
    let pin = learning_model(
        &container,
        model_requirements::practice(),
        "Running this simulation",
    )
    .await?;
    let llm = &pin.llm;
    let reply = crate::features::learning::practical_generation::generate_simulation_turn(
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
        &crate::features::learning::practical_repository::SimulationTurnWrite {
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
            "finish",
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
            lattice_core::shared::error::AppError::InvalidInput(
                "Simulation changed; reload and retry.".into(),
            ),
        ));
    }
    if session.status != LearningSimulationStatus::Active
        || !session
            .turns
            .iter()
            .any(|turn| turn.speaker == LearningSimulationSpeaker::Learner)
    {
        return Err(ApiError::from(
            lattice_core::shared::error::AppError::InvalidInput(
                "Respond at least once in an active simulation before requesting feedback.".into(),
            ),
        ));
    }
    let (activity, source_ids) = repo
        .simulation_session_generation_context(&request.program_id, &session.id)
        .await
        .map_err(ApiError::from)?;
    let sources = practical_source_versions(&container, &request.program_id, &source_ids)
        .await
        .map_err(ApiError::from)?;
    let pin = learning_model(
        &container,
        model_requirements::practice(),
        "Giving simulation feedback",
    )
    .await?;
    let llm = &pin.llm;
    let feedback = crate::features::learning::practical_generation::generate_simulation_turn(
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
        &crate::features::learning::practical_repository::SimulationTurnWrite {
            content: feedback.content,
            citations: feedback.citations,
            model_name: llm.model_name().into(),
        },
        &hash,
    )
    .await
    .map_err(ApiError::from)
}
