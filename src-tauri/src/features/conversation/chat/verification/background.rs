//! Grounding verification after the answer is on screen.
//!
//! The check reads every cited page and may ask the utility model about dozens
//! of claims; run inline it held the finished answer back for up to its whole
//! budget. So the turn persists the answer with a pending marker and returns,
//! and this task fills the verdicts in: it patches the message's metadata and
//! tells the window, which swaps the badge from "checking" to the result.

use std::sync::Arc;
use std::time::Instant;

use serde::Serialize;
use tracing::{info, warn};

use crate::application::contracts::settings::LLMVerificationSettingsDto;
use crate::application::ports::LLMPort;
use crate::features::qa::dto::SourceDto;
use crate::interfaces::di::Container;

use super::super::source_snapshots;
use super::super::turn_record::{TurnRecordDto, TurnStepKind, TurnStepState};
use super::super::ChatStreamEventDto;
use super::GroundingVerifier;

/// Stream status of the event that carries a finished check.
pub(in crate::features::conversation) const VERIFICATION_READY_STATUS: &str = "verification";

/// The metadata an answer is persisted with while its check is still running.
pub(in crate::features::conversation) fn pending_metadata() -> serde_json::Value {
    serde_json::json!({ "enabled": true, "pending": true })
}

/// A finished check, addressed to the message it belongs to.
#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct VerificationReadyDto {
    pub message_id: String,
    /// The same object persisted under `metadata.verification`.
    pub verification: serde_json::Value,
    /// The turn record with its "Checking the answer" step finished, when the
    /// turn kept one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub turn: Option<TurnRecordDto>,
}

/// Everything the check needs, owned, so it can outlive the turn.
pub(in crate::features::conversation) struct BackgroundVerification {
    pub container: Container,
    pub conversation_id: String,
    pub request_id: String,
    pub message_id: String,
    pub response: String,
    pub sources: Vec<SourceDto>,
    pub tuning: LLMVerificationSettingsDto,
    /// Stands in as the judge when no utility model is configured.
    pub chat_llm: Arc<dyn LLMPort>,
    /// The record persisted with the answer; its verify step is still running.
    pub turn: Option<TurnRecordDto>,
    /// When the verify step began, so its duration covers the whole check.
    pub started: Instant,
    pub emit: Box<dyn Fn(ChatStreamEventDto) + Send + Sync>,
}

impl BackgroundVerification {
    pub(in crate::features::conversation) fn spawn(self) {
        tokio::spawn(self.run());
    }

    async fn run(self) {
        // The utility model judges claims so a chat turn is not charged a
        // second pass through the large model. Without one configured the
        // chat LLM stands in, exactly as retrieval planning does; only a hard
        // load failure drops back to lexical-only verification.
        let judge_llm = match self.container.get_or_load_utility_llm().await {
            Ok(Some(utility)) => Some(utility),
            Ok(None) => Some(Arc::clone(&self.chat_llm)),
            Err(e) => {
                warn!(
                    error = %e,
                    "Utility LLM load failed — grounding stays lexical for this turn"
                );
                None
            }
        };
        let evidence = source_snapshots::with_archived_page_text(
            &self.container,
            &self.conversation_id,
            &self.sources,
        )
        .await;
        let report = GroundingVerifier::new(judge_llm)
            .with_tuning(&self.tuning)
            .verify(&self.response, &evidence)
            .await;
        let elapsed_ms = u64::try_from(self.started.elapsed().as_millis()).unwrap_or(u64::MAX);

        if report.claims_evaluated > 0 {
            info!(
                conversation_id = self.conversation_id.as_str(),
                claims_evaluated = report.claims_evaluated,
                supported_claims = report.supported_claims,
                unsupported_claims = report.unsupported_count(),
                contradicted_claims = report.contradicted_count(),
                unverified_claims = report.unverified_claims,
                judged_claims = report.judged_claim_count(),
                grounded_ratio = report.grounded_ratio(),
                verification_ms = elapsed_ms,
                "Response grounding verification complete"
            );
        }

        let verification = report.metadata_json();
        let turn = self.turn.map(|mut turn| {
            if let Some(step) = turn.steps.iter_mut().find(|step| {
                step.kind == TurnStepKind::Verify && step.state == TurnStepState::Running
            }) {
                step.state = TurnStepState::Done;
                step.duration_ms = Some(elapsed_ms);
                step.result = Some(report.result_line());
            }
            turn.timing.verification_ms = elapsed_ms;
            turn
        });

        let mut fields = vec![("verification".to_string(), verification.clone())];
        if let Some(turn) = &turn {
            fields.push(("turn".to_string(), serde_json::json!(turn)));
        }
        let service = self.container.conversation_service();
        if let Err(e) = service
            .set_message_metadata_fields(&self.message_id, fields)
            .await
        {
            // The message may have been deleted while the check ran. The
            // window is still told, so an open answer does not sit on
            // "checking" forever.
            warn!(error = %e, "Could not persist the grounding check for this answer");
        }

        (self.emit)(ChatStreamEventDto {
            status: Some(VERIFICATION_READY_STATUS.to_owned()),
            verification: Some(VerificationReadyDto {
                message_id: self.message_id,
                verification,
                turn,
            }),
            ..ChatStreamEventDto::new(&self.conversation_id, &self.request_id)
        });
    }
}
