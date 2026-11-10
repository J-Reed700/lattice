//! The course's recall cards as the tutor reads and rates them. A card answered
//! here goes through the Recall tab's own review, into the same FSRS schedule.

use super::course_turn::CourseTurn;
use super::repository::{assistant_replies, card_due_at, card_to_rate, due_cards, due_count};
use crate::persistence::now;
use crate::portability_dto::{
    LearningRecallCardFormat, LearningRecallContentDto, ReviewLearningRecallCardRequestDto,
    SaveLearningRecallCardRequestDto,
};
use crate::recall::schedule::DAY_MS;
use crate::recall::study_dto::StudyRating;
use crate::recall_repository::LearningRecallRepository;
use crate::source_library::LearningSourceLibraryRepository;
use futures::TryStreamExt;
use lattice_core::shared::error::{AppError, Result};
use lattice_core::shared::persistence::Database;
use std::collections::VecDeque;
use std::fmt::Write as _;

/// One due card, front only: its answer never reaches the model before the
/// learner gives theirs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct DueCard {
    pub id: String,
    pub front: String,
    pub options: Vec<String>,
    /// The module its lesson sits in, or the card's own topic: what a session
    /// mixes across.
    pub group: String,
}

impl DueCard {
    /// The card as the model reads it: the id to rate it by, its front and any
    /// options.
    pub(super) fn entry(&self) -> String {
        let mut entry = format!("- card_id: {}\n  {}\n", self.id, self.front);
        for (index, option) in self.options.iter().enumerate() {
            let _ = writeln!(entry, "  {index}. {option}");
        }
        entry
    }
}

/// The cards, soonest due first within each group, one group after another in
/// turn, so a session's reviews come from across the course.
pub(super) fn interleaved(cards: Vec<DueCard>) -> Vec<DueCard> {
    let mut groups: Vec<(String, VecDeque<DueCard>)> = Vec::new();
    for card in cards {
        match groups.iter_mut().find(|(group, _)| *group == card.group) {
            Some((_, group)) => group.push_back(card),
            None => groups.push((card.group.clone(), VecDeque::from([card]))),
        }
    }
    let mut mixed = Vec::new();
    while groups.iter().any(|(_, group)| !group.is_empty()) {
        for (_, group) in &mut groups {
            mixed.extend(group.pop_front());
        }
    }
    mixed
}

/// `text` without the emphasis a reply puts on it, its spacing or its case.
fn plain_words(text: &str) -> String {
    text.replace(['*', '_', '`'], "")
        .replace('’', "'")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

/// Whether a reply of the chat put `front` to the learner. Newest first, as the
/// card asked last is the one answered.
async fn asked_in(database: &Database, conversation_id: &str, front: &str) -> Result<bool> {
    let front = plain_words(front);
    let mut replies = assistant_replies(database, conversation_id);
    while let Some(reply) = replies.try_next().await? {
        if plain_words(&reply).contains(&front) {
            return Ok(true);
        }
    }
    Ok(false)
}

pub(super) struct NewCard<'a> {
    pub question: &'a str,
    pub answer: &'a str,
    pub explanation: &'a str,
    pub source_ids: Vec<String>,
}

fn invalid(message: &str) -> AppError {
    AppError::InvalidInput(message.to_string())
}

impl CourseTurn {
    pub(super) async fn due_cards(&self) -> Result<Vec<DueCard>> {
        let cards = due_cards(&self.database, &self.program_id, now()).await?;
        Ok(interleaved(cards))
    }

    /// Rates the learner's answer to a due card into its schedule, as the
    /// Recall tab does. A card that is not due was rated already, and one no
    /// reply of this chat asked has no answer to rate: either way its schedule
    /// stays as it is.
    pub(super) async fn rate_card(
        &self,
        card_id: &str,
        rating: StudyRating,
        selected_option: Option<usize>,
    ) -> Result<String> {
        let at = now();
        let card = card_to_rate(&self.database, &self.program_id, card_id)
            .await?
            .ok_or_else(|| {
                invalid(
                    "That card is not in this course's deck. The turn's facts and due_reviews give \
                     each card's card_id.",
                )
            })?;
        if card.due_at > at {
            return Err(invalid(
                "That card is not due: it was already reviewed, or it did not come from due_reviews or save_card. Nothing was recorded.",
            ));
        }
        if !asked_in(&self.database, &self.conversation_id, &card.question).await? {
            return Err(invalid(
                "That card was not asked in this chat, so there is no answer to rate. Nothing was recorded. Ask it as written, then rate the learner's answer.",
            ));
        }
        if card.format == "multiple_choice" && selected_option.is_none() {
            return Err(invalid(
                "That card is multiple choice: pass the learner's selected_option.",
            ));
        }
        let recall = LearningRecallRepository::new(self.database.clone());
        recall
            .record_card_review(&ReviewLearningRecallCardRequestDto {
                review_id: uuid::Uuid::new_v4().to_string(),
                program_id: self.program_id.clone(),
                card_id: card_id.to_string(),
                expected_review_count: card.review_count,
                rating,
                selected_option,
            })
            .await?;
        let next = card_due_at(&self.database, card_id).await?;
        let still_due = due_count(&self.database, &self.program_id, at).await?;
        Ok(format!(
            "Recorded. This card comes back {}. Cards still due: {still_due}.",
            when(next - at)
        ))
    }

    /// Adds a card to the course's deck, citing the source versions it comes
    /// from. Due at once, as a new card is, so the tutor can ask and rate it.
    pub(super) async fn save_card(&self, card: NewCard<'_>) -> Result<String> {
        if card.source_ids.is_empty()
            && !LearningSourceLibraryRepository::new(self.database.clone())
                .active_versions(&self.program_id)
                .await?
                .is_empty()
        {
            return Err(invalid(
                "A card cites where it comes from: pass the source_id of the passage it was taught from.",
            ));
        }
        let card_id = uuid::Uuid::new_v4().to_string();
        LearningRecallRepository::new(self.database.clone())
            .store_card(&SaveLearningRecallCardRequestDto {
                operation_id: uuid::Uuid::new_v4().to_string(),
                program_id: self.program_id.clone(),
                card_id: card_id.clone(),
                expected_content_revision: None,
                format: LearningRecallCardFormat::QuestionAnswer,
                content: LearningRecallContentDto {
                    prompt: card.question.to_string(),
                    answer: card.answer.to_string(),
                    explanation: card.explanation.to_string(),
                    options: Vec::new(),
                    correct_option_index: None,
                    language: None,
                    cloze_deletions: Vec::new(),
                },
                source_version_ids: card.source_ids,
                change_reason: "Saved by the tutor".to_string(),
            })
            .await?;
        Ok(format!(
            "Saved to the course's recall deck as card {card_id}."
        ))
    }
}

fn when(gap_ms: i64) -> String {
    let days = gap_ms / DAY_MS;
    match days {
        0 => "later today".to_string(),
        1 => "tomorrow".to_string(),
        days => format!("in {days} days"),
    }
}
