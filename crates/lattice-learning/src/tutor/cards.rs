//! The course's recall cards as the tutor reads and rates them. A card answered
//! here goes through the Recall tab's own review, into the same FSRS schedule.

use super::course_turn::CourseTurn;
use super::names::{Card, Choice, Chosen};
use super::repository::{
    card_due_at, card_to_rate, due_cards, due_count, record_asking, was_asked, CardToRate,
};
use crate::persistence::now;
use crate::portability_dto::{
    LearningRecallCardFormat, LearningRecallContentDto, ReviewLearningRecallCardRequestDto,
    SaveLearningRecallCardRequestDto,
};
use crate::recall::schedule::DAY_MS;
use crate::recall::study_dto::StudyRating;
use crate::recall_repository::LearningRecallRepository;
use crate::source_library::LearningSourceLibraryRepository;
use lattice_core::shared::error::{AppError, Result};
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

pub(super) struct NewCard<'a> {
    pub question: &'a str,
    pub answer: &'a str,
    pub explanation: &'a str,
    pub source_ids: Vec<String>,
}

const NOT_DUE: &str = "That card is not due: it was already reviewed, or it did not come from due_reviews or save_card. Nothing was recorded.";

fn invalid(message: &str) -> AppError {
    AppError::InvalidInput(message.to_string())
}

impl CourseTurn {
    pub(super) async fn due_cards(&self) -> Result<Vec<DueCard>> {
        let cards = due_cards(&self.database, &self.program_id, now()).await?;
        Ok(interleaved(cards))
    }

    /// The card as the model reads it: its name, its front and its options, each
    /// under a name of its own.
    pub(super) fn card_entry(&self, id: &str, front: &str, options: &[String]) -> Result<String> {
        let mut entry = format!("- {}\n  {front}\n", self.name(Card(id.to_string()))?);
        for (index, option) in options.iter().enumerate() {
            let chosen = Chosen {
                card: id.to_string(),
                index,
            };
            let _ = writeln!(entry, "  {} {option}", self.name(Choice(chosen))?);
        }
        Ok(entry)
    }

    /// Records that the tutor asks a due card now, and gives the card as the
    /// learner is to be asked it.
    pub(super) async fn ask_card(&self, card_id: &str) -> Result<String> {
        let at = now();
        let card = self.card_in_deck(card_id).await?;
        if card.due_at > at {
            return Err(invalid(NOT_DUE));
        }
        record_asking(
            &self.database,
            &self.conversation_id,
            card_id,
            card.review_count,
            at,
        )
        .await?;
        let entry = self.card_entry(card_id, &card.question, &card.options)?;
        Ok(format!(
            "Put this card to the learner now, as written, and wait for their answer. Rate it with record_recall once they have answered.\n{entry}"
        ))
    }

    async fn card_in_deck(&self, card_id: &str) -> Result<CardToRate> {
        card_to_rate(&self.database, &self.program_id, card_id)
            .await?
            .ok_or_else(|| {
                invalid(
                    "That card is not in this course's deck. The turn's facts and due_reviews give each card's name.",
                )
            })
    }

    /// Rates the learner's answer to a due card into its schedule, as the
    /// Recall tab does. A card that is not due was rated already, and one the
    /// tutor did not ask with `ask_card` has no answer to rate: either way its
    /// schedule stays as it is.
    pub(super) async fn rate_card(
        &self,
        card_id: &str,
        rating: Option<StudyRating>,
        chosen: Option<Chosen>,
    ) -> Result<String> {
        let at = now();
        let card = self.card_in_deck(card_id).await?;
        if card.due_at > at {
            return Err(invalid(NOT_DUE));
        }
        if !was_asked(
            &self.database,
            &self.conversation_id,
            card_id,
            card.review_count,
        )
        .await?
        {
            return Err(invalid(
                "That card was not asked with ask_card, so there is no answer to rate. Nothing was recorded. Call ask_card with it, put it to the learner as written, then rate the learner's answer.",
            ));
        }
        let multiple_choice = card.format == "multiple_choice";
        let selected_option =
            match (multiple_choice, chosen) {
                (true, Some(chosen)) if chosen.card == card_id => Some(chosen.index),
                (true, Some(_)) => return Err(invalid(
                    "That option belongs to another card. Give an option listed under this card.",
                )),
                (true, None) => {
                    return Err(invalid(
                        "That card is multiple choice: pass the option the learner chose.",
                    ))
                }
                (false, _) => None,
            };
        let rating = match (rating, multiple_choice) {
            (Some(rating), _) => rating,
            (None, true) => StudyRating::Good,
            (None, false) => {
                return Err(invalid(
                    "Give `rating`: again, hard, good or easy, from the learner's answer.",
                ))
            }
        };
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
                "A card cites where it comes from: pass the source of the passage it was taught from.",
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
            "Saved to the course's recall deck as card {}.",
            self.name(Card(card_id))?
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
