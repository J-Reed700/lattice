//! What an outcome means for a schedule, and which patterns the next drill
//! plants: the order rules, the family rotation, the language filter.

use std::collections::HashSet;

use super::*;

fn due(pattern: &str, due_at: i64) -> PatternDue {
    PatternDue {
        pattern: pattern.to_owned(),
        due_at: Some(due_at),
    }
}

fn never(pattern: &str) -> PatternDue {
    PatternDue {
        pattern: pattern.to_owned(),
        due_at: None,
    }
}

fn ids(chosen: &[&'static Pattern]) -> Vec<&'static str> {
    chosen.iter().map(|pattern| pattern.id).collect()
}

fn families(chosen: &[&'static Pattern]) -> Vec<Family> {
    chosen.iter().map(|pattern| pattern.family).collect()
}

#[test]
fn an_issue_found_by_the_first_finding_is_easy() {
    assert_eq!(
        rating(&IssueOutcome::Found { finding: 0 }),
        PatternRating::Easy
    );
}

#[test]
fn an_issue_found_by_a_later_finding_is_good() {
    for finding in [1, 2, 7] {
        assert_eq!(
            rating(&IssueOutcome::Found { finding }),
            PatternRating::Good
        );
    }
}

#[test]
fn a_half_found_issue_is_hard_whichever_finding_half_found_it() {
    for finding in [0, 1, 7] {
        assert_eq!(
            rating(&IssueOutcome::HalfFound { finding }),
            PatternRating::Hard
        );
    }
}

#[test]
fn a_missed_issue_is_again() {
    assert_eq!(rating(&IssueOutcome::Missed), PatternRating::Again);
}

#[test]
fn a_rating_is_written_in_snake_case() {
    assert_eq!(
        serde_json::to_string(&PatternRating::Easy).unwrap(),
        "\"easy\""
    );
    let state = due("security.ssrf", 12);
    let stored = serde_json::to_string(&state).unwrap();
    assert_eq!(serde_json::from_str::<PatternDue>(&stored).unwrap(), state);
}

#[test]
fn patterns_due_come_first_earliest_first() {
    let states = [
        due("security.sql_injection", 5),
        due("errors.swallowed_exception", 3),
        due("correctness.off_by_one", 9),
    ];
    assert_eq!(
        ids(&targets("python", &states, 10, 3)),
        [
            "errors.swallowed_exception",
            "security.sql_injection",
            "correctness.off_by_one"
        ]
    );
}

#[test]
fn a_pattern_is_due_when_its_time_is_not_after_now() {
    let states = [
        due("security.sql_injection", 10),
        due("errors.swallowed_exception", 11),
    ];
    let chosen = targets("python", &states, 10, 2);
    assert_eq!(
        chosen.first().map(|pattern| pattern.id),
        Some("security.sql_injection")
    );
    // The one due a moment from now ranks with those practised and not due.
    assert!(!ids(&chosen).contains(&"errors.swallowed_exception"));
}

#[test]
fn never_practised_patterns_come_after_the_due_and_before_the_rest() {
    let states = [
        due("errors.swallowed_exception", 100),
        due("security.sql_injection", 1),
    ];
    let chosen = targets("python", &states, 10, 9);
    assert_eq!(
        chosen.first().map(|pattern| pattern.id),
        Some("security.sql_injection")
    );
    // Errors has patterns never practised, so the one not yet due is passed over.
    assert!(ids(&chosen).contains(&"errors.error_as_success"));
    assert!(!ids(&chosen).contains(&"errors.swallowed_exception"));
}

#[test]
fn a_pattern_with_no_state_is_never_practised() {
    let with_state = [never("correctness.off_by_one")];
    assert_eq!(
        ids(&targets("python", &with_state, 0, 9)),
        ids(&targets("python", &[], 0, 9))
    );
}

#[test]
fn when_every_pattern_is_practised_the_soonest_due_not_yet_due_comes_first() {
    let states: Vec<PatternDue> = patterns_for("python")
        .enumerate()
        .map(|(position, pattern)| due(pattern.id, 1000 - position as i64))
        .collect();
    let latest = patterns_for("python").last().unwrap();
    let chosen = targets("python", &states, 0, 1);
    assert_eq!(ids(&chosen), [latest.id]);
}

#[test]
fn the_rest_are_taken_soonest_due_first_one_family_at_a_time() {
    let states = [
        due("design.long_function", 40),
        due("security.sql_injection", 20),
        due("errors.swallowed_exception", 30),
        due("correctness.off_by_one", 10),
    ];
    let everything_else_is_far: Vec<PatternDue> = patterns_for("sql")
        .filter(|pattern| !states.iter().any(|state| state.pattern == pattern.id))
        .map(|pattern| due(pattern.id, 1_000))
        .collect();
    let all: Vec<PatternDue> = states
        .iter()
        .cloned()
        .chain(everything_else_is_far)
        .collect();
    let chosen = targets("sql", &all, 0, 4);
    assert_eq!(
        ids(&chosen),
        [
            "correctness.off_by_one",
            "security.sql_injection",
            "errors.swallowed_exception",
            "design.long_function"
        ]
    );
}

#[test]
fn due_patterns_of_one_family_wait_for_other_families() {
    let states = [
        due("concurrency.lock_ordering", 1),
        due("concurrency.check_then_act", 2),
        due("concurrency.missing_await", 3),
    ];
    assert_eq!(
        ids(&targets("python", &states, 10, 3)),
        [
            "concurrency.lock_ordering",
            "correctness.off_by_one",
            "errors.swallowed_exception"
        ]
    );
}

#[test]
fn due_patterns_in_other_families_are_all_taken_before_a_never_practised_one() {
    let states = [
        due("testing.untested_branch", 3),
        due("security.sql_injection", 2),
        due("design.long_function", 1),
    ];
    assert_eq!(
        ids(&targets("python", &states, 10, 4)),
        [
            "design.long_function",
            "security.sql_injection",
            "testing.untested_branch",
            "correctness.off_by_one"
        ]
    );
}

#[test]
fn a_family_comes_round_again_only_when_every_family_has_been_taken() {
    let chosen = targets("python", &[], 0, 18);
    assert_eq!(chosen.len(), 18);
    assert_eq!(families(&chosen[..9]), Family::ALL);
    assert_eq!(families(&chosen[9..]), Family::ALL);
    let distinct: HashSet<&str> = ids(&chosen).into_iter().collect();
    assert_eq!(distinct.len(), 18);
}

#[test]
fn every_language_can_fill_a_drill_from_every_family() {
    for language in LANGUAGES {
        let chosen = targets(language, &[], 0, Family::ALL.len());
        assert_eq!(families(&chosen), Family::ALL, "{language}");
    }
}

#[test]
fn only_patterns_that_apply_to_the_language_are_chosen() {
    for language in LANGUAGES {
        let chosen = targets(language, &[], 0, usize::MAX);
        assert!(!chosen.is_empty(), "{language}");
        assert!(
            chosen.iter().all(|pattern| pattern.applies_to(language)),
            "{language}"
        );
    }
    let rust = ids(&targets("rust", &[], 0, usize::MAX));
    assert!(rust.contains(&"errors.unwrap_on_fallible"));
    assert!(!rust.contains(&"correctness.mutable_default_argument"));
    let python = ids(&targets("python", &[], 0, usize::MAX));
    assert!(python.contains(&"correctness.mutable_default_argument"));
    assert!(!python.contains(&"errors.unwrap_on_fallible"));
}

#[test]
fn a_state_for_a_pattern_that_does_not_apply_or_does_not_exist_changes_nothing() {
    let states = [
        due("correctness.mutable_default_argument", 0),
        due("no.such_pattern", 0),
    ];
    assert_eq!(
        ids(&targets("rust", &states, 10, 9)),
        ids(&targets("rust", &[], 10, 9))
    );
}

#[test]
fn a_count_larger_than_what_applies_gives_every_pattern_once() {
    for language in LANGUAGES {
        let applying = patterns_for(language).count();
        let chosen = targets(language, &[], 0, applying + 100);
        assert_eq!(chosen.len(), applying, "{language}");
        let distinct: HashSet<&str> = ids(&chosen).into_iter().collect();
        assert_eq!(distinct.len(), applying, "{language}");
    }
}

#[test]
fn a_count_of_nothing_chooses_nothing() {
    assert!(targets("python", &[], 0, 0).is_empty());
}

#[test]
fn a_count_smaller_than_what_applies_chooses_that_many() {
    for count in [1, 2, 5, 9] {
        assert_eq!(targets("go", &[], 0, count).len(), count);
    }
}

#[test]
fn equal_inputs_give_the_same_patterns_in_the_same_order() {
    let states = [
        due("security.sql_injection", 5),
        due("errors.swallowed_exception", 5),
        never("design.long_function"),
        due("testing.untested_branch", 99),
    ];
    let first = ids(&targets("java", &states, 10, 12));
    for _ in 0..3 {
        assert_eq!(ids(&targets("java", &states, 10, 12)), first);
    }
}

#[test]
fn the_order_of_the_states_does_not_change_the_choice() {
    let states = [
        due("security.sql_injection", 5),
        due("errors.swallowed_exception", 5),
        never("design.long_function"),
        due("testing.untested_branch", 99),
        due("correctness.off_by_one", 7),
    ];
    let reversed: Vec<PatternDue> = states.iter().rev().cloned().collect();
    assert_eq!(
        ids(&targets("java", &states, 10, 12)),
        ids(&targets("java", &reversed, 10, 12))
    );
}

#[test]
fn patterns_due_at_the_same_time_are_taken_in_the_catalogues_order() {
    let states = [
        due("security.sql_injection", 5),
        due("errors.swallowed_exception", 5),
        due("correctness.off_by_one", 5),
    ];
    assert_eq!(
        ids(&targets("python", &states, 10, 3)),
        [
            "correctness.off_by_one",
            "errors.swallowed_exception",
            "security.sql_injection"
        ]
    );
}

#[test]
fn a_pattern_listed_twice_takes_its_first_entry() {
    let first_due = [
        due("security.sql_injection", 1),
        never("security.sql_injection"),
    ];
    assert_eq!(
        ids(&targets("python", &first_due, 10, 1)),
        ["security.sql_injection"]
    );
    let first_never = [
        never("security.sql_injection"),
        due("security.sql_injection", 1),
    ];
    assert_eq!(
        ids(&targets("python", &first_never, 10, 1)),
        ["correctness.off_by_one"]
    );
}
