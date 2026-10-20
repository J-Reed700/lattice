//! What a backend is serving now, as the app's own scheduler in front of it
//! counts it. Nothing here asks a server anything.

/// The requests one backend's scheduler is running and holding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequestLoad {
    /// The backend the requests share, the same for every role that reaches
    /// it, so two models' ports on one server are known to share one queue.
    pub backend: String,
    /// Requests the backend serves side by side.
    pub slots: usize,
    /// Admitted and in flight.
    pub running: usize,
    /// Waiting for a slot.
    pub queued: usize,
    /// Of `queued`, those a person is waiting on (a chat turn).
    pub queued_interactive: usize,
}
