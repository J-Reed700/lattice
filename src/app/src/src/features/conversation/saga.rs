use crate::features::conversation::summarizer::ConversationSummarizer;
use crate::infrastructure::event_bus::EventBus;
use crate::infrastructure::events::{ConversationEvent, SummaryRefreshRequestedEvent};
use crate::infrastructure::persistence::repositories::summary_repository::SummaryRepository;
use crate::shared::error::Result;
use std::sync::Arc;
use tokio::sync::broadcast::error::RecvError;
use tokio_util::sync::CancellationToken;
use tracing::{error, info, warn, Instrument};

/// Background worker that processes conversation summary events.
pub struct ConversationSummarySaga {
    event_bus: Arc<EventBus<ConversationEvent>>,
    summary_repo: Arc<SummaryRepository>,
}

impl ConversationSummarySaga {
    pub fn new(
        event_bus: Arc<EventBus<ConversationEvent>>,
        summary_repo: Arc<SummaryRepository>,
    ) -> Self {
        Self {
            event_bus,
            summary_repo,
        }
    }

    /// Run the saga until either the event bus closes (clean shutdown
    /// of the producer) or `cancel` fires (app-wide shutdown signal).
    ///
    /// The cancellation token races `recv()` so a saga blocked on a
    /// quiet bus can still wake up promptly when the app is closing.
    /// Once cancelled, the saga drops its receiver and returns; the
    /// supervisor (if any) will not restart it.
    pub async fn start(&self, cancel: CancellationToken) {
        let mut receiver = self.event_bus.subscribe();
        loop {
            tokio::select! {
                // Bias toward cancellation so a fired token wins over
                // a simultaneously-ready event.
                biased;

                _ = cancel.cancelled() => {
                    info!("ConversationSummarySaga cancelled; subscriber exiting");
                    return;
                }

                recv = receiver.recv() => match recv {
                    Ok(envelope) => {
                        // Instrument the handler under the publish-time
                        // span so any #[tracing::instrument] inside
                        // handle_event inherits this trace context.
                        // Net effect: a single user click produces one
                        // contiguous trace across chat.rs → publish →
                        // saga handler → DB write.
                        let span = envelope.span.clone();
                        let payload = envelope.payload;
                        let result = async {
                            self.handle_event(payload).await
                        }
                        .instrument(span)
                        .await;
                        if let Err(e) = result {
                            error!("ConversationSummarySaga error: {}", e);
                        }
                    }
                    // Slow consumer dropped events. Channel still live.
                    Err(RecvError::Lagged(n)) => {
                        warn!(
                            dropped = n,
                            "ConversationSummarySaga lagged behind producer; events dropped"
                        );
                    }
                    // Producer side closed. MUST break or we spin a
                    // CPU core at 100% — recv() returns immediately.
                    Err(RecvError::Closed) => {
                        info!("ConversationSummarySaga event bus closed; subscriber exiting");
                        return;
                    }
                },
            }
        }
    }

    async fn handle_event(&self, event: ConversationEvent) -> Result<()> {
        match event {
            ConversationEvent::SummaryRefreshRequested(event) => {
                self.handle_summary_refresh_requested(event).await?;
            }
        }
        Ok(())
    }

    async fn handle_summary_refresh_requested(
        &self,
        event: SummaryRefreshRequestedEvent,
    ) -> Result<()> {
        let Some(last_message_id) = event.completed_messages.last().map(|m| m.id.as_str()) else {
            return Ok(());
        };

        if self
            .summary_repo
            .is_valid(&event.conversation_id, last_message_id)
            .await?
        {
            return Ok(());
        }

        let summarizer =
            ConversationSummarizer::new(Arc::clone(&event.llm), Arc::clone(&self.summary_repo));
        summarizer
            .refresh_summary(
                &event.conversation_id,
                &event.completed_messages,
                event.original_tokens,
            )
            .await?;

        info!(
            conversation_id = event.conversation_id.as_str(),
            message_count = event.completed_messages.len(),
            "Conversation summary refreshed from event"
        );

        Ok(())
    }
}
