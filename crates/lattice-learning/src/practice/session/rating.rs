//! The schedule rating a finished session earns, for every kind of work.

use crate::practice::session::record::Outcome;
use lattice_core::domain::learning::drills::PatternRating;
use lattice_core::domain::learning::practice::work::Rung;

/// The rating a finished session earns: Easy with every case passed and no help, Good after at most a nudge,
/// Hard after an idea or a plan, Again when a case still fails, a point was given or the solution was seen first.
pub fn rating(outcome: &Outcome) -> PatternRating {
    if !outcome.all_passed || outcome.solution_seen_before_passing {
        return PatternRating::Again;
    }
    match outcome.highest_rung {
        None => PatternRating::Easy,
        Some(Rung::Nudge) => PatternRating::Good,
        Some(Rung::Idea | Rung::Plan) => PatternRating::Hard,
        Some(Rung::Point) => PatternRating::Again,
    }
}

#[cfg(test)]
mod tests;
