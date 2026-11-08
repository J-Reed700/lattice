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

fn linked(mut known: KeyIssue) -> KeyIssue {
    known.lesson = Some(LessonLink {
        lesson_id: "lesson-1".to_owned(),
        sentences: vec![LessonSentence {
            text: "A sentence.".to_owned(),
            source: None,
        }],
    });
    known
}

fn passage_rating(outcomes: Vec<(IssueOutcome, String)>) -> Option<PatternRating> {
    let key: Vec<KeyIssue> = (0..outcomes.len())
        .map(|position| issue_of("correctness.off_by_one", 3 + position as u32))
        .collect();
    lesson_rating(&Material::Passage, &key, &assessment(outcomes, vec![]))
}

fn one_rating(pattern: &str, rating: PatternRating) -> IssueRating {
    IssueRating {
        pattern: pattern.to_owned(),
        rating,
    }
}

#[test]
fn a_passage_lesson_rated_good_throughout_is_good() {
    assert_eq!(
        passage_rating(vec![found(1), found(2)]),
        Some(PatternRating::Good)
    );
}

#[test]
fn a_passage_lesson_takes_the_floor_of_the_mean() {
    assert_eq!(
        passage_rating(vec![found(0), found(1), missed()]),
        Some(PatternRating::Hard)
    );
    assert_eq!(
        passage_rating(vec![found(0), missed()]),
        Some(PatternRating::Hard)
    );
}

#[test]
fn a_passage_lesson_found_first_throughout_is_easy() {
    assert_eq!(
        passage_rating(vec![found(0), found(0)]),
        Some(PatternRating::Easy)
    );
}

#[test]
fn a_passage_lesson_missed_throughout_is_again() {
    assert_eq!(
        passage_rating(vec![missed(), missed()]),
        Some(PatternRating::Again)
    );
}

#[test]
fn a_lesson_issue_with_no_outcome_counts_as_missed() {
    let key = [
        issue_of("correctness.off_by_one", 3),
        issue_of("readability.magic_number", 9),
    ];
    let graded = assessment(vec![found(1)], vec![]);
    assert_eq!(
        lesson_rating(&Material::Passage, &key, &graded),
        Some(PatternRating::Hard)
    );
}

#[test]
fn a_missed_review_found_issue_is_left_out_of_a_lessons_rating() {
    let key = [
        issue_of("correctness.off_by_one", 3),
        review_found(Severity::Major, 9, 9),
    ];
    let left_out = assessment(vec![found(0), missed()], vec![]);
    assert_eq!(
        lesson_rating(&Material::Passage, &key, &left_out),
        Some(PatternRating::Easy)
    );
}

#[test]
fn a_found_review_found_issue_counts_in_a_lessons_rating() {
    let key = [
        issue_of("correctness.off_by_one", 3),
        review_found(Severity::Major, 9, 9),
    ];
    let counted = assessment(vec![missed(), found(0)], vec![]);
    assert_eq!(
        lesson_rating(&Material::Passage, &key, &counted),
        Some(PatternRating::Hard)
    );
}

#[test]
fn a_lesson_with_nothing_rated_has_no_rating() {
    assert_eq!(
        lesson_rating(&Material::Passage, &[], &assessment(vec![], vec![])),
        None
    );
    let only_review_found = [review_found(Severity::Major, 9, 9)];
    assert_eq!(
        lesson_rating(
            &Material::Passage,
            &only_review_found,
            &assessment(vec![missed()], vec![])
        ),
        None
    );
}

#[test]
fn a_code_lesson_counts_only_the_issues_tied_to_it() {
    let code = Material::code("python");
    let key = [
        linked(issue_of("correctness.off_by_one", 3)),
        issue_of("readability.magic_number", 9),
    ];
    let graded = assessment(vec![found(1), missed()], vec![]);
    assert_eq!(
        lesson_rating(&code, &key, &graded),
        Some(PatternRating::Good)
    );
}

#[test]
fn a_code_lesson_with_no_tied_issue_is_rated_by_every_rated_issue() {
    let code = Material::code("python");
    let key = [
        issue_of("correctness.off_by_one", 3),
        issue_of("readability.magic_number", 9),
    ];
    let graded = assessment(vec![found(0), missed()], vec![]);
    assert_eq!(
        lesson_rating(&code, &key, &graded),
        Some(PatternRating::Hard)
    );
}

#[test]
fn a_code_lesson_with_nothing_rated_has_no_rating() {
    let code = Material::code("python");
    let key = [review_found(Severity::Major, 3, 3)];
    let graded = assessment(vec![missed()], vec![]);
    assert_eq!(lesson_rating(&code, &key, &graded), None);
}

#[test]
fn two_ratings_of_one_pattern_become_their_floored_mean() {
    let rated = [
        one_rating("correctness.off_by_one", PatternRating::Easy),
        one_rating("correctness.off_by_one", PatternRating::Again),
    ];
    assert_eq!(
        per_pattern(&rated),
        [one_rating("correctness.off_by_one", PatternRating::Hard)]
    );
}

#[test]
fn per_pattern_keeps_the_order_patterns_first_appear_in() {
    let rated = [
        one_rating("readability.magic_number", PatternRating::Good),
        one_rating("correctness.off_by_one", PatternRating::Easy),
        one_rating("readability.magic_number", PatternRating::Good),
        one_rating("errors.swallowed_exception", PatternRating::Again),
        one_rating("correctness.off_by_one", PatternRating::Again),
    ];
    assert_eq!(
        per_pattern(&rated),
        [
            one_rating("readability.magic_number", PatternRating::Good),
            one_rating("correctness.off_by_one", PatternRating::Hard),
            one_rating("errors.swallowed_exception", PatternRating::Again),
        ]
    );
}

#[test]
fn per_pattern_leaves_single_ratings_and_nothing_alone() {
    let rated = [
        one_rating("correctness.off_by_one", PatternRating::Easy),
        one_rating("readability.magic_number", PatternRating::Again),
    ];
    assert_eq!(per_pattern(&rated), rated);
    assert_eq!(per_pattern(&[]), Vec::<IssueRating>::new());
}
