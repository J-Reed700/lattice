//! The collection-level summary tier a knowledge-base turn plans with.
use tracing::{info, warn};

use crate::features::conversation::chat::ports::ChatRuntime;
use lattice_core::application::contracts::search::CorpusDocument;

/// How many documents the summary tier may nominate to open with. Matches the
/// planner's own `opening_document_ids` cap, which the plan validator enforces.
const SUMMARY_OPENING_DOCUMENTS: usize = 4;

/// The collection-level summary tier for one turn.
///
/// Empty — and free — whenever summaries are off, which is the default:
/// `Container::summary_search` returns `None` after one indexed `COUNT` and
/// nothing else runs. Everything here is additive; a failure anywhere leaves
/// the turn with exactly the retrieval it would have had without the tier.
#[derive(Default)]
pub(super) struct SummaryTier {
    /// Document-level summary text keyed by document id, for the planner
    /// catalog.
    pub(super) summaries: std::collections::HashMap<String, String>,
    /// Documents the tier would open with, best first.
    openings: Vec<String>,
}

impl SummaryTier {
    /// Hand the tier's opening documents to a finished plan.
    pub(super) fn apply(
        &self,
        plan: &mut super::corpus_plan::CorpusSearchPlan,
        message: &str,
        catalog: &[CorpusDocument],
    ) {
        if super::corpus_plan::apply_summary_openings(plan, message, &self.openings, catalog) {
            info!(
                opening_documents = ?plan.opening_document_ids,
                "retrieval: opening documents chosen by summary similarity"
            );
        }
    }
}

/// Load the summary tier for this turn.
pub(super) async fn summary_tier(
    container: &dyn ChatRuntime,
    message: &str,
    scope: &super::SpaceDocumentScope,
) -> SummaryTier {
    let Some(search) = container.summary_search().await else {
        return SummaryTier::default();
    };
    let summaries = match search.document_summaries(&scope.document_ids).await {
        Ok(summaries) => summaries,
        Err(error) => {
            warn!(%error, "Could not read document summaries; planning without them");
            std::collections::HashMap::new()
        }
    };
    // Only pay for a summary search when the request is about documents as
    // wholes; a focused question keeps the planner's own opening selection.
    if !super::corpus_plan::whole_document_intent(message) {
        return SummaryTier {
            summaries,
            openings: Vec::new(),
        };
    }
    let openings = match search
        .top_summaries(message, &scope.document_ids, SUMMARY_OPENING_DOCUMENTS)
        .await
    {
        Ok(hits) => lattice_core::application::ports::summary_search::select_opening_documents(
            &hits,
            &scope.document_ids,
            SUMMARY_OPENING_DOCUMENTS,
        ),
        Err(error) => {
            warn!(%error, "Summary search failed; keeping the planner's opening documents");
            Vec::new()
        }
    };
    SummaryTier {
        summaries,
        openings,
    }
}
