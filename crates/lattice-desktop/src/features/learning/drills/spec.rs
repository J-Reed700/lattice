//! What one drill is built for: the language, the area of code, the size and the patterns to plant.

use lattice_core::domain::learning::drills::{pattern, Pattern, LANGUAGES};
use lattice_core::shared::error::{AppError, Result};
use serde::{Deserialize, Serialize};

/// A request for one drill.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DrillSpec {
    /// A catalogue language id.
    pub language: String,
    /// What the code is about. `None`: the bank picks one.
    pub domain: Option<String>,
    /// The size the learner chose, a target for the prompt.
    pub lines: u32,
    /// The target pattern ids, from `drills::targets`.
    pub patterns: Vec<String>,
    /// The course whose topic gave the domain, when one did.
    pub program_id: Option<String>,
}

impl DrillSpec {
    /// Whether the spec names a language drills are written in, a size, a domain that says something when it is
    /// given, and at least one pattern, each in the catalogue, applying to the language and listed once.
    pub fn validate(&self) -> Result<()> {
        if !LANGUAGES.contains(&self.language.as_str()) {
            return Err(AppError::InvalidInput(format!(
                "{} is not a language drills are written in.",
                self.language
            )));
        }
        if self.lines == 0 {
            return Err(AppError::InvalidInput(
                "A drill needs at least one line of code.".into(),
            ));
        }
        if self
            .domain
            .as_deref()
            .is_some_and(|domain| domain.trim().is_empty())
        {
            return Err(AppError::InvalidInput(
                "A drill's domain cannot be blank.".into(),
            ));
        }
        if self.patterns.is_empty() {
            return Err(AppError::InvalidInput(
                "A drill needs at least one pattern to plant.".into(),
            ));
        }
        self.targets().map(drop)
    }

    /// The patterns to plant, in the spec's order.
    pub fn targets(&self) -> Result<Vec<&'static Pattern>> {
        let mut found: Vec<&'static Pattern> = Vec::with_capacity(self.patterns.len());
        for id in &self.patterns {
            let Some(known) = pattern(id) else {
                return Err(AppError::InvalidInput(format!(
                    "{id} is not a pattern of the catalogue."
                )));
            };
            if !known.applies_to(&self.language) {
                return Err(AppError::InvalidInput(format!(
                    "{id} does not apply to {}.",
                    self.language
                )));
            }
            if found.iter().any(|earlier| earlier.id == known.id) {
                return Err(AppError::InvalidInput(format!("{id} is listed twice.")));
            }
            found.push(known);
        }
        Ok(found)
    }
}
