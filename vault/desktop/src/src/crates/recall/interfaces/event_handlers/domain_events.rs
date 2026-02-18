//! Domain Event Handlers
//!
//! Handlers for domain events emitted by aggregates.
//! These are side effects triggered by domain operations.

use crate::audit_success;
use crate::domain::events::DomainEvent;
use crate::shared::error::Result;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{error, info};

/// Domain event handler
pub struct DomainEventHandler {
    // Event subscribers
    subscribers: Arc<RwLock<Vec<Box<dyn EventSubscriber + Send + Sync>>>>,
}

/// Event subscriber trait
#[async_trait::async_trait]
pub trait EventSubscriber {
    /// Handle a domain event
    async fn handle(&self, event: &DomainEvent) -> Result<()>;
}

impl Default for DomainEventHandler {
    fn default() -> Self {
        Self::new()
    }
}

impl DomainEventHandler {
    pub fn new() -> Self {
        Self {
            subscribers: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Register an event subscriber
    pub async fn subscribe(&self, subscriber: Box<dyn EventSubscriber + Send + Sync>) {
        self.subscribers.write().await.push(subscriber);
    }

    /// Publish a domain event to all subscribers
    pub async fn publish(&self, event: DomainEvent) -> Result<()> {
        info!("Publishing domain event: {:?}", event);

        let subscribers = self.subscribers.read().await;
        for subscriber in subscribers.iter() {
            if let Err(e) = subscriber.handle(&event).await {
                error!("Event handler failed: {}", e);
                // Continue processing other subscribers
            }
        }

        Ok(())
    }
}

/// Example: Audit log subscriber
pub struct AuditLogSubscriber;

#[async_trait::async_trait]
impl EventSubscriber for AuditLogSubscriber {
    async fn handle(&self, event: &DomainEvent) -> Result<()> {
        match event {
            DomainEvent::DocumentIndexed(e) => {
                let logger = crate::infrastructure::audit::get_audit_logger();
                audit_success!(logger, crate::infrastructure::audit::AuditAction::FileIndexed, e.document_id.as_str(),
                    "chunks_count" => e.chunks_count.to_string()
                ).await.ok();
            }
            DomainEvent::DocumentDeleted(e) => {
                let logger = crate::infrastructure::audit::get_audit_logger();
                audit_success!(
                    logger,
                    crate::infrastructure::audit::AuditAction::FileDeleted,
                    e.document_id.as_str()
                )
                .await
                .ok();
            }
            DomainEvent::TagAdded(e) => {
                let logger = crate::infrastructure::audit::get_audit_logger();
                audit_success!(logger, crate::infrastructure::audit::AuditAction::FileUpdated, e.document_id.as_str(),
                    "tag_id" => e.tag_id.as_str()
                ).await.ok();
            }
        }

        Ok(())
    }
}

/// Example: Search index subscriber
pub struct SearchIndexSubscriber {
    // Would hold reference to search index
}

#[async_trait::async_trait]
impl EventSubscriber for SearchIndexSubscriber {
    async fn handle(&self, event: &DomainEvent) -> Result<()> {
        match event {
            DomainEvent::DocumentIndexed(e) => {
                info!("Updating search index for document: {}", e.document_id);
                // Update search index
            }
            DomainEvent::DocumentDeleted(e) => {
                info!("Removing from search index: {}", e.document_id);
                // Remove from search index
            }
            _ => {}
        }

        Ok(())
    }
}
