//! Port for facts a local model file records about itself in its own header.

use std::path::Path;

/// Reads facts a local model file records about itself.
pub trait ModelFileInspector: Send + Sync {
    /// Context length the model was trained with, when the file records one.
    /// An unreadable or unfamiliar file yields `None`.
    fn trained_context_length(&self, path: &Path) -> Option<u32>;

    /// Whether the model's chat template can call tools. `None` when the file
    /// cannot be read.
    fn supports_tool_calling(&self, path: &Path) -> Option<bool>;
}
