//! User-provided relationships are retrieval context, never citation evidence.
use crate::shared::error::{AppError, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum StructureMode {
    #[default]
    Sections,
    Pages,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SourceGroup {
    pub id: String,
    pub title: String,
    pub edition: Option<String>,
    pub description: Option<String>,
    pub ordered: bool,
    #[serde(default)]
    pub structure: StructureMode,
}

impl SourceGroup {
    pub fn validate(&self) -> Result<()> {
        if uuid::Uuid::parse_str(&self.id).is_err() || self.title.trim().is_empty() {
            return Err(AppError::InvalidInput(
                "A related source needs a title and a valid ID".into(),
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SourceContext {
    pub group: SourceGroup,
    /// Stable, zero-based position supplied by the import preview, including on retries.
    pub position: u32,
}
