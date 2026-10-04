//! In-process messaging and download event bridges.

pub mod command_channel;
pub mod event_bus;

pub use crate::features::download::events::infra_events::{
    DownloadEvent, DownloadEventBridge, DownloadEventEmitter,
};
