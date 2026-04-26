//! Single-consumer command channel for in-process work delegation.
//!
//! Sibling to `EventBus<T>` (broadcast pub/sub for fan-out events).
//! Use this when:
//!
//! - There is exactly ONE producer and ONE consumer.
//! - The message is a *command* — "please do this work" — not a
//!   notification. Lost commands cause user-visible bugs (e.g., a
//!   summary refresh that silently never happens), so the channel
//!   must be lossless.
//! - You want backpressure: if the consumer is slow, the producer's
//!   `send().await` should block until there is room rather than
//!   silently drop.
//!
//! Use `EventBus<T>` instead when there are multiple subscribers, or
//! when the messages are observability/notifications where dropping
//! under load is acceptable.
//!
//! # Tracing context propagation
//!
//! Like `EventBus<T>`, every send wraps the payload in an envelope
//! that snapshots a fresh `command_enqueued` child span at publish
//! time. Consumers `.instrument(envelope.span)` their handler so a
//! single user-action trace covers command → channel → handler →
//! downstream work.

use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};
use tracing::Span;

/// Wraps every command with the tracing span captured at send time.
///
/// `tracing::Span` is `Clone` (atomic refcount on the underlying span
/// data); kept that way for symmetry with `EventEnvelope<T>`.
#[derive(Debug)]
pub struct CommandEnvelope<T> {
    pub payload: T,
    /// Span created at send time, parented under the sender's current
    /// span. Consumers should `.instrument(envelope.span.clone())`
    /// their handler call.
    pub span: Span,
}

/// Producer half of a command channel. `Clone`-able so multiple
/// publishers can share it (though the typical usage is single
/// producer single consumer).
pub struct CommandSender<T> {
    tx: mpsc::Sender<CommandEnvelope<T>>,
    /// Static command-type label, captured once at construction.
    command_type: &'static str,
}

impl<T> Clone for CommandSender<T> {
    fn clone(&self) -> Self {
        Self {
            tx: self.tx.clone(),
            command_type: self.command_type,
        }
    }
}

impl<T> CommandSender<T> {
    /// Send a command, awaiting backpressure if the channel is full.
    ///
    /// Wraps the payload in an envelope with a fresh
    /// `command_enqueued` span snapshotted from the caller's current
    /// tracing context.
    ///
    /// # Returns
    ///
    /// - `Ok(())` — command was queued.
    /// - `Err(SendError)` — the consumer half has been dropped; no
    ///   further commands can be sent on this channel.
    pub async fn send(&self, command: T) -> Result<(), mpsc::error::SendError<T>> {
        let span = tracing::info_span!(
            "command_enqueued",
            command_type = %self.command_type,
        );
        let envelope = CommandEnvelope {
            payload: command,
            span,
        };
        self.tx
            .send(envelope)
            .await
            .map_err(|mpsc::error::SendError(env)| mpsc::error::SendError(env.payload))
    }

    /// Send a command without awaiting backpressure. Returns
    /// `TrySendError::Full` if the channel buffer is full.
    ///
    /// Prefer `send()`. Use this only when blocking the publisher is
    /// unacceptable AND dropping the command on Full is the right
    /// behavior (it usually isn't — that's why we have a channel).
    pub fn try_send(&self, command: T) -> Result<(), mpsc::error::TrySendError<T>> {
        let span = tracing::info_span!(
            "command_enqueued",
            command_type = %self.command_type,
        );
        let envelope = CommandEnvelope {
            payload: command,
            span,
        };
        self.tx.try_send(envelope).map_err(|err| match err {
            mpsc::error::TrySendError::Full(env) => mpsc::error::TrySendError::Full(env.payload),
            mpsc::error::TrySendError::Closed(env) => {
                mpsc::error::TrySendError::Closed(env.payload)
            }
        })
    }

    /// Returns true if the consumer half has been dropped.
    pub fn is_closed(&self) -> bool {
        self.tx.is_closed()
    }
}

/// Consumer half of a command channel. Wrapped in `Mutex` so it can
/// be shared via `Arc` even though `mpsc::Receiver` is not `Sync`;
/// the saga calls `lock()` once at startup and holds it for the
/// channel's lifetime.
pub struct CommandReceiver<T> {
    rx: Mutex<mpsc::Receiver<CommandEnvelope<T>>>,
}

impl<T> CommandReceiver<T> {
    /// Receive the next command, awaiting until one arrives.
    ///
    /// Returns `None` when the producer half has been dropped and the
    /// channel buffer is empty (clean shutdown signal — the consumer
    /// should exit its loop).
    pub async fn recv(&self) -> Option<CommandEnvelope<T>> {
        self.rx.lock().await.recv().await
    }
}

/// Construct a new command channel with the given buffer capacity.
///
/// Capacity sizing: this is a *command* channel, so capacity should
/// reflect "how many in-flight commands can stack up while the
/// consumer is briefly slow". For low-frequency commands like
/// summary-refresh, 16-32 is plenty — at burst that gives the
/// consumer 16-32 commands of slack before the publisher waits.
pub fn channel<T>(capacity: usize) -> (CommandSender<T>, Arc<CommandReceiver<T>>) {
    let (tx, rx) = mpsc::channel(capacity);
    let sender = CommandSender {
        tx,
        command_type: std::any::type_name::<T>(),
    };
    let receiver = Arc::new(CommandReceiver {
        rx: Mutex::new(rx),
    });
    (sender, receiver)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, PartialEq)]
    struct Cmd(u32);

    #[tokio::test]
    async fn send_and_recv_round_trips_payload() {
        let (tx, rx) = channel::<Cmd>(8);
        tx.send(Cmd(42)).await.expect("send");
        let envelope = rx.recv().await.expect("recv");
        assert_eq!(envelope.payload, Cmd(42));
    }

    #[tokio::test]
    async fn envelope_carries_command_enqueued_span() {
        let (tx, rx) = channel::<Cmd>(4);
        tx.send(Cmd(1)).await.expect("send");
        let envelope = rx.recv().await.expect("recv");
        assert_eq!(
            envelope.span.metadata().map(|m| m.name()),
            Some("command_enqueued")
        );
    }

    #[tokio::test]
    async fn recv_returns_none_after_sender_dropped() {
        let (tx, rx) = channel::<Cmd>(4);
        tx.send(Cmd(7)).await.expect("send");
        drop(tx);
        // Drain the buffered command first.
        assert_eq!(rx.recv().await.map(|e| e.payload), Some(Cmd(7)));
        // Now the channel is closed and empty.
        assert!(rx.recv().await.is_none());
    }

    #[tokio::test]
    async fn send_propagates_backpressure() {
        let (tx, rx) = channel::<Cmd>(2);
        tx.send(Cmd(1)).await.expect("send");
        tx.send(Cmd(2)).await.expect("send");
        // Channel is full. Try non-blocking send to confirm.
        let try_result = tx.try_send(Cmd(3));
        assert!(matches!(
            try_result,
            Err(mpsc::error::TrySendError::Full(_))
        ));
        // Drain to free space.
        let _ = rx.recv().await;
        // Now try_send succeeds again.
        tx.try_send(Cmd(3)).expect("try_send after drain");
    }

    #[tokio::test]
    async fn send_returns_error_when_receiver_dropped() {
        let (tx, rx) = channel::<Cmd>(4);
        drop(rx);
        let err = tx.send(Cmd(99)).await.expect_err("should fail");
        // Error variant exposes the original payload type, not the envelope.
        assert_eq!(err.0, Cmd(99));
    }
}
