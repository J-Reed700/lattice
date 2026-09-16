use super::dto::StudyRating;

const DAY_MS: i64 = 86_400_000;

/// A simple expanding review schedule. Again returns the card in ten minutes;
/// the other ratings schedule a later day, capped at one year.
pub fn next_review(rating: StudyRating, previous_days: i64, now: i64) -> (i64, i64) {
    let previous = previous_days.clamp(0, 365);
    let days = match rating {
        StudyRating::Again => return (now + 600_000, 0),
        StudyRating::Hard => (previous * 6 / 5).max(1),
        StudyRating::Good => (previous * 2).max(1),
        StudyRating::Easy => (previous * 3).max(4),
    }
    .min(365);
    (now + days * DAY_MS, days)
}
