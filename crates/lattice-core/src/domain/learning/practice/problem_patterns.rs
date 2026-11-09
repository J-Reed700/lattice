//! The catalogue of problem patterns for coding sessions. A problem never names its pattern before the debrief.

use serde::{Deserialize, Serialize};

/// The six families of problem patterns.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, specta::Type,
)]
#[serde(rename_all = "snake_case")]
pub enum PatternFamily {
    Lookup,
    Scanning,
    Ordering,
    Structures,
    Search,
    Building,
}

impl PatternFamily {
    /// Every family, in catalogue order.
    pub const ALL: [Self; 6] = [
        Self::Lookup,
        Self::Scanning,
        Self::Ordering,
        Self::Structures,
        Self::Search,
        Self::Building,
    ];

    /// The family's name as the app stores and shows it.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Lookup => "lookup",
            Self::Scanning => "scanning",
            Self::Ordering => "ordering",
            Self::Structures => "structures",
            Self::Search => "search",
            Self::Building => "building",
        }
    }
}

/// A recurring pattern in coding problems.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProblemPattern {
    pub id: &'static str,
    pub name: &'static str,
    /// Other names the pattern goes by; a statement naming one gives the pattern away as much as its name does.
    pub aliases: &'static [&'static str],
    pub family: PatternFamily,
    /// What to notice in a problem that calls for the pattern.
    pub cue: &'static str,
}

/// The patterns of the catalogue.
pub const PROBLEM_PATTERNS: &[ProblemPattern] = &[];

/// The pattern with this id.
pub fn problem_pattern(_id: &str) -> Option<&'static ProblemPattern> {
    None
}

/// The first of `pattern_ids` whose name or an alias appears in `text`, ignoring case, as a whole phrase.
pub fn named_in(_text: &str, _pattern_ids: &[String]) -> Option<&'static ProblemPattern> {
    None
}

#[cfg(test)]
mod tests;
