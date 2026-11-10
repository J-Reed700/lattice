//! What a problem teaches the learner's schedule: a rating for each of its patterns by rule, the schedules those
//! ratings move, the patterns the next build should use, and what is due
//! (docs/design/2026-11-08-coding-sessions-and-one-practice-area.md sections 4.9 and 6.2).

use super::dto::CodeHintRung;
use lattice_core::domain::learning::drills::{Material, PatternRating};
use lattice_core::domain::learning::practice::ProblemPattern;

mod repository;
pub use repository::{practice_due_in, record_ratings_in, targets_in};

/// The prefix of the material problem patterns are scheduled under, before the language.
const PROBLEM_MATERIAL: &str = "problem:";

/// How a problem went, as far as its patterns are concerned.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProblemOutcome {
    pub all_passed: bool,
    pub highest_rung: Option<CodeHintRung>,
    pub solution_seen_before_passing: bool,
}

/// The rating every pattern of a problem gets: Easy with every case passed and no help, Good after at most a nudge,
/// Hard after an idea or a plan, Again when a case still fails, a line was pointed at or the solution was seen first.
pub fn rating(outcome: &ProblemOutcome) -> PatternRating {
    if !outcome.all_passed || outcome.solution_seen_before_passing {
        return PatternRating::Again;
    }
    match outcome.highest_rung {
        None => PatternRating::Easy,
        Some(CodeHintRung::Nudge) => PatternRating::Good,
        Some(CodeHintRung::Idea | CodeHintRung::Plan) => PatternRating::Hard,
        Some(CodeHintRung::Line) => PatternRating::Again,
    }
}

/// The material problem patterns of `language` are scheduled under.
fn problem_material(language: &str) -> String {
    format!("{PROBLEM_MATERIAL}{}", language.to_ascii_lowercase())
}

fn family_name(pattern: &ProblemPattern) -> String {
    match serde_json::to_value(pattern.family) {
        Ok(serde_json::Value::String(name)) => name,
        _ => String::new(),
    }
}

/// Where a pattern stands, in the order the next build prefers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Standing {
    Due,
    New,
    Rested,
}

/// The material a schedule name stands for: `passage`, or `code:<language>`.
fn material_of(name: &str) -> Option<Material> {
    if name == "passage" {
        return Some(Material::Passage);
    }
    name.strip_prefix("code:")
        .filter(|language| !language.is_empty())
        .map(Material::code)
}

fn plural(count: u32, one: &str, many: &str) -> String {
    if count == 1 {
        format!("{count} {one}")
    } else {
        format!("{count} {many}")
    }
}

#[cfg(test)]
mod tests;
