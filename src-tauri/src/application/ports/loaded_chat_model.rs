use std::sync::Arc;

use super::LLMPort;

/// Read-only access to the currently loaded chat model.
///
/// This does not initiate loading. Callers that require a model must arrange
/// loading first and handle absence if configuration changes in between.
/// Returned handles remain valid across later invalidation.
pub trait LoadedChatModelPort: Send + Sync {
    fn current_model(&self) -> Option<Arc<dyn LLMPort>>;
}
