//! The course's recall cards as the tutor reads and rates them. A card answered
//! here goes through the Recall tab's own review, into the same FSRS schedule.

use super::course_turn::CourseTurn;
use crate::features::learning::persistence::{db, now};
use crate::features::learning::portability_dto::{
    LearningRecallCardFormat, LearningRecallContentDto, ReviewLearningRecallCardRequestDto,
    SaveLearningRecallCardRequestDto,
};
use crate::features::learning::recall::schedule::DAY_MS;
use crate::features::learning::recall::study_dto::StudyRating;
use crate::features::learning::recall_repository::LearningRecallRepository;
use crate::features::learning::source_library::LearningSourceLibraryRepository;
use lattice_core::shared::error::{AppError, Result};
use sqlx::{Row, SqlitePool};
use std::collections::VecDeque;
use std::fmt::Write as _;

/// The cards of `program_id`'s deck due at `at`, as the Recall badge counts them.
pub(super) async fn due_count(pool: &SqlitePool, program_id: &str, at: i64) -> Result<i64> {
    sqlx::query_scalar(
        "SELECT count(*) FROM study_cards c JOIN learning_memory m ON m.deck_id=c.deck_id WHERE m.program_id=? AND c.due_at<=?",
    )
    .bind(program_id)
    .bind(at)
    .fetch_one(pool)
    .await
    .map_err(db)
}

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
        let rows = sqlx::query(
            "SELECT c.id,c.question,c.options_json,COALESCE(l.module_id,c.topic) group_key FROM study_cards c JOIN learning_memory m ON m.deck_id=c.deck_id LEFT JOIN learning_card_origins o ON o.card_id=c.id AND o.program_id=m.program_id LEFT JOIN learning_lessons l ON l.id=o.lesson_id WHERE m.program_id=? AND c.due_at<=? ORDER BY c.due_at,c.id",
        )
        .bind(&self.program_id)
        .bind(now())
        .fetch_all(&self.pool)
        .await
        .map_err(db)?;
        let cards = rows
            .iter()
            .map(|row| {
                Ok(DueCard {
                    id: row.get("id"),
                    front: row.get("question"),
                    options: serde_json::from_str(row.get("options_json")).map_err(|error| {
                        AppError::InvalidState(format!("Invalid recall options: {error}"))
                    })?,
                    group: row.get("group_key"),
                })
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(interleaved(cards))
    }

    /// Rates the learner's answer to a due card into its schedule, as the
    /// Recall tab does. A card that is not due was not asked from the due
    /// list, or was rated already: its schedule stays as it is.
    pub(super) async fn rate_card(
        &self,
        card_id: &str,
        rating: StudyRating,
        selected_option: Option<usize>,
    ) -> Result<String> {
        let at = now();
        let row = sqlx::query(
            "SELECT c.due_at,c.review_count,c.format FROM study_cards c JOIN learning_memory m ON m.deck_id=c.deck_id WHERE c.id=? AND m.program_id=?",
        )
        .bind(card_id)
        .bind(&self.program_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(db)?
        .ok_or_else(|| {
            invalid(
                "That card is not in this course's deck. The turn's facts and due_reviews give \
                 each card's card_id.",
            )
        })?;
        if row.get::<i64, _>("due_at") > at {
            return Err(invalid(
                "That card is not due: it was already reviewed, or it did not come from due_reviews or save_card. Nothing was recorded.",
            ));
        }
        if row.get::<String, _>("format") == "multiple_choice" && selected_option.is_none() {
            return Err(invalid(
                "That card is multiple choice: pass the learner's selected_option.",
            ));
        }
        let recall = LearningRecallRepository::new(self.pool.clone());
        recall
            .record_card_review(&ReviewLearningRecallCardRequestDto {
                review_id: uuid::Uuid::new_v4().to_string(),
                program_id: self.program_id.clone(),
                card_id: card_id.to_string(),
                expected_review_count: row.get("review_count"),
                rating,
                selected_option,
            })
            .await?;
        let next: i64 = sqlx::query_scalar("SELECT due_at FROM study_cards WHERE id=?")
            .bind(card_id)
            .fetch_one(&self.pool)
            .await
            .map_err(db)?;
        let still_due = due_count(&self.pool, &self.program_id, at).await?;
        Ok(format!(
            "Recorded. This card comes back {}. Cards still due: {still_due}.",
            when(next - at)
        ))
    }

    /// Adds a card to the course's deck, citing the source versions it comes
    /// from. Due at once, as a new card is, so the tutor can ask and rate it.
    pub(super) async fn save_card(&self, card: NewCard<'_>) -> Result<String> {
        if card.source_ids.is_empty()
            && !LearningSourceLibraryRepository::new(self.pool.clone())
                .active_versions(&self.program_id)
                .await?
                .is_empty()
        {
            return Err(invalid(
                "A card cites where it comes from: pass the source_id of the passage it was taught from.",
            ));
        }
        let card_id = uuid::Uuid::new_v4().to_string();
        LearningRecallRepository::new(self.pool.clone())
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
