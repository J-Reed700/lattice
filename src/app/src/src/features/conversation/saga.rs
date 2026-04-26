use crate::features::conversation::summarizer::ConversationSummarizer;
use crate::infrastructure::command_channel::CommandReceiver;
use crate::infrastructure::events::{ConversationEvent, SummaryRefreshRequestedEvent};
use crate::infrastructure::persistence::repositories::summary_repository::SummaryRepository;
use crate::shared::error::Result;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
use tracing::{error, info, Instrument};

/// Background worker that processes conversation summary commands.
///
/// Migrated from broadcast `EventBus` to mpsc `CommandChannel`:
/// SummaryRefreshRequested is a *command* (do this work), not a
/// notification — losing one because a slow saga lagged would
/// silently break the user's summary. mpsc gives backpressure
/// instead of dropping. There is exactly one publisher (chat.rs)
/// and one consumer (this saga), so fan-out is unnecessary.
pub struct ConversationSummarySaga {
    command_rx: Arc<CommandReceiver<ConversationEvent>>,
    summary_repo: Arc<SummaryRepository>,
}

impl ConversationSummarySaga {
    pub fn new(
        command_rx: Arc<CommandReceiver<ConversationEvent>>,
        summary_repo: Arc<SummaryRepository>,
    ) -> Self {
        Self {
            command_rx,
            summary_repo,
        }
    }

    /// Run the saga until either the command channel closes (sender
    /// dropped — clean shutdown) or `cancel` fires (app-wide shutdown
    /// signal).
    ///
    /// The cancellation token races `recv()` so a saga blocked on a
    /// quiet channel can still wake up promptly when the app is
    /// closing. Once cancelled or closed, the saga returns; the
    /// supervisor (if any) will not restart it.
    pub async fn start(&self, cancel: CancellationToken) {
        loop {
            tokio::select! {
                // Bias toward cancellation so a fired token wins over
                // a simultaneously-ready command.
                biased;

                _ = cancel.cancelled() => {
                    info!("ConversationSummarySaga cancelled; consumer exiting");
                    return;
                }

                recv = self.command_rx.recv() => match recv {
                    Some(envelope) => {
                        // Instrument the handler under the send-time
                        // span so any #[tracing::instrument] inside
                        // handle_event inherits this trace context.
                        // Net effect: a single user click produces one
                        // contiguous trace across chat.rs → command
                        // send → saga handler → DB write.
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
                    // Sender dropped — clean shutdown. mpsc has no
                    // Lagged variant (backpressured, lossless), so
                    // this is the only error case.
                    None => {
                        info!("ConversationSummarySaga command channel closed; consumer exiting");
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
