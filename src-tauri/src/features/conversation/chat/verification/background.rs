//! Grounding verification after the answer is on screen.
//!
//! The check reads every cited page and may ask the utility model about dozens
//! of claims; run inline it held the finished answer back for up to its whole
//! budget. So the turn persists the answer with a pending marker and returns,
//! and this task fills the verdicts in: it patches the message's metadata and
//! tells the window, which swaps the badge from "checking" to the result.

#[cfg(test)]
use futures::future::BoxFuture;
use std::sync::Arc;
use std::time::Instant;

use serde::Serialize;
use tracing::{info, warn};

use crate::application::contracts::settings::LLMVerificationSettingsDto;
use crate::application::ports::llm_port::OptionalLlmLoader;
use crate::application::ports::LLMPort;
use crate::features::conversation::repository::ConversationRepository;
use crate::features::conversation::trait_def::ConversationServiceTrait;
use crate::features::qa::dto::SourceDto;
#[cfg(test)]
use crate::shared::error::Result;

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
#[derive(Clone)]
pub(in crate::features::conversation) struct BackgroundVerification {
    pub conversation_repository: Arc<ConversationRepository>,
    pub conversation_service: Arc<dyn ConversationServiceTrait>,
    pub load_utility_llm: OptionalLlmLoader,
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
    pub emit: Arc<dyn Fn(ChatStreamEventDto) + Send + Sync>,
}

impl BackgroundVerification {
    pub(in crate::features::conversation) async fn spawn(self) {
        let cancel = crate::shared::runtime::background::cancellation_token();
        let finalizer = self.clone();
        if crate::shared::runtime::background::spawn(self.run(cancel)).is_none() {
            finalizer.finish_cancelled().await;
        }
    }

    async fn run(self, cancel: tokio_util::sync::CancellationToken) {
        // The utility model judges claims so a chat turn is not charged a
        // second pass through the large model. Without one configured the
        // chat LLM stands in, exactly as retrieval planning does; only a hard
        // load failure drops back to lexical-only verification.
        let judge_result = tokio::select! {
            biased;
            _ = cancel.cancelled() => {
                self.finish_cancelled().await;
                return;
            }
            result = (self.load_utility_llm)() => result,
        };
        let judge_llm = match judge_result {
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
        if cancel.is_cancelled() {
            self.finish_cancelled().await;
            return;
        }
        let repository = Arc::clone(&self.conversation_repository);
        let conversation_id = self.conversation_id.clone();
        let sources = self.sources.clone();
        let evidence = tokio::select! {
            biased;
            _ = cancel.cancelled() => {
                self.finish_cancelled().await;
                return;
            }
            evidence = source_snapshots::with_archived_page_text(
                &repository,
                &conversation_id,
                &sources,
            ) => evidence,
        };
        let verifier = GroundingVerifier::new(judge_llm).with_tuning(&self.tuning);
        let response = self.response.clone();
        let report = tokio::select! {
            biased;
            _ = cancel.cancelled() => {
                self.finish_cancelled().await;
                return;
            }
            report = verifier.verify(&response, &evidence) => report,
        };
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
        if let Err(e) = self
            .conversation_service
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

    async fn finish_cancelled(&self) {
        let verification = serde_json::json!({
            "enabled": true,
            "pending": false,
            "interrupted": true,
        });
        let elapsed_ms = u64::try_from(self.started.elapsed().as_millis()).unwrap_or(u64::MAX);
        let turn = self.turn.as_ref().map(|turn| {
            let mut turn = turn.clone();
            if let Some(step) = turn.steps.iter_mut().find(|step| {
                step.kind == TurnStepKind::Verify && step.state == TurnStepState::Running
            }) {
                step.state = TurnStepState::Failed;
                step.duration_ms = Some(elapsed_ms);
                step.result = Some("Verification interrupted".to_string());
            }
            turn.timing.verification_ms = elapsed_ms;
            turn
        });
        let mut fields = vec![("verification".to_string(), verification.clone())];
        if let Some(turn) = &turn {
            fields.push(("turn".to_string(), serde_json::json!(turn)));
        }
        if let Err(error) = self
            .conversation_service
            .set_message_metadata_fields(&self.message_id, fields)
            .await
        {
            warn!(error = %error, "Could not persist cancelled grounding check");
        }
        (self.emit)(ChatStreamEventDto {
            status: Some(VERIFICATION_READY_STATUS.to_owned()),
            verification: Some(VerificationReadyDto {
                message_id: self.message_id.clone(),
                verification,
                turn,
            }),
            ..ChatStreamEventDto::new(&self.conversation_id, &self.request_id)
        });
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::features::conversation::chat::turn_record::TurnStepDto;
    use crate::features::conversation::mocks::MockConversationService;
    use crate::features::llm::engine::factory::MockLLMPort;
    use std::sync::Mutex;
    use tokio::sync::Notify;

    #[tokio::test]
    async fn cancellation_during_utility_load_emits_interrupted_verification() {
        let service = Arc::new(MockConversationService::new());
        let load_started = Arc::new(Notify::new());
        let load_started_by_factory = Arc::clone(&load_started);
        let load_utility_llm = Arc::new(move || {
            let load_started = Arc::clone(&load_started_by_factory);
            Box::pin(async move {
                load_started.notify_one();
                std::future::pending().await
            }) as BoxFuture<'static, Result<Option<Arc<dyn LLMPort>>>>
        });
        let event = Arc::new(Mutex::new(None));
        let event_sink = Arc::clone(&event);
        let mut turn = TurnRecordDto::default();
        turn.steps.push(TurnStepDto {
            id: "verify-1".to_string(),
            kind: TurnStepKind::Verify,
            label: "Checking the answer".to_string(),
            detail: None,
            state: TurnStepState::Running,
            started_at_ms: 1,
            duration_ms: None,
            result: None,
            links: vec![],
        });
        let verification = BackgroundVerification {
            conversation_repository: Arc::new(ConversationRepository::new(
                sqlx::sqlite::SqlitePoolOptions::new()
                    .connect_lazy("sqlite::memory:")
                    .unwrap(),
            )),
            conversation_service: service,
            load_utility_llm,
            conversation_id: "conversation-1".to_string(),
            request_id: "request-1".to_string(),
            message_id: "message-1".to_string(),
            response: "A completed answer.".to_string(),
            sources: vec![],
            tuning: LLMVerificationSettingsDto::default(),
            chat_llm: Arc::new(MockLLMPort::new()),
            turn: Some(turn),
            started: Instant::now(),
            emit: Arc::new(move |payload| *event_sink.lock().unwrap() = Some(payload)),
        };
        let cancel = tokio_util::sync::CancellationToken::new();
        let worker_cancel = cancel.clone();
        let worker = tokio::spawn(verification.run(worker_cancel));

        load_started.notified().await;
        cancel.cancel();
        worker.await.unwrap();

        let event = event.lock().unwrap().take().unwrap();
        let ready = event.verification.unwrap();
        assert_eq!(ready.verification["pending"], false);
        assert_eq!(ready.verification["interrupted"], true);
        let verify_step = ready
            .turn
            .unwrap()
            .steps
            .into_iter()
            .find(|step| step.kind == TurnStepKind::Verify)
            .unwrap();
        assert_eq!(verify_step.state, TurnStepState::Failed);
        assert_eq!(
            verify_step.result.as_deref(),
            Some("Verification interrupted")
        );
    }
}
