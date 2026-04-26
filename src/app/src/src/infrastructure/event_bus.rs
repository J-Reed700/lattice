//! Generic in-process event bus for fan-out notifications.
//!
//! `EventBus<T>` is a thin typed wrapper around `tokio::sync::broadcast`.
//! Per project policy, event types stay narrowly scoped to a single
//! bounded context — instantiate one bus per domain
//! (`EventBus<ModelDownloadEvent>`, …) rather than a global
//! "all events" enum.
//!
//! Use this when:
//! - There are MULTIPLE subscribers (broadcast / fan-out semantics).
//! - Messages are observability/notifications where dropping under
//!   load is acceptable (broadcast is lossy on slow consumer).
//!
//! For 1-to-1 *commands* where lossless backpressure matters (a slow
//! consumer must not silently lose work), use
//! `infrastructure::command_channel::CommandSender/CommandReceiver`
//! instead — that's a backpressured mpsc with the same tracing-context
//! envelope pattern.
//!
//! # Tracing context propagation
//!
//! Every published event is wrapped in an `EventEnvelope<T>` that
//! carries a `tracing::Span`. The publisher does not see the envelope —
//! `publish(event)` takes `T` and the bus snapshots a fresh
//! `event_enqueued` child span behind the scenes. Subscribers receive
//! `EventEnvelope<T>` and `.instrument(envelope.span)` their handler
//! call so any `#[tracing::instrument]` decorations inherit the
//! enqueue context as their parent.
//!
//! Why a *new* `event_enqueued` span (rather than `Span::current()`):
//! the publishing operation (e.g., a chat turn) finishes and exports
//! immediately, while saga work may run for seconds afterwards. Cloning
//! the parent span would artificially extend its duration; nesting a
//! short-lived child span at publish time keeps the parent's timing
//! honest while still linking the saga's work into the same trace.

use tokio::sync::broadcast;
use tracing::Span;

/// Wraps every event with the tracing span captured at publish time.
///
/// `tracing::Span` is `Clone` (atomic refcount bump on the underlying
/// span data), so this satisfies broadcast's `Clone` bound without
/// duplicating event payloads beyond what was already required.
#[derive(Clone, Debug)]
pub struct EventEnvelope<T: Clone> {
    pub payload: T,
    /// Span created at publish time, parented under whatever span the
    /// publisher was running in. Saga loops should use
    /// `.instrument(envelope.span.clone())` on their handler call.
    pub span: Span,
}

#[derive(Clone)]
pub struct EventBus<T: Clone> {
    sender: broadcast::Sender<EventEnvelope<T>>,
    /// Static event-type label, used as the name of the per-event span.
    /// Captured once at bus construction so we don't pay
    /// `std::any::type_name` per publish.
    event_type: &'static str,
}

impl<T: Clone> EventBus<T> {
    /// Create a new EventBus with default channel capacity of 1000.
    pub fn new() -> Self {
        Self::with_capacity(1000)
    }

    /// Create a new EventBus with custom channel capacity.
    pub fn with_capacity(capacity: usize) -> Self {
        let (sender, _) = broadcast::channel(capacity);
        Self {
            sender,
            event_type: std::any::type_name::<T>(),
        }
    }

    /// Publish an event to all subscribers.
    ///
    /// The bus wraps the event in an `EventEnvelope` that snapshots a
    /// fresh `event_enqueued` tracing span. Publisher code does not
    /// need to know about envelopes or spans.
    ///
    /// # Returns
    ///
    /// - `Ok(usize)` — number of receivers that received the event.
    /// - `Err(SendError)` — no active receivers.
    pub fn publish(&self, event: T) -> Result<usize, broadcast::error::SendError<T>> {
        // info_span! parents under whatever span is current in the
        // publisher's task — typically a #[tracing::instrument]
        // command handler. Recording event_type as a span field so
        // searches like `event_type=ConversationEvent` work in any
        // log aggregator.
        let span = tracing::info_span!(
            "event_enqueued",
            event_type = %self.event_type,
        );
        let envelope = EventEnvelope {
            payload: event,
            span,
        };
        self.sender.send(envelope).map_err(|broadcast::error::SendError(env)| {
            broadcast::error::SendError(env.payload)
        })
    }

    /// Subscribe to events from this bus.
    ///
    /// Returns a receiver that yields `EventEnvelope<T>`. Sagas should
    /// `.instrument(envelope.span.clone())` on their handler call so
    /// the handler's span is parented under the enqueue span.
    pub fn subscribe(&self) -> broadcast::Receiver<EventEnvelope<T>> {
        self.sender.subscribe()
    }

    /// Get the number of active subscribers.
    pub fn subscriber_count(&self) -> usize {
        self.sender.receiver_count()
    }
}

impl<T: Clone> Default for EventBus<T> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone, Debug, PartialEq)]
    struct TestEvent(u32);

    #[tokio::test]
    async fn publish_subscribe_round_trips_payload() {
        let bus = EventBus::<TestEvent>::new();
        let mut rx = bus.subscribe();
        bus.publish(TestEvent(42)).expect("send");
        let envelope = rx.recv().await.expect("recv");
        assert_eq!(envelope.payload, TestEvent(42));
    }

    #[tokio::test]
    async fn envelope_span_is_not_disabled() {
        // Smoke-test: the captured span should be a real, recordable
        // span (not Span::none()). We can't easily inspect parent/child
        // relationships without a Subscriber, but we can confirm the
        // span has metadata.
        let bus = EventBus::<TestEvent>::new();
        let mut rx = bus.subscribe();
        bus.publish(TestEvent(1)).expect("send");
        let envelope = rx.recv().await.expect("recv");
        assert!(envelope.span.metadata().is_some(), "span should have metadata");
        assert_eq!(envelope.span.metadata().map(|m| m.name()), Some("event_enqueued"));
    }

    #[tokio::test]
    async fn publish_with_no_subscribers_returns_send_error() {
        let bus = EventBus::<TestEvent>::new();
        let result = bus.publish(TestEvent(99));
        assert!(result.is_err());
        // Error variant carries the original payload, not the envelope —
        // we want to expose `T` to publishers, not internal types.
        if let Err(broadcast::error::SendError(payload)) = result {
            assert_eq!(payload, TestEvent(99));
        }
    }
}
