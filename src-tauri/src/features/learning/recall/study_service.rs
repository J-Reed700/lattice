//! Study validation and orchestration. Persisted state belongs to the repository.
use super::{study_dto::*, study_repository::StudyRepository};
use crate::{
    application::ports::{LLMPort, LibraryPassagesPort},
    shared::error::{AppError, Result},
};

fn text(value: &str, label: &str, max: usize) -> Result<()> {
    if value.trim().is_empty() || value.chars().count() > max {
        return Err(AppError::InvalidInput(format!(
            "{label} must contain 1–{max} characters"
        )));
    }
    Ok(())
}
pub fn validate_generation(request: &GenerateStudyDeckRequestDto) -> Result<()> {
    text(&request.title, "Deck title", 120)?;
    if request.focus.chars().count() > 200 {
        return Err(AppError::InvalidInput(
            "Keep the topic under 200 characters".into(),
        ));
    }
    if request.study_goal.chars().count() > 300 {
        return Err(AppError::InvalidInput(
            "Keep the learning goal under 300 characters".into(),
        ));
    }
    if !(1..=3).contains(&request.document_ids.len()) || !(2..=12).contains(&request.count) {
        return Err(AppError::InvalidInput(
            "Select 1–3 documents and 2–12 cards".into(),
        ));
    }
    let mut seen = std::collections::HashSet::new();
    for id in &request.document_ids {
        uuid::Uuid::parse_str(id)
            .map_err(|_| AppError::InvalidInput("Invalid document ID".into()))?;
        if !seen.insert(id) {
            return Err(AppError::InvalidInput(
                "Select each document only once".into(),
            ));
        }
    }
    Ok(())
}

/// Passages for a deck, read from the library: each selected document's
/// chunks that mention the focus, or an even sample of it when there is no
/// focus.
pub async fn passages(
    library: &dyn LibraryPassagesPort,
    ids: &[String],
    focus: &str,
) -> Result<Vec<StudySourceDto>> {
    let terms: Vec<_> = focus
        .split(|c: char| !c.is_alphanumeric())
        .filter(|word| word.len() >= 4 && !["with", "from", "this", "that", "about"].contains(word))
        .take(8)
        .map(str::to_lowercase)
        .collect();
    let per_document = (24 / ids.len().max(1)).max(4);
    let mut sources = Vec::new();
    for id in ids {
        let document = library
            .document_text(id)
            .await?
            .filter(|document| !document.chunks.is_empty())
            .ok_or_else(|| {
                AppError::InvalidInput(
                    "A selected document has no indexed text yet. Let its import finish first."
                        .into(),
                )
            })?;
        let stride = (document.chunks.len() / per_document).max(1);
        let chosen = document
            .chunks
            .iter()
            .enumerate()
            .filter(|(index, chunk)| {
                if terms.is_empty() {
                    index % stride == 0
                } else {
                    let text = chunk.text.to_lowercase();
                    terms.iter().any(|term| text.contains(term.as_str()))
                }
            })
            .take(per_document);
        for (_, chunk) in chosen {
            sources.push(StudySourceDto {
                chunk_id: chunk.id.clone(),
                document_id: id.clone(),
                file_name: document.title.clone(),
                file_path: document.file_path.clone(),
                excerpt: chunk.text.chars().take(2400).collect(),
                url: None,
            });
        }
    }
    if sources.is_empty() {
        return Err(AppError::InvalidInput(
            "No passages matched that focus in the selected documents. Try a broader topic.".into(),
        ));
    }
    Ok(sources)
}

pub async fn generate_deck(
    repo: &StudyRepository,
    library: &dyn LibraryPassagesPort,
    llm: &dyn LLMPort,
    request: GenerateStudyDeckRequestDto,
) -> Result<StudyDeckDto> {
    validate_generation(&request)?;
    let sources = passages(library, &request.document_ids, &request.focus).await?;
    let now = chrono::Utc::now().timestamp_millis();
    let id = uuid::Uuid::new_v4().to_string();
    let cards = tokio::time::timeout(
        std::time::Duration::from_secs(180),
        super::study_generation::generate(llm, &request, &sources, &id, now),
    )
    .await
    .map_err(|_| {
        AppError::ServiceNotAvailable(
            "Question generation timed out. Try fewer cards or a narrower topic.".into(),
        )
    })??;
    let deck = StudyDeckDto {
        id,
        title: request.title.trim().into(),
        focus: request.focus.trim().into(),
        study_goal: request.study_goal.trim().into(),
        model_name: llm.model_name().into(),
        created_at: now,
        cards,
    };
    repo.save(&deck).await?;
    Ok(deck)
}

pub async fn generate_conversation_deck(
    repo: &StudyRepository,
    llm: &dyn LLMPort,
    request: GenerateConversationStudyDeckRequestDto,
) -> Result<StudyDeckDto> {
    text(&request.title, "Deck title", 120)?;
    uuid::Uuid::parse_str(&request.conversation_id)
        .map_err(|_| AppError::InvalidInput("Invalid conversation ID".into()))?;
    let claims = repo.conversation_claims(&request.conversation_id).await?;
    let now = chrono::Utc::now().timestamp_millis();
    let id = uuid::Uuid::new_v4().to_string();
    let cards = super::study_generation::generate_from_conversation(llm, &claims, &id, now).await?;
    let deck = StudyDeckDto {
        id,
        title: request.title.trim().into(),
        focus: "Verified conversation claims".into(),
        study_goal: "Recall verified claims and their cited evidence".into(),
        model_name: llm.model_name().into(),
        created_at: now,
        cards,
    };
    repo.save(&deck).await?;
    Ok(deck)
}

pub async fn review(
    repo: &StudyRepository,
    request: ReviewStudyCardRequestDto,
) -> Result<StudyCardDto> {
    uuid::Uuid::parse_str(&request.review_id)
        .map_err(|_| AppError::InvalidInput("Invalid review ID".into()))?;
    repo.review(&request, chrono::Utc::now().timestamp_millis())
        .await
}

pub async fn update_card(repo: &StudyRepository, request: UpdateStudyCardRequestDto) -> Result<()> {
    text(&request.question, "Question", 2000)?;
    text(&request.answer, "Answer", 1000)?;
    text(&request.explanation, "Explanation", 3000)?;
    repo.update_card(&request).await
}
