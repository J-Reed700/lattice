//! Download feature event types.
//!
//! - `model_download_events` — model download domain events, broadcast on
//!   `infrastructure::event_bus::EventBus<ModelDownloadEvent>`.
//! - `infra_events` — outbound UI/transport event payloads.

pub mod infra_events;
pub mod model_download_events;
