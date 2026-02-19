//! Event Handlers
//!
//! Handlers for:
//! - Domain events (emitted by aggregates)
//! - File system events (file watcher)
//!
//! Event handlers are side effects triggered by domain operations
//! or external system events.

pub mod domain_events;
pub mod file_system_events;

pub use domain_events::{
    AuditLogSubscriber, DomainEventHandler, EventSubscriber, SearchIndexSubscriber,
};
pub use file_system_events::{start_file_watcher, FileSystemEventHandler};
