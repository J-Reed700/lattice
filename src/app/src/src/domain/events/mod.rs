//! Domain Events
//!
//! Per-feature event types live inside their feature slice and are
//! republished here for legacy import paths. The runtime broadcast
//! mechanism is `infrastructure::event_bus::EventBus<T>` — typed,
//! per-bounded-context, statically dispatched.

// Vertical-slice migration (download): event types live in features/download/events/.
#[path = "../../features/download/events/model_download_events.rs"]
pub mod model_download_events;
