//! Study validation and orchestration. Persisted state belongs to the repository.
use super::{dto::*, repository::StudyRepository};
use crate::{
    application::ports::LLMPort,
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

pub async fn generate_deck(
    repo: &StudyRepository,
    llm: &dyn LLMPort,
    request: GenerateStudyDeckRequestDto,
) -> Result<StudyDeckDto> {
    validate_generation(&request)?;
    let sources = repo.sources(&request.document_ids, &request.focus).await?;
    let now = chrono::Utc::now().timestamp_millis();
    let id = uuid::Uuid::new_v4().to_string();
    let cards = tokio::time::timeout(
        std::time::Duration::from_secs(180),
        super::generation::generate(llm, &request, &sources, &id, now),
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
    let cards = super::generation::generate_from_conversation(llm, &claims, &id, now).await?;
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
