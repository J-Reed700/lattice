use super::*;

pub(super) async fn learning_memory(
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
            let deck = crate::features::learning::recall::study_repository::StudyRepository::new(
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
    let drafts = crate::features::learning::generation::generate_recall_drafts(
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
