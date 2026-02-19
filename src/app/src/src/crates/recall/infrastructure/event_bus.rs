use tokio::sync::broadcast;

/// Generic EventBus that can handle any event type
///
/// This design allows the application to create multiple event buses
/// for different domains (e.g., ModelDownloadEvents, UserEvents, etc.)
/// without code duplication or type coupling.
///
/// # Type Parameters
///
/// * `T` - The event type this bus will handle. Must be Clone for broadcasting.
///
/// # Example
///
/// ```
/// use crate::infrastructure::event_bus::EventBus;
/// use crate::domain::events::model_download_events::ModelDownloadEvent;
///
/// // Create a bus for model download events
/// let download_bus = EventBus::<ModelDownloadEvent>::new();
///
/// // Publish events
/// let event = ModelDownloadEvent::Completed(...);
/// download_bus.publish(event).ok();
///
/// // Subscribe to events
/// let mut receiver = download_bus.subscribe();
/// ```
#[derive(Clone)]
pub struct EventBus<T: Clone> {
    sender: broadcast::Sender<T>,
}

impl<T: Clone> EventBus<T> {
    /// Create a new EventBus with default channel capacity of 1000
    pub fn new() -> Self {
        let (sender, _) = broadcast::channel(1000);
        Self { sender }
    }

    /// Create a new EventBus with custom channel capacity
    pub fn with_capacity(capacity: usize) -> Self {
        let (sender, _) = broadcast::channel(capacity);
        Self { sender }
    }

    /// Publish an event to all subscribers
    ///
    /// # Returns
    ///
    /// - `Ok(usize)` - Number of receivers that received the event
    /// - `Err(SendError)` - If there are no active receivers
    pub fn publish(&self, event: T) -> Result<usize, broadcast::error::SendError<T>> {
        self.sender.send(event)
    }

    /// Subscribe to events from this bus
    ///
    /// # Returns
    ///
    /// A receiver that can be used to listen for events
    pub fn subscribe(&self) -> broadcast::Receiver<T> {
        self.sender.subscribe()
    }

    /// Get the number of active subscribers
    pub fn subscriber_count(&self) -> usize {
        self.sender.receiver_count()
    }
}

impl<T: Clone> Default for EventBus<T> {
    fn default() -> Self {
        Self::new()
    }
}
