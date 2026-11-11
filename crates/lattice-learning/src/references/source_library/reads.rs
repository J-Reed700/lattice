//! Reading source workspaces, versions, search hits and prepared sources.
use super::repository::reads as stored;
use super::*;
use lattice_core::shared::persistence::Transactions;

/// A source's active version as the course index keys it: its immutable id,
/// the title its passages are embedded under, and the hash of its text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActiveVersion {
    pub id: String,
    pub title: String,
    pub content_sha256: String,
}

impl LearningSourceLibraryRepository {
    /// The program's sources, with what the library offers the user to remove
    /// (claim classes design 3.6).
    pub async fn workspace(&self, program_id: &str) -> Result<LearningSourceWorkspaceDto> {
        let mut workspace = self.workspace_with_deleted(program_id, false).await?;
        workspace.not_sources = self
            .refused_sources(program_id)
            .await?
            .into_iter()
            .map(Into::into)
            .collect();
        workspace.unused_research = self
            .unused_research_sources(program_id)
            .await?
            .into_iter()
            .map(Into::into)
            .collect();
        Ok(workspace)
    }

    /// Build a source snapshot for portable exports, including tombstones whose
    /// immutable versions and refresh history must remain recoverable.
    #[cfg(test)]
    pub(crate) async fn workspace_including_deleted(
        &self,
        program_id: &str,
    ) -> Result<LearningSourceWorkspaceDto> {
        self.workspace_with_deleted(program_id, true).await
    }

    async fn workspace_with_deleted(
        &self,
        program_id: &str,
        include_deleted: bool,
    ) -> Result<LearningSourceWorkspaceDto> {
        Self::validate_program_id(program_id)?;
        self.backfill_legacy(program_id).await?;
        let mut tx = self.database.begin_read().await.map_err(db)?;
        let workspace = stored::workspace_in(&mut tx, program_id, include_deleted).await?;
        tx.commit().await.map_err(db)?;
        Ok(workspace)
    }

    pub async fn get_version(
        &self,
        req: &GetLearningSourceVersionRequestDto,
    ) -> Result<LearningSourceVersionDto> {
        Self::validate_program_id(&req.program_id)?;
        for (v, label) in [
            (&req.source_id, "source"),
            (&req.version_id, "source version"),
        ] {
            if v.trim().is_empty() || v.len() > 256 {
                return Err(invalid(format!("Invalid {label} ID")));
            }
        }
        self.backfill_legacy(&req.program_id).await?;
        let found = stored::stored_version(
            &self.database,
            &req.program_id,
            &req.source_id,
            &req.version_id,
        )
        .await?
        .ok_or_else(|| AppError::NotFound("Source version not found in this program".into()))?;
        let usage = stored::version_usage(&self.database, &req.program_id, &req.version_id).await?;
        Ok(LearningSourceVersionDto {
            source_id: req.source_id.clone(),
            version: found.summary,
            full_text: found.full_text,
            usage,
        })
    }

    pub async fn search(
        &self,
        req: &SearchLearningSourcesRequestDto,
    ) -> Result<Vec<LearningSourceSearchResultDto>> {
        Self::validate_program_id(&req.program_id)?;
        let query = req.query.trim();
        if query.is_empty() {
            return Err(invalid("Search query must not be empty"));
        }
        let limit = req.limit.clamp(1, 50) as i64;
        let mut rows = stored::searchable_versions(&self.database, &req.program_id);
        let mut results = Vec::new();
        while let Some(r) = rows.try_next().await? {
            if let Some(index) = find_case_insensitive(&r.text, query) {
                results.push(LearningSourceSearchResultDto {
                    source_id: r.source_id,
                    version_id: r.version_id,
                    title: r.title,
                    excerpt: context_excerpt(&r.text, index),
                });
                if results.len() as i64 >= limit {
                    break;
                }
            }
        }
        Ok(results)
    }

    pub async fn prepare_sources(&self, program_id: &str) -> Result<Vec<LearningSourceDto>> {
        self.backfill_legacy(program_id).await?;
        stored::excerpted_sources(&self.database, program_id).await
    }
    /// The active version of every source the program has not deleted, in
    /// the order [`Self::verification_sources`] returns them, without their
    /// text: what the course index holds, read cheaply enough to compare with
    /// it on every use.
    pub async fn active_versions(&self, program_id: &str) -> Result<Vec<ActiveVersion>> {
        stored::active_versions(&self.database, program_id).await
    }
    /// The whole captured text of one version of the program's sources,
    /// deleted ones included.
    pub async fn version_text(&self, program_id: &str, version_id: &str) -> Result<Option<String>> {
        stored::version_text(&self.database, program_id, version_id).await
    }

    /// The whole captured text of every version in the program, by version
    /// ID: what a model receives in place of a stored preview.
    pub async fn version_texts(&self, program_id: &str) -> Result<HashMap<String, String>> {
        stored::version_texts(&self.database, program_id).await
    }

    pub async fn verification_sources(&self, program_id: &str) -> Result<Vec<LearningSourceDto>> {
        self.backfill_legacy(program_id).await?;
        stored::full_text_sources(&self.database, program_id).await
    }
}
