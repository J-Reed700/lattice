//! Study validation and orchestration. Persisted state belongs to the repository.
use super::{study_dto::*, study_repository::StudyRepository};
use lattice_core::{
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

/// Passages for a deck, read from the library: every chunk of each selected
/// document that mentions a focus term, or with no focus every chunk in an
/// order that spreads any prefix evenly across the document. The deck
/// request takes as many as its window holds.
pub async fn passages(
    library: &dyn LibraryPassagesPort,
    ids: &[String],
    focus: &str,
) -> Result<Vec<StudySourceDto>> {
    let terms: Vec<_> = focus
        .split(|c: char| !c.is_alphanumeric())
        .filter(|word| word.len() >= 4 && !["with", "from", "this", "that", "about"].contains(word))
        .map(str::to_lowercase)
        .collect();
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
        let order = if terms.is_empty() {
            spread(document.chunks.len())
        } else {
            (0..document.chunks.len()).collect()
        };
        for chunk in order
            .into_iter()
            .filter_map(|index| document.chunks.get(index))
        {
            if !terms.is_empty() {
                let text = chunk.text.to_lowercase();
                if !terms.iter().any(|term| text.contains(term.as_str())) {
                    continue;
                }
            }
            sources.push(StudySourceDto {
                chunk_id: chunk.id.clone(),
                document_id: id.clone(),
                file_name: document.title.clone(),
                file_path: document.file_path.clone(),
                excerpt: chunk.text.clone(),
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

/// The indexes `0..len` in an order whose every prefix is spread evenly:
/// the middle first, then the middles of each half, and so on.
pub(super) fn spread(len: usize) -> Vec<usize> {
    let mut order = Vec::with_capacity(len);
    let mut ranges = std::collections::VecDeque::from([(0, len)]);
    while let Some((low, high)) = ranges.pop_front() {
        if low >= high {
            continue;
        }
        let middle = low + (high - low) / 2;
        order.push(middle);
        ranges.push_back((low, middle));
        ranges.push_back((middle + 1, high));
    }
    order
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
    let cards = super::study_generation::generate(llm, &request, &sources, &id, now).await?;
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
