//! The embedding model as the activity view lists it: while it loads,
//! everything that embeds waits for it.

use super::runtime::EmbeddingRuntime;
use async_trait::async_trait;
use lattice_core::application::ports::{ActivityItem, ActivitySource, ActivityState};
use lattice_core::shared::Result;
use std::sync::Arc;

pub(crate) struct EmbeddingActivity {
    runtime: Arc<EmbeddingRuntime>,
}

impl EmbeddingActivity {
    pub(crate) fn new(runtime: Arc<EmbeddingRuntime>) -> Self {
        Self { runtime }
    }
}

#[async_trait]
impl ActivitySource for EmbeddingActivity {
    async fn activity(&self) -> Result<Vec<ActivityItem>> {
        if !self.runtime.is_loading() {
            return Ok(Vec::new());
        }
        Ok(vec![ActivityItem::new(
            "embedding.loading",
            "embedding.loading",
            "Loading the embedding model",
            ActivityState::Running,
        )
        .subject("the embedding model")
        .detail(
            "Searches and indexes that embed wait until it is ready",
        )])
    }
}
