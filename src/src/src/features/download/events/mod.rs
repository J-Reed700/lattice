//! Download feature event types.
//!
//! `model_download_events` is loaded via Strangler Fig at
//! `crate::domain::events::model_download_events` — that redirect
//! stays, consumers import via the domain aggregator.

pub mod infra_events;
