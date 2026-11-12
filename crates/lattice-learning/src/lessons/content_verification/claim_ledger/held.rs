//! The lessons a preparation job holds for the user's review: the error
//! that unwinds it, the result reference that names them, and the review
//! row that says why each stopped (§6.1).

use super::*;
use lattice_core::domain::learning::claim_review::StopCode;
use lattice_core::shared::persistence::Database;

/// The prefix of a job's result reference when lessons wait for review.
const NEEDS_REVIEW_REF: &str = "needs_review:";

/// The error that unwinds a lesson's preparation when it waits for the user's
/// review. It is not a failure: the job reads the review row and completes
/// (§6.1, `generation_jobs`). Its text is the hold's own words, with no
/// failure category in front of them, wherever it is shown or logged.
pub(crate) fn needs_review(claims: usize) -> AppError {
    AppError::LessonHeld(format!(
        "Needs your review: {claims} statement{} could not be verified. Resolve each one, then resume verification.",
        if claims == 1 { "" } else { "s" }
    ))
}

/// The lessons a job's result reference names as waiting for review: the
/// inverse of [`review_outcome`].
pub(crate) fn held_lessons(result_ref: &str) -> Option<Vec<String>> {
    let ids = result_ref.strip_prefix(NEEDS_REVIEW_REF)?;
    Some(
        ids.split(',')
            .filter(|id| !id.is_empty())
            .map(str::to_owned)
            .collect(),
    )
}

/// A lesson of a preparation job that waits for the user's review.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct HeldForReview {
    pub lesson_id: String,
    /// Claims waiting for the user, or the checker's answer.
    pub claims: usize,
    /// Why it stopped; `None` when it waits for the user's decisions.
    pub stop: Option<StopReason>,
}

/// Whether `lesson_id`'s preparation in `job_id` stopped because the lesson
/// waits for the user's review (§6.1) or because the checker could not be
/// read, and for how many claims: the review row, not the error that unwound
/// the preparation, is what says so.
pub(crate) async fn held_for_review(
    database: &Database,
    lesson_id: &str,
    job_id: &str,
) -> Result<Option<HeldForReview>> {
    let ledger = ClaimReviewRepository::new(database.clone())
        .load(lesson_id)
        .await?;
    let review = ledger
        .review
        .as_ref()
        .filter(|review| review.job_id == job_id)
        .filter(|review| {
            matches!(
                review.state,
                LessonReviewState::NeedsReview | LessonReviewState::Stopped
            )
        });
    Ok(review.map(|review| {
        let counts = ClaimCounts::tally(ledger.claims.iter());
        HeldForReview {
            lesson_id: lesson_id.to_owned(),
            claims: counts.unresolved(),
            stop: review
                .stop_reason
                .clone()
                .filter(|_| review.state == LessonReviewState::Stopped),
        }
    }))
}

/// The completed job's result reference and message when lessons wait for
/// review (§6.1): `needs_review:<lesson id>[,<lesson id>…]`.
pub(crate) fn review_outcome(held: &[HeldForReview]) -> (String, String) {
    let ids: Vec<&str> = held.iter().map(|hold| hold.lesson_id.as_str()).collect();
    let claims: usize = held.iter().map(|hold| hold.claims).sum();
    let plural =
        |count: usize, word: &str| format!("{count} {word}{}", if count == 1 { "" } else { "s" });
    let stops: Option<Vec<&StopReason>> = held.iter().map(|hold| hold.stop.as_ref()).collect();
    let message = match stops.as_deref() {
        Some([only]) => format!("Stopped: {}", only.message),
        Some(stops)
            if stops
                .iter()
                .all(|stop| stop.code == StopCode::SourcesDoNotSupport) =>
        {
            format!(
                "Stopped: the sources do not support enough of {}",
                plural(held.len(), "lesson")
            )
        }
        Some(stops)
            if stops
                .iter()
                .all(|stop| stop.code == StopCode::CheckerUnreadable) =>
        {
            format!(
                "Stopped: the checking model's answers could not be read for {}",
                plural(claims, "statement")
            )
        }
        Some(_) => format!(
            "Stopped: {} could not go on; each lesson's statement review says why",
            plural(held.len(), "lesson")
        ),
        None if held.len() == 1 => {
            format!("Needs your review: {}", plural(claims, "statement"))
        }
        None => format!(
            "Needs your review: {} in {}",
            plural(claims, "statement"),
            plural(held.len(), "lesson")
        ),
    };
    (format!("{NEEDS_REVIEW_REF}{}", ids.join(",")), message)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stop(code: StopCode, message: &str) -> StopReason {
        StopReason {
            code,
            message: message.into(),
            fix: String::new(),
            at: 0,
            request: None,
        }
    }

    fn stopped(lesson: &str, claims: usize, code: StopCode, message: &str) -> HeldForReview {
        HeldForReview {
            lesson_id: lesson.into(),
            claims,
            stop: Some(stop(code, message)),
        }
    }

    fn waiting(lesson: &str, claims: usize) -> HeldForReview {
        HeldForReview {
            lesson_id: lesson.into(),
            claims,
            stop: None,
        }
    }

    #[test]
    fn several_held_lessons_complete_the_job_as_needs_review_for_each() {
        assert_eq!(
            review_outcome(&[waiting("a", 2), waiting("b", 3)]),
            (
                "needs_review:a,b".to_owned(),
                "Needs your review: 5 statements in 2 lessons".to_owned()
            )
        );
        assert_eq!(
            review_outcome(&[waiting("a", 1)]).1,
            "Needs your review: 1 statement"
        );
    }

    #[test]
    fn one_stopped_lesson_reports_its_own_stop() {
        const WHY: &str = "The sources do not support enough of this lesson: its first explanation would have to be taken out.";
        for code in [StopCode::SourcesDoNotSupport, StopCode::PolicyChanged] {
            assert_eq!(
                review_outcome(&[stopped("a", 0, code, WHY)]).1,
                format!("Stopped: {WHY}")
            );
        }
    }

    #[test]
    fn lessons_that_stopped_for_one_cause_report_that_cause() {
        let unsupported = |lesson: &str| stopped(lesson, 0, StopCode::SourcesDoNotSupport, "why");
        assert_eq!(
            review_outcome(&[unsupported("a"), unsupported("b")]).1,
            "Stopped: the sources do not support enough of 2 lessons"
        );
        let unreadable = |lesson: &str| stopped(lesson, 3, StopCode::CheckerUnreadable, "why");
        assert_eq!(
            review_outcome(&[unreadable("a"), unreadable("b")]).1,
            "Stopped: the checking model's answers could not be read for 6 statements"
        );
    }

    #[test]
    fn lessons_that_stopped_for_different_causes_say_each_review_tells_why() {
        assert_eq!(
            review_outcome(&[
                stopped("a", 0, StopCode::SourcesDoNotSupport, "why"),
                stopped("b", 3, StopCode::CheckerUnreadable, "why"),
            ])
            .1,
            "Stopped: 2 lessons could not go on; each lesson's statement review says why"
        );
        assert_eq!(
            review_outcome(&[
                stopped("a", 0, StopCode::NoProgress, "why"),
                waiting("b", 3)
            ])
            .1,
            "Needs your review: 3 statements in 2 lessons"
        );
    }
}
