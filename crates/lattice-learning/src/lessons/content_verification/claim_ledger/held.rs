//! The lessons a preparation job holds for the user's review: the error
//! that unwinds it, the result reference that names them, and the review
//! row that says why each stopped (§6.1).

use super::*;
use lattice_core::shared::persistence::DatabaseHandle;

/// The prefix of a job's result reference when lessons wait for review.
const NEEDS_REVIEW_REF: &str = "needs_review:";

/// The error that unwinds a lesson's preparation when it waits for the user's
/// review. It is not a failure: the job reads the review row and completes
/// (§6.1, `generation_jobs`). Its text is the hold's own words, with no
/// failure category in front of them, wherever it is shown or logged.
pub(crate) fn needs_review(claims: usize) -> AppError {
    AppError::Other(format!(
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
    /// The lesson stopped because the checker's answers could not be read,
    /// rather than waiting for the user's decisions.
    pub stopped: bool,
    /// Why it stopped, when its sources do not support enough of it.
    pub unsupported: Option<String>,
}

/// Whether `lesson_id`'s preparation in `job_id` stopped because the lesson
/// waits for the user's review (§6.1) or because the checker could not be
/// read, and for how many claims: the review row, not the error that unwound
/// the preparation, is what says so.
pub(crate) async fn held_for_review(
    pool: &impl DatabaseHandle,
    lesson_id: &str,
    job_id: &str,
) -> Result<Option<HeldForReview>> {
    let ledger = ClaimReviewRepository::new(pool.database())
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
            claims: counts.queued + counts.open,
            stopped: review.state == LessonReviewState::Stopped,
            unsupported: review
                .stop_reason
                .as_ref()
                .filter(|stop| stop.code == cuts::UNSUPPORTED)
                .map(|stop| stop.message.clone()),
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
    let message = if held.iter().all(|hold| hold.stopped) {
        match held {
            [only] if only.unsupported.is_some() => {
                format!(
                    "Stopped: {}",
                    only.unsupported.as_deref().unwrap_or_default()
                )
            }
            _ if held.iter().all(|hold| hold.unsupported.is_some()) => format!(
                "Stopped: the sources do not support enough of {}",
                plural(held.len(), "lesson")
            ),
            _ => format!(
                "Stopped: the checking model's answers could not be read for {}",
                plural(claims, "statement")
            ),
        }
    } else if held.len() == 1 {
        format!("Needs your review: {}", plural(claims, "statement"))
    } else {
        format!(
            "Needs your review: {} in {}",
            plural(claims, "statement"),
            plural(held.len(), "lesson")
        )
    };
    (format!("{NEEDS_REVIEW_REF}{}", ids.join(",")), message)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn several_held_lessons_complete_the_job_as_needs_review_for_each() {
        let held = |lesson: &str, claims| HeldForReview {
            lesson_id: lesson.into(),
            claims,
            stopped: false,
            unsupported: None,
        };
        assert_eq!(
            review_outcome(&[held("a", 2), held("b", 3)]),
            (
                "needs_review:a,b".to_owned(),
                "Needs your review: 5 statements in 2 lessons".to_owned()
            )
        );
    }

    #[test]
    fn a_lesson_its_sources_do_not_support_enough_of_says_so() {
        const WHY: &str = "The sources do not support enough of this lesson: its first explanation would have to be taken out.";
        let unsupported = |lesson: &str| HeldForReview {
            lesson_id: lesson.into(),
            claims: 0,
            stopped: true,
            unsupported: Some(WHY.into()),
        };
        assert_eq!(
            review_outcome(&[unsupported("a")]).1,
            format!("Stopped: {WHY}")
        );
        assert_eq!(
            review_outcome(&[unsupported("a"), unsupported("b")]).1,
            "Stopped: the sources do not support enough of 2 lessons"
        );
    }

    #[test]
    fn a_lesson_stopped_for_an_unreadable_checker_says_so_not_that_it_needs_review() {
        let stopped = HeldForReview {
            lesson_id: "a".into(),
            claims: 3,
            stopped: true,
            unsupported: None,
        };
        assert_eq!(
            review_outcome(&[stopped]),
            (
                "needs_review:a".to_owned(),
                "Stopped: the checking model's answers could not be read for 3 statements"
                    .to_owned()
            )
        );
    }
}
