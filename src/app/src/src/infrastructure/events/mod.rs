// Vertical-slice migration (conversation): events live in features/conversation/events.rs.
#[path = "../../features/conversation/events.rs"]
pub mod conversation_events;
// Vertical-slice migration (download): infrastructure event bridge lives in features/download/events/.
#[path = "../../features/download/events/infra_events.rs"]
pub mod download_events;

pub use conversation_events::{ConversationEvent, SummaryRefreshRequestedEvent};
pub use download_events::{DownloadEvent, DownloadEventBridge, DownloadEventEmitter};
