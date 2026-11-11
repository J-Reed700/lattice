//! Program's technique spacing: the material its patterns are scheduled under, the schedules a session's rating
//! moves, the patterns the next build should use, and what is due
//! (docs/design/2026-11-08-coding-sessions-and-one-practice-area.md sections 4.9 and 6.2).

use lattice_core::domain::learning::drills::Material;
use lattice_core::domain::learning::practice::ProblemPattern;

mod repository;
pub use repository::{practice_due_in, record_ratings_in, targets_in};

/// The prefix of the material problem patterns are scheduled under, before the language.
const PROBLEM_MATERIAL: &str = "problem:";

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
