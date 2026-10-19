//! The Verifier's web (§14.5) over the app's web service: search through the
//! same search lesson research uses, and fetch through the user's link path
//! (`claim_reviews::attach::capture_page`), which saves the page as a source
//! of the course with `added_by = 'verifier'` and requests its frozen copy,
//! so every page a verdict quotes is reproducible.

use std::sync::Arc;

use async_trait::async_trait;
use sqlx::SqlitePool;

use lattice_core::application::ports::{ReferenceSnapshotPort, WebServiceTrait};
use lattice_core::domain::learning::claim_review::SourceAddedBy;
use lattice_core::shared::error::{AppError, Result};

use super::tools::{SearchHit, VerifierWeb};
use super::AuditPassage;
use crate::features::learning::dto::GetLearningSourceVersionRequestDto;
use crate::features::learning::lessons::claim_reviews::attach::{capture_page, PageCapture};
use crate::features::learning::references::web_research;
use crate::features::learning::source_library::LearningSourceLibraryRepository;

/// One course's web, as its Verifier audit may use it.
pub(crate) struct VerifierPages {
    pub pool: SqlitePool,
    pub program_id: String,
    pub web: Arc<dyn WebServiceTrait>,
    pub snapshots: Arc<dyn ReferenceSnapshotPort>,
}

#[async_trait]
impl VerifierWeb for VerifierPages {
    async fn search(&self, need: &web_research::PlannedNeed) -> Result<Vec<SearchHit>> {
        Ok(web_research::search_need(self.web.as_ref(), need)
            .await?
            .into_iter()
            .map(|result| SearchHit {
                title: result.title,
                url: result.url,
                snippet: result.snippet,
            })
            .collect())
    }

    async fn save_page(&self, url: &str) -> Result<AuditPassage> {
        let capture = PageCapture {
            web: self.web.as_ref(),
            snapshots: self.snapshots.as_ref(),
        };
        let saved = capture_page(
            &capture,
            &self.pool,
            &self.program_id,
            url,
            SourceAddedBy::Verifier,
        )
        .await?
        .map_err(|failure| AppError::InvalidInput(failure.message))?;
        let version = LearningSourceLibraryRepository::new(self.pool.clone())
            .get_version(&GetLearningSourceVersionRequestDto {
                program_id: self.program_id.clone(),
                source_id: saved.source_id,
                version_id: saved.version_id.clone(),
            })
            .await?;
        Ok(AuditPassage {
            source_version_id: saved.version_id,
            title: saved.title,
            start_byte: 0,
            end_byte: version.full_text.len(),
            text: version.full_text,
        })
    }
}
