//! Audit sink implementations.
//!
//! This module provides different implementations of the `AuditSink` trait
//! for persisting audit events to various storage backends.

pub mod memory;
pub mod sqlite;

pub use memory::MemoryAuditSink;
pub use sqlite::SqliteAuditSink;
