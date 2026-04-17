use crate::features::conversation::summarizer::ConversationSummarizer;
use crate::infrastructure::event_bus::EventBus;
use crate::infrastructure::events::{ConversationEvent, SummaryRefreshRequestedEvent};
use crate::infrastructure::persistence::repositories::summary_repository::SummaryRepository;
use crate::shared::error::Result;
use std::sync::Arc;
use tracing::{error, info, warn};

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

    pub async fn start(&self) {
        let mut receiver = self.event_bus.subscribe();
        loop {
            match receiver.recv().await {
                Ok(event) => {
                    if let Err(e) = self.handle_event(event).await {
                        error!("ConversationSummarySaga error: {}", e);
                    }
                }
                Err(e) => {
                    warn!("ConversationSummarySaga receiver error: {}", e);
                }
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
