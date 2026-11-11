//! The time a Program session gets when the learner chose none.

use crate::practice::session::kind::Budget;
use lattice_core::domain::learning::practice::work::{Brief, Difficulty};

#[cfg(test)]
mod tests;

const SECONDS_PER_MINUTE: u32 = 60;
const EASY_BUDGET_MINUTES: u32 = 20;
const MEDIUM_BUDGET_MINUTES: u32 = 35;
const HARD_BUDGET_MINUTES: u32 = 45;

pub struct ProgramBudget;

impl Budget for ProgramBudget {
    fn seconds(&self, brief: &Brief) -> Option<u32> {
        let minutes = match brief.difficulty {
            Difficulty::Easy => EASY_BUDGET_MINUTES,
            Difficulty::Medium => MEDIUM_BUDGET_MINUTES,
            Difficulty::Hard => HARD_BUDGET_MINUTES,
        };
        Some(minutes * SECONDS_PER_MINUTE)
    }
}
