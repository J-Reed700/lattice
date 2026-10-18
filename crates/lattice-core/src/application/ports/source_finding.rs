//! What callers hold to find sources: the chat's web tools, the learning
//! research loop and outline references all go through this port.

use async_trait::async_trait;
use url::Url;

use super::source_finder::Budget;
use crate::domain::sourcing::{
    FinderHealth, Findings, Lead, Near, OwnerResolution, SourceNeed, SubjectHint,
};
use crate::shared::error::Result;

/// Routed, fused, budgeted source finding.
#[cfg_attr(any(test, feature = "test-support"), mockall::automock)]
#[async_trait]
pub trait SourceFinding: Send + Sync {
    /// Routes the need to finders in waves, fuses their leads, and reports
    /// every finder's outcome. Errs only when every applicable finder was
    /// skipped or failed, with every cause.
    async fn find(&self, need: SourceNeed, budget: Budget) -> Result<Findings>;

    /// The origins that own a subject (Wikidata, a registry, GitHub), cached.
    async fn resolve_owner(&self, subject: &SubjectHint, budget: Budget)
        -> Result<OwnerResolution>;

    /// The outbound links of a page, with the sentence and offset of each,
    /// those near `near` first.
    async fn leads_from_page(
        &self,
        url: &Url,
        near: Option<Near>,
        budget: Budget,
    ) -> Result<Vec<Lead>>;

    /// Every registered finder's health.
    fn health(&self) -> Vec<FinderHealth>;
}
