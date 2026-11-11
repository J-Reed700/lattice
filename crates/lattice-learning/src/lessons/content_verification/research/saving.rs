//! Saving the pages a research round captures.
use super::*;
use lattice_core::shared::hash::sha256_hex;

/// Where a research round saves what it captures.
pub(super) struct Saving<'s> {
    pub(super) context: &'s Context,
    pub(super) program_repo: &'s crate::repository::LearningRepository,
    pub(super) library: &'s crate::source_library::LearningSourceLibraryRepository,
}

impl Saving<'_> {
    /// Saves a capture as a research source unless the course already holds
    /// that page, with the planner's reason for selecting it (`note`); the
    /// new version's id, or `None` for a page already held, including one
    /// another lesson of the course saved meanwhile. `sources` and `urls`
    /// (pages without fragments) are brought up to date.
    pub(super) async fn save(
        &self,
        sources: &mut Vec<LearningSourceDto>,
        urls: &mut HashSet<String>,
        requested_url: &str,
        captured: crate::source_library::CapturedLearningSource,
        note: Option<&str>,
    ) -> Result<Option<String>> {
        let resolved = captured.resolved_url.clone().unwrap_or_default();
        if sources.iter().any(|s| {
            s.url.as_deref().map(identity) == Some(identity(&resolved))
                || s.excerpt.trim() == captured.text.trim()
        }) {
            return Ok(None);
        }
        let version_id = uuid::Uuid::new_v4().to_string();
        let hash = sha256_hex(&captured.text);
        let saved = self
            .library
            .add_selected_once(
                crate::source_library::NewLearningSource {
                    operation_id: &uuid::Uuid::new_v4().to_string(),
                    source_id: &uuid::Uuid::new_v4().to_string(),
                    version_id: &version_id,
                    program_id: &self.context.program_id,
                    kind: crate::dto::LearningSourceKind::Web,
                    operation_kind: "add_web",
                    origin: requested_url,
                    requested_url: Some(requested_url),
                    freshness: crate::dto::LearningSourcePolicy::Fixed,
                },
                captured,
                hash,
                note,
            )
            .await?;
        urls.insert(identity(requested_url));
        urls.insert(identity(&resolved));
        // The repository normalizes captures. Retrieve those canonical
        // bytes so the report and vectors bind to exactly what is saved.
        *sources = self
            .program_repo
            .verification_sources(&self.context.program_id)
            .await?;
        Ok(match saved {
            crate::source_library::SelectedSaved::Added => Some(version_id),
            crate::source_library::SelectedSaved::Held { .. } => None,
        })
    }
}
