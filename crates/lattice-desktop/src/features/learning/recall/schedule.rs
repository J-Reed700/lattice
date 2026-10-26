//! FSRS-6 scheduling, the one scheduler every recall card uses.
//!
//! A card's schedule is plain data stored on its `study_cards` row; this
//! module only decides the next state, so it never touches the database.
//!
//! A deck may have a target, such as an interview. FSRS aims at months of
//! retention and never schedules sooner than a day, so before a target the
//! next review is pulled in to land before it. Each gap is a share of the time
//! left, which is where final recall peaks (Cepeda et al. 2008: at short
//! retention intervals the best gap is about 20-40% of the interval), and a
//! miss comes back after one short relearning step (Rawson and Dunlosky's
//! successive relearning). FSRS memory still moves with every review, so once
//! the target has passed the card returns to its long-term schedule.
use super::study_dto::StudyRating;
use fsrs::{MemoryState, FSRS};
use lattice_core::shared::error::{AppError, Result};

pub(crate) const DAY_MS: i64 = 86_400_000;
/// The retention FSRS aims for when it picks the next interval.
const DESIRED_RETENTION: f32 = 0.9;
/// Longest interval, about a century, so a due date always fits in an i64.
const MAX_INTERVAL_DAYS: i64 = 36_500;
const MINUTE_MS: i64 = 60_000;
/// Before a target, a missed card comes back after this one relearning step.
pub(crate) const RELEARN_STEP_MS: i64 = 10 * MINUTE_MS;
/// Inside this window before a target, a recalled card has had its last
/// review there: its next one is its long-term FSRS review, after the target.
pub(crate) const FINAL_WINDOW_MS: i64 = 90 * MINUTE_MS;

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

    /// The schedule after one review rated `rating` at `now`, landing before
    /// `target_at` while the deck's target is still ahead.
    pub(crate) fn review(
        &self,
        rating: StudyRating,
        now: i64,
        target_at: Option<i64>,
    ) -> Result<Self> {
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
        let scheduled = Self {
            stability: Some(f64::from(next.memory.stability)),
            difficulty: Some(f64::from(next.memory.difficulty)),
            last_reviewed_at: Some(now),
            due_at: now.saturating_add(days.saturating_mul(DAY_MS)),
            interval_days: days,
            review_count: self.review_count + 1,
            lapses: self.lapses + i64::from(rating == StudyRating::Again),
        };
        Ok(match target_at {
            Some(target) => scheduled.before(target, rating, now),
            None => scheduled,
        })
    }

    /// This schedule with its next review pulled in to land before `target`.
    /// The FSRS review stands when it is already sooner, when the target has
    /// passed, and for a recalled card inside the final window.
    fn before(self, target: i64, rating: StudyRating, now: i64) -> Self {
        let left = target.saturating_sub(now);
        if left <= 0 {
            return self;
        }
        let gap = match rating {
            StudyRating::Again => RELEARN_STEP_MS,
            _ if left <= FINAL_WINDOW_MS => return self,
            StudyRating::Hard => (left / 4).max(30 * MINUTE_MS),
            StudyRating::Good => (left / 2).max(60 * MINUTE_MS),
            StudyRating::Easy => (left / 4 * 3).max(120 * MINUTE_MS),
        };
        let due_at = now.saturating_add(gap);
        if due_at >= target || due_at >= self.due_at {
            return self;
        }
        Self {
            due_at,
            interval_days: gap / DAY_MS,
            ..self
        }
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
        let reviewed = unseen(now).review(StudyRating::Good, now, None)?;
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
        let seen = unseen(now).review(StudyRating::Good, now, None)?;
        let later = seen.due_at;
        let intervals = [
            StudyRating::Again,
            StudyRating::Hard,
            StudyRating::Good,
            StudyRating::Easy,
        ]
        .map(|rating| {
            seen.review(rating, later, None)
                .map(|next| next.interval_days)
        });
        let [again, hard, good, easy] = intervals;
        let (again, hard, good, easy) = (again?, hard?, good?, easy?);
        assert!(again <= hard && hard <= good && good <= easy);
        assert!(good > seen.interval_days, "a remembered card spaces out");
        assert_eq!(seen.review(StudyRating::Again, later, None)?.lapses, 1);
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
        assert!(corrupt.review(StudyRating::Good, DAY_MS, None).is_err());
    }

    const HOUR_MS: i64 = 60 * MINUTE_MS;

    #[test]
    fn before_a_target_every_rating_lands_before_it_and_better_recall_waits_longer() -> Result<()> {
        let now = 1_000 * DAY_MS;
        let target = now + 26 * HOUR_MS;
        let card = unseen(now);
        let due = [
            StudyRating::Again,
            StudyRating::Hard,
            StudyRating::Good,
            StudyRating::Easy,
        ]
        .map(|rating| {
            card.review(rating, now, Some(target))
                .map(|next| next.due_at)
        });
        let [again, hard, good, easy] = due;
        let (again, hard, good, easy) = (again?, hard?, good?, easy?);
        assert_eq!(
            again,
            now + RELEARN_STEP_MS,
            "a miss comes back after one step"
        );
        assert!(again < hard && hard < good && good < easy);
        assert!(
            easy < target,
            "every rating is reviewed again before the target"
        );
        assert_eq!(good, now + 13 * HOUR_MS, "Good waits half the time left");
        Ok(())
    }

    #[test]
    fn a_card_recalled_every_time_is_reviewed_a_few_times_before_the_target() -> Result<()> {
        let now = 1_000 * DAY_MS;
        let target = now + 26 * HOUR_MS;
        let mut card = unseen(now);
        let mut reviews = 0;
        while card.due_at < target {
            card = card.review(StudyRating::Good, card.due_at, Some(target))?;
            reviews += 1;
        }
        assert!(
            (3..=6).contains(&reviews),
            "{reviews} reviews before the target"
        );
        let last = card.last_reviewed_at.unwrap_or_default();
        assert!(
            target - last <= FINAL_WINDOW_MS,
            "the last one is close to the target"
        );
        Ok(())
    }

    #[test]
    fn a_far_passed_or_imminent_target_keeps_the_fsrs_schedule() -> Result<()> {
        let now = 1_000 * DAY_MS;
        let card = unseen(now);
        let plain = card.review(StudyRating::Good, now, None)?;
        for target in [now + 365 * DAY_MS, now - HOUR_MS, now + FINAL_WINDOW_MS] {
            assert_eq!(card.review(StudyRating::Good, now, Some(target))?, plain);
        }
        let missed = card.review(StudyRating::Again, now, Some(now + FINAL_WINDOW_MS))?;
        assert_eq!(
            missed.due_at,
            now + RELEARN_STEP_MS,
            "a miss still relearns"
        );
        assert_eq!(missed.interval_days, 0);
        Ok(())
    }
}
