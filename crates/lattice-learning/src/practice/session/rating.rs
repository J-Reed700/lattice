//! The schedule rating a finished session earns, for every kind of work.

use crate::practice::session::record::Outcome;
use lattice_core::domain::learning::drills::PatternRating;

pub fn rating(_outcome: &Outcome) -> PatternRating {
    PatternRating::Again
}

#[cfg(test)]
mod tests;
