//! The rating each key issue's pattern gets from a review, in the form the schedule is given.

use super::*;

fn issue_of(pattern: &str, start: u32) -> KeyIssue {
    let mut known = issue(Severity::Major, start, start);
    known.pattern = pattern.to_owned();
    known
}

#[test]
fn each_issue_rates_its_pattern_by_its_outcome_in_the_keys_order() {
    let key = [
        issue_of("correctness.off_by_one", 3),
        issue_of("readability.magic_number", 9),
        issue_of("errors.swallowed_exception", 14),
    ];
    let graded = assessment(
        vec![found(0), half_found(1), missed()],
        vec![matches(&[0]), matches(&[1])],
    );
    assert_eq!(
        ratings(&key, &graded),
        [
            IssueRating {
                pattern: "correctness.off_by_one".to_owned(),
                rating: PatternRating::Easy
            },
            IssueRating {
                pattern: "readability.magic_number".to_owned(),
                rating: PatternRating::Hard
            },
            IssueRating {
                pattern: "errors.swallowed_exception".to_owned(),
                rating: PatternRating::Again
            },
        ]
    );
}

#[test]
fn an_issue_found_by_a_later_finding_rates_good() {
    let key = [issue_of("correctness.off_by_one", 3)];
    let graded = assessment(
        vec![found(2)],
        vec![false_positive(), false_positive(), matches(&[0])],
    );
    assert_eq!(ratings(&key, &graded)[0].rating, PatternRating::Good);
}

#[test]
fn two_issues_of_one_pattern_give_two_ratings() {
    let key = [
        issue_of("correctness.off_by_one", 3),
        issue_of("correctness.off_by_one", 11),
    ];
    let graded = assessment(vec![found(0), missed()], vec![matches(&[0])]);
    let rated = ratings(&key, &graded);
    assert_eq!(rated.len(), 2);
    assert_eq!(rated[0].rating, PatternRating::Easy);
    assert_eq!(rated[1].rating, PatternRating::Again);
}

#[test]
fn an_issue_the_assessment_has_no_outcome_for_rates_again() {
    let key = [
        issue_of("correctness.off_by_one", 3),
        issue_of("readability.magic_number", 9),
    ];
    let graded = assessment(vec![found(0)], vec![matches(&[0])]);
    assert_eq!(
        ratings(&key, &graded)
            .iter()
            .map(|rated| rated.rating)
            .collect::<Vec<_>>(),
        [PatternRating::Easy, PatternRating::Again]
    );
}

#[test]
fn an_empty_key_rates_nothing() {
    assert_eq!(
        ratings(&[], &assessment(vec![], vec![])),
        Vec::<IssueRating>::new()
    );
}

#[test]
fn a_rating_is_stored_as_a_pattern_and_a_word() {
    let rated = IssueRating {
        pattern: "correctness.off_by_one".to_owned(),
        rating: PatternRating::Hard,
    };
    assert_eq!(
        serde_json::to_value([rated]).unwrap(),
        serde_json::json!([{"pattern": "correctness.off_by_one", "rating": "hard"}])
    );
}

#[test]
fn a_missed_review_found_issue_gives_no_rating_and_a_found_one_gives_its_own() {
    let mut raised = review_found(Severity::Major, 9, 9);
    raised.pattern = "readability.magic_number".to_owned();
    let mut not_raised = review_found(Severity::Major, 14, 14);
    not_raised.pattern = "errors.swallowed_exception".to_owned();
    let key = [
        issue_of("correctness.off_by_one", 3),
        raised,
        issue_of("correctness.off_by_one", 20),
        not_raised,
    ];
    let graded = assessment(
        vec![half_found(0), found(1), missed(), missed()],
        vec![matches(&[0]), matches(&[1])],
    );
    assert_eq!(
        ratings(&key, &graded),
        [
            IssueRating {
                pattern: "correctness.off_by_one".to_owned(),
                rating: PatternRating::Hard
            },
            IssueRating {
                pattern: "readability.magic_number".to_owned(),
                rating: PatternRating::Good
            },
            IssueRating {
                pattern: "correctness.off_by_one".to_owned(),
                rating: PatternRating::Again
            },
        ]
    );
}

#[test]
fn a_review_found_issue_with_no_outcome_gives_no_rating() {
    let key = [
        issue_of("correctness.off_by_one", 3),
        review_found(Severity::Major, 9, 9),
    ];
    let graded = assessment(vec![found(0)], vec![matches(&[0])]);
    assert_eq!(ratings(&key, &graded).len(), 1);
}
