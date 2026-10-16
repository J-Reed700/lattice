//! Port for facts a local model file records about itself in its own header.

use std::path::Path;

/// Reads facts a local model file records about itself.
pub trait ModelFileInspector: Send + Sync {
    /// Context length the model was trained with, when the file records one.
    /// An unreadable or unfamiliar file yields `None`.
    fn trained_context_length(&self, path: &Path) -> Option<u32>;
}
