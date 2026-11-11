use super::*;
use lattice_core::domain::learning::practice::work::Rung;

fn outcome(all_passed: bool, highest_rung: Option<Rung>, seen: bool) -> Outcome {
    Outcome {
        all_passed,
        highest_rung,
        solution_seen_before_passing: seen,
    }
}

#[test]
fn every_case_passed_without_help_is_easy() {
    assert_eq!(rating(&outcome(true, None, false)), PatternRating::Easy);
}

#[test]
fn every_case_passed_after_a_nudge_is_good() {
    let nudged = outcome(true, Some(Rung::Nudge), false);
    assert_eq!(rating(&nudged), PatternRating::Good);
}

#[test]
fn every_case_passed_after_an_idea_is_hard() {
    let helped = outcome(true, Some(Rung::Idea), false);
    assert_eq!(rating(&helped), PatternRating::Hard);
}

#[test]
fn every_case_passed_after_a_plan_is_hard() {
    let helped = outcome(true, Some(Rung::Plan), false);
    assert_eq!(rating(&helped), PatternRating::Hard);
}

#[test]
fn a_failing_case_is_again_whatever_the_help() {
    for rung in [
        None,
        Some(Rung::Nudge),
        Some(Rung::Idea),
        Some(Rung::Plan),
        Some(Rung::Point),
    ] {
        assert_eq!(rating(&outcome(false, rung, false)), PatternRating::Again);
    }
}

#[test]
fn a_point_is_again_even_when_every_case_passed() {
    let pointed = outcome(true, Some(Rung::Point), false);
    assert_eq!(rating(&pointed), PatternRating::Again);
}

#[test]
fn the_solution_seen_before_passing_is_again() {
    assert_eq!(rating(&outcome(true, None, true)), PatternRating::Again);
    let nudged = outcome(true, Some(Rung::Nudge), true);
    assert_eq!(rating(&nudged), PatternRating::Again);
}
