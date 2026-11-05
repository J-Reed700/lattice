//! What one drill is built for: the material, the area of code, the size and the patterns to plant.

use lattice_core::domain::learning::drills::{pattern, Material, Pattern, LANGUAGES};
use lattice_core::shared::error::{AppError, Result};
use serde::{Deserialize, Serialize};

/// A request for one drill. A spec stored with a `language` and no `material` is read as code in that language.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", try_from = "StoredSpec")]
pub struct DrillSpec {
    /// What is reviewed.
    pub material: Material,
    /// What the code is about. `None`: the bank picks one.
    pub domain: Option<String>,
    /// The size the learner chose, a target for the prompt.
    pub lines: u32,
    /// The target pattern ids, from `drills::targets`.
    pub patterns: Vec<String>,
    /// The course whose topic gave the domain, when one did.
    pub program_id: Option<String>,
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
        })
    }
}

impl DrillSpec {
    /// Whether the spec names code in a language drills are written in, a size, a domain that says something when
    /// it is given, and at least one pattern, each in the catalogue, applying to the material and listed once.
    pub fn validate(&self) -> Result<()> {
        match &self.material {
            Material::Code { language } if !LANGUAGES.contains(&language.as_str()) => {
                return Err(AppError::InvalidInput(format!(
                    "{language} is not a language drills are written in."
                )));
            }
            Material::Code { .. } => {}
            Material::Passage => {
                return Err(AppError::InvalidInput(
                    "Drills on a passage are not supported yet.".into(),
                ));
            }
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
