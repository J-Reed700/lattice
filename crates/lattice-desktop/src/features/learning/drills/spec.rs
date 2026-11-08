//! What one drill is built for: the material, the lesson of the course it is built from, the size and the patterns to
//! plant.

use lattice_core::domain::learning::drills::{pattern, Material, Pattern, LANGUAGES};
use lattice_core::shared::error::{AppError, Result};
use serde::{Deserialize, Serialize};

/// A request for one drill. A spec stored with a `language` and no `material` is read as code in that language.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", try_from = "StoredSpec")]
pub struct DrillSpec {
    /// What is reviewed.
    pub material: Material,
    /// The topic the learner asked for. No writer reads it: a drill is about its lesson, and the writers take the
    /// lesson's title as its domain.
    pub domain: Option<String>,
    /// The size the learner chose, a target for the prompt: lines of code, or sentences of an explanation.
    pub lines: u32,
    /// The target pattern ids, from `drills::targets`.
    pub patterns: Vec<String>,
    /// For code, the course whose topic gave the domain, when one did; for a passage, the course whose sources it is
    /// taken from.
    pub program_id: Option<String>,
    /// The lesson of the course the drill is built from; `None` in specs from before drills were built from lessons.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lesson_id: Option<String>,
}

/// A spec as stored: new ones hold a `material`, the ones from before passages hold the language of their code.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredSpec {
    material: Option<Material>,
    language: Option<String>,
    domain: Option<String>,
    lines: u32,
    patterns: Vec<String>,
    program_id: Option<String>,
    #[serde(default)]
    lesson_id: Option<String>,
}

impl TryFrom<StoredSpec> for DrillSpec {
    type Error = String;

    fn try_from(stored: StoredSpec) -> std::result::Result<Self, String> {
        let material = match (stored.material, stored.language) {
            (Some(material), _) => material,
            (None, Some(language)) => Material::Code { language },
            (None, None) => return Err("a spec has neither a material nor a language".to_owned()),
        };
        Ok(Self {
            material,
            domain: stored.domain,
            lines: stored.lines,
            patterns: stored.patterns,
            program_id: stored.program_id,
            lesson_id: stored.lesson_id,
        })
    }
}

impl DrillSpec {
    /// Whether the spec names code in a language drills are written in, or a passage of a named course; a size; a
    /// domain that says something when it is given; a lesson, when it names one, that says something and is of a
    /// course; and at least one pattern, each in the catalogue, applying to the
    /// material and listed once.
    pub fn validate(&self) -> Result<()> {
        match &self.material {
            Material::Code { language } if !LANGUAGES.contains(&language.as_str()) => {
                return Err(AppError::InvalidInput(format!(
                    "{language} is not a language drills are written in."
                )));
            }
            Material::Code { .. } => {}
            Material::Passage if self.course().is_none() => {
                return Err(AppError::InvalidInput(
                    "A drill on a passage names the course whose sources it is taken from.".into(),
                ));
            }
            Material::Passage => {}
        }
        if self.lines == 0 {
            return Err(AppError::InvalidInput(match self.material {
                Material::Code { .. } => "A drill needs at least one line of code.".into(),
                Material::Passage => "A drill needs at least one sentence to review.".into(),
            }));
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
        if let Some(lesson_id) = &self.lesson_id {
            if lesson_id.trim().is_empty() {
                return Err(AppError::InvalidInput(
                    "A drill's lesson cannot be blank.".into(),
                ));
            }
            if self.course().is_none() {
                return Err(AppError::InvalidInput(
                    "A drill built from a lesson names the course the lesson is of.".into(),
                ));
            }
        }
        if self.patterns.is_empty() {
            return Err(AppError::InvalidInput(
                "A drill needs at least one pattern to plant.".into(),
            ));
        }
        self.targets().map(drop)
    }

    /// The most issues of the drill that may come from its lesson: half of them, rounded up, for code; none for a
    /// passage.
    pub fn lesson_issues(&self) -> usize {
        0
    }

    /// The course a passage is taken from: the spec's course, when it names one that says something.
    pub fn course(&self) -> Option<&str> {
        self.program_id
            .as_deref()
            .filter(|program_id| !program_id.trim().is_empty())
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
            if !known.applies_to(&self.material) {
                return Err(AppError::InvalidInput(format!(
                    "{id} does not apply to {}.",
                    self.material.language().unwrap_or("a passage")
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
