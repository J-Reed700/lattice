use std::sync::Arc;

use super::EmbeddingPort;

/// Read-only access to the loaded embedding model; never initiates loading.
/// Absence allows consumers to use their existing degraded-mode policy.
pub trait LoadedEmbeddingModelPort: Send + Sync {
    fn current_model(&self) -> Option<Arc<dyn EmbeddingPort>>;
}
