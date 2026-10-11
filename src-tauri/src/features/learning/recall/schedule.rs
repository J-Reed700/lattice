//! FSRS-6 scheduling, the one scheduler every recall card uses.
//!
//! A card's schedule is plain data stored on its `study_cards` row; this
//! module only decides the next state, so it never touches the database.
use super::study_dto::StudyRating;
use crate::shared::error::{AppError, Result};
use fsrs::{MemoryState, FSRS};

pub(crate) const DAY_MS: i64 = 86_400_000;
/// The retention FSRS aims for when it picks the next interval.
const DESIRED_RETENTION: f32 = 0.9;
/// Longest interval, about a century, so a due date always fits in an i64.
const MAX_INTERVAL_DAYS: i64 = 36_500;

/// Where a card stands: its FSRS memory, when it is next due, and its counts.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct CardSchedule {
    /// FSRS memory. Both are `None` until the first review.
    pub stability: Option<f64>,
    pub difficulty: Option<f64>,
    pub last_reviewed_at: Option<i64>,
    pub due_at: i64,
    pub interval_days: i64,
    pub review_count: i64,
    pub lapses: i64,
}

impl CardSchedule {
    fn memory(&self) -> Option<MemoryState> {
        Some(MemoryState {
            stability: self.stability? as f32,
            difficulty: self.difficulty? as f32,
        })
    }

    /// The schedule after one review rated `rating` at `now`.
    pub(crate) fn review(&self, rating: StudyRating, now: i64) -> Result<Self> {
        if !self.stability.is_none_or(f64::is_finite) || !self.difficulty.is_none_or(f64::is_finite)
        {
            return Err(AppError::InvalidState(
                "A recall card has an invalid stored schedule.".into(),
            ));
        }
        let elapsed_days = self
            .last_reviewed_at
            .map(|last| now.saturating_sub(last).max(0) / DAY_MS)
            .unwrap_or(0)
            .min(i64::from(u32::MAX)) as u32;
        let states = FSRS::default()
            .next_states(self.memory(), DESIRED_RETENTION, elapsed_days)
            .map_err(|error| AppError::InvalidState(format!("FSRS scheduling failed: {error}")))?;
        let next = match rating {
            StudyRating::Again => states.again,
            StudyRating::Hard => states.hard,
            StudyRating::Good => states.good,
            StudyRating::Easy => states.easy,
        };
        let days = (f64::from(next.interval).round() as i64).clamp(1, MAX_INTERVAL_DAYS);
        Ok(Self {
            stability: Some(f64::from(next.memory.stability)),
            difficulty: Some(f64::from(next.memory.difficulty)),
            last_reviewed_at: Some(now),
            due_at: now.saturating_add(days.saturating_mul(DAY_MS)),
            interval_days: days,
            review_count: self.review_count + 1,
            lapses: self.lapses + i64::from(rating == StudyRating::Again),
        })
    }
}

/// The answer a review records. A quiz choice decides the rating: right is
/// Good and wrong is Again. A flashcard keeps the rating the learner gave.
pub(crate) fn answered(
    selected_option: Option<usize>,
    correct_option: usize,
    rating: StudyRating,
) -> (Option<bool>, StudyRating) {
    let correct = selected_option.map(|index| index == correct_option);
    let rating = match correct {
        Some(true) => StudyRating::Good,
        Some(false) => StudyRating::Again,
        None => rating,
    };
    (correct, rating)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A card nobody has reviewed yet, due at `now`.
    fn unseen(now: i64) -> CardSchedule {
        CardSchedule {
            stability: None,
            difficulty: None,
            last_reviewed_at: None,
            due_at: now,
            interval_days: 0,
            review_count: 0,
            lapses: 0,
        }
    }

    #[test]
    fn a_first_review_sets_fsrs_memory_and_a_bounded_due_date() -> Result<()> {
        let now = 1_000 * DAY_MS;
        let reviewed = unseen(now).review(StudyRating::Good, now)?;
        assert!(reviewed.stability.is_some_and(|value| value > 0.0));
        assert!(reviewed.difficulty.is_some());
        assert_eq!(reviewed.last_reviewed_at, Some(now));
        assert!((1..=MAX_INTERVAL_DAYS).contains(&reviewed.interval_days));
        assert_eq!(reviewed.due_at, now + reviewed.interval_days * DAY_MS);
        assert_eq!((reviewed.review_count, reviewed.lapses), (1, 0));
        Ok(())
    }

    #[test]
    fn better_recall_never_schedules_sooner_and_again_counts_a_lapse() -> Result<()> {
        let now = 1_000 * DAY_MS;
        let seen = unseen(now).review(StudyRating::Good, now)?;
        let later = seen.due_at;
        let intervals = [
            StudyRating::Again,
            StudyRating::Hard,
            StudyRating::Good,
            StudyRating::Easy,
        ]
        .map(|rating| seen.review(rating, later).map(|next| next.interval_days));
        let [again, hard, good, easy] = intervals;
        let (again, hard, good, easy) = (again?, hard?, good?, easy?);
        assert!(again <= hard && hard <= good && good <= easy);
        assert!(good > seen.interval_days, "a remembered card spaces out");
        assert_eq!(seen.review(StudyRating::Again, later)?.lapses, 1);
        Ok(())
    }

    #[test]
    fn a_quiz_choice_decides_the_rating_and_a_flashcard_keeps_its_own() {
        assert_eq!(
            answered(Some(1), 1, StudyRating::Easy),
            (Some(true), StudyRating::Good)
        );
        assert_eq!(
            answered(Some(0), 1, StudyRating::Easy),
            (Some(false), StudyRating::Again)
        );
        assert_eq!(
            answered(None, 1, StudyRating::Hard),
            (None, StudyRating::Hard)
        );
    }

    #[test]
    fn a_corrupt_stored_memory_is_refused() {
        let corrupt = CardSchedule {
            stability: Some(f64::NAN),
            ..unseen(0)
        };
        assert!(corrupt.review(StudyRating::Good, DAY_MS).is_err());
    }
}
