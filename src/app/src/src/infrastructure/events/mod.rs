// Vertical-slice migration (conversation): events live in features/conversation/events.rs.
#[path = "../../features/conversation/events.rs"]
pub mod conversation_events;

pub use conversation_events::{ConversationEvent, SummaryRefreshRequestedEvent};
pub use crate::features::download::events::infra_events::{DownloadEvent, DownloadEventBridge, DownloadEventEmitter};
