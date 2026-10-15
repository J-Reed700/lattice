//! Stage 4: evidence earlier turns cited, then the retrieval pipeline.
use super::carry::CarriedMaterial;
use super::classify::Route;
use super::prepare::PreparedTurn;
use super::*;
use crate::features::conversation::chat::retrieval::RetrievalPipelineOutcome;
use lattice_core::application::services::context_assembler::{EvidenceBudget, EvidenceShare};

/// What the turn found to read, before it is fitted and numbered.
pub(super) struct Retrieved {
    /// Passages earlier turns cited, recalled for this question.
    pub(super) prior_sources: Vec<SourceDto>,
    /// Earlier cited sources that matched but had no room.
    pub(super) prior_left_out: usize,
    pub(super) retrieval: RetrievalPipelineOutcome,
}

pub(super) async fn retrieve_evidence(
    container: &dyn ChatRuntime,
    turn: &PreparedTurn,
    flags: SearchFlags,
    route: &Route,
    carried: &CarriedMaterial,
    mut evidence: EvidenceBudget,
    metrics: &mut ConversationFlowTimingMetrics,
) -> Result<Retrieved> {
    tracing::info!(
        conversation_id = turn.conv_id.as_str(),
        context_messages = turn.context.len(),
        max_tokens = turn.llm.max_context_tokens(),
        "chat_with_conversation: Context built, starting LLM generation"
    );

    // Evidence recall runs independently of web/KB search toggles. Forced web
    // research supplements a conversation; it does not reset its source set.
    let mut allowed_prior_documents: HashSet<String> = turn
        .document_context
        .iter()
        .map(|reference| reference.document_id.clone())
        .collect();
    turn.focus.confine(&mut allowed_prior_documents);
    let prior_evidence::Recalled {
        sources: prior_sources,
        left_out: prior_left_out,
    } = if flags.closed_book {
        prior_evidence::Recalled::default()
    } else {
        prior_evidence::recall(
            container,
            &turn.conv_id,
            &turn.message,
            &allowed_prior_documents,
            &carried.attachments.sources(),
            evidence.allowance(EvidenceShare::PRIOR_EVIDENCE),
            &turn.llm,
        )
        .await?
    };
    evidence.charge(
        prior_evidence::render(&prior_sources, prior_left_out)
            .as_deref()
            .map(|text| turn.llm.count_tokens(text))
            .unwrap_or(0),
    );

    // A message that only points at a file ("reference the chat I attached")
    // has no subject of its own, so the web-query rewrite has to read one off
    // the attachment or it searches for the request instead of the question.
    let attachment_digest = carried.attachments.subject_digest(ATTACHMENT_DIGEST_CHARS);
    let retrieval_start = Instant::now();
    let retrieval = run_retrieval_pipeline(
        container,
        turn.chat_turn(),
        &turn.settings,
        &route.decision,
        flags,
        &turn.document_context,
        evidence,
        attachment_digest.as_deref(),
    )
    .await;
    metrics.retrieval_pipeline_ms = elapsed_ms(retrieval_start);
    metrics.retrieval_subtimings = Some(retrieval.sub_timings.clone());
    Ok(Retrieved {
        prior_sources,
        prior_left_out,
        retrieval,
    })
}
