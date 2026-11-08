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
    /// The size the learner chose, a target for the prompt: lines of code, or sentences of an explanation.
    pub lines: u32,
    /// The target pattern ids, from `drills::targets`.
    pub patterns: Vec<String>,
    /// The course the drill is built for: the course of its lesson.
    pub program_id: Option<String>,
    /// The lesson of the course the drill is built from; `None` only in specs from before drills were built from
    /// lessons, which `validate` refuses.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lesson_id: Option<String>,
}

/// A spec as stored: new ones hold a `material`, the ones from before passages hold the language of their code.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredSpec {
    material: Option<Material>,
    language: Option<String>,
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
            lines: stored.lines,
            patterns: stored.patterns,
            program_id: stored.program_id,
            lesson_id: stored.lesson_id,
        })
    }
}

/// Whether `language` is one drills are written in.
pub fn check_language(_language: &str) -> Result<()> {
    Ok(())
}

/// Whether a drill of `material` with `lines` and `patterns` patterns can be asked for: code in a language drills are
/// written in, or a passage; at least one line of code or sentence; and at least one pattern.
pub fn check_request(material: &Material, lines: u32, patterns: usize) -> Result<()> {
    if let Material::Code { language } = material {
        if !LANGUAGES.contains(&language.as_str()) {
            return Err(AppError::InvalidInput(format!(
                "{language} is not a language drills are written in."
            )));
        }
    }
    if lines == 0 {
        return Err(AppError::InvalidInput(match material {
            Material::Code { .. } => "A drill needs at least one line of code.".into(),
            Material::Passage => "A drill needs at least one sentence to review.".into(),
        }));
    }
    if patterns == 0 {
        return Err(AppError::InvalidInput(
            "A drill needs at least one pattern to plant.".into(),
        ));
    }
    Ok(())
}

impl DrillSpec {
    /// Whether the spec can be built: [`check_request`] on its material, size and patterns; a course and a lesson of
    /// it, both saying something; and each pattern in the catalogue, applying to the material and listed once.
    pub fn validate(&self) -> Result<()> {
        check_request(&self.material, self.lines, self.patterns.len())?;
        if self.course().is_none() {
            return Err(AppError::InvalidInput(
                "A drill names the course it is built for.".into(),
            ));
        }
        match self.lesson_id.as_deref() {
            None => {
                return Err(AppError::InvalidInput(
                    "A drill is built from a lesson of its course.".into(),
                ));
            }
            Some(lesson_id) if lesson_id.trim().is_empty() => {
                return Err(AppError::InvalidInput(
                    "A drill's lesson cannot be blank.".into(),
                ));
            }
            Some(_) => {}
        }
        self.targets().map(drop)
    }

    /// The most issues of the drill that may come from its lesson: half of them, rounded up, for code; none for a
    /// passage.
    pub fn lesson_issues(&self) -> usize {
        match self.material.language() {
            Some(_) => self.patterns.len().div_ceil(2),
            None => 0,
        }
    }

    /// The course the drill is built for: the spec's course, when it names one that says something.
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
