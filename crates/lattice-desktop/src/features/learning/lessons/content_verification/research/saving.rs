//! Saving the pages a research round captures, and the citations between them.
use super::*;

/// Where a research round saves what it captures.
pub(super) struct Saving<'s> {
    pub(super) context: &'s Context,
    pub(super) program_repo: &'s crate::features::learning::repository::LearningRepository,
    pub(super) library:
        &'s crate::features::learning::source_library::LearningSourceLibraryRepository,
}

impl Saving<'_> {
    /// Saves a capture as a research source unless the course already holds
    /// that page, with the planner's reason for selecting it (`note`); the
    /// new version's id, or `None` for a page already held. `sources` and
    /// `urls` (pages without fragments) are brought up to date.
    pub(super) async fn save(
        &self,
        sources: &mut Vec<LearningSourceDto>,
        urls: &mut HashSet<String>,
        requested_url: &str,
        captured: crate::features::learning::source_library::CapturedLearningSource,
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
        let hash = digest(&captured.text);
        self.library
            .add_selected(
                crate::features::learning::source_library::NewLearningSource {
                    operation_id: &uuid::Uuid::new_v4().to_string(),
                    source_id: &uuid::Uuid::new_v4().to_string(),
                    version_id: &version_id,
                    program_id: &self.context.program_id,
                    kind: crate::features::learning::dto::LearningSourceKind::Web,
                    operation_kind: "add_web",
                    origin: requested_url,
                    requested_url: Some(requested_url),
                    freshness: crate::features::learning::dto::LearningSourcePolicy::Fixed,
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
        Ok(Some(version_id))
    }

    /// Records that the supporting version cites the page saved as
    /// `version_id` (§8 `Cites`), with the link's place in its text.
    pub(super) async fn record_citation(
        &self,
        cited: &scrutiny::Follow,
        version_id: &str,
    ) -> Result<()> {
        use lattice_core::application::ports::SourceTrustStore;
        use lattice_core::domain::provenance::{
            RelationEvidence, RelationKind, RelationTarget, SourceRelation,
        };
        let store = crate::features::learning::references::trust::SqliteSourceTrustStore::new(
            self.context.pool.clone(),
        );
        let Some(link) = store
            .provenance(&cited.from)
            .await?
            .and_then(|record| record.links.into_iter().find(|link| link.url == cited.url))
        else {
            return Ok(());
        };
        store
            .save_relations(
                &self.context.program_id,
                &[SourceRelation {
                    from: cited.from.clone(),
                    to: RelationTarget::Version(version_id.to_owned()),
                    kind: RelationKind::Cites,
                    evidence: RelationEvidence::Citation {
                        start_byte: link.start_byte,
                        end_byte: link.end_byte,
                        anchor: link.anchor,
                    },
                }],
            )
            .await
    }
}
