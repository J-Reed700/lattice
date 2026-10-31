//! Speech feature dependency injection.
//!
//! Construction is cheap — no file I/O, no model load.

use std::sync::Arc;

use crate::features::speech::engine::KokoroSpeech;
use crate::interfaces::di::Container;
use lattice_core::domain::repositories::DownloadedModelRepository;
use lattice_core::shared::runtime::background::BackgroundTasks;

/// Build the read-aloud engine.
pub fn build(
    models: Arc<dyn DownloadedModelRepository>,
    background: Arc<BackgroundTasks>,
) -> Arc<KokoroSpeech> {
    Arc::new(KokoroSpeech::new(models, background))
}

/// Speech's registrar surface on `Container`.
impl Container {
    /// The read-aloud engine, one per process so one piece speaks at a time.
    pub fn speech(&self) -> Arc<KokoroSpeech> {
        Arc::clone(self.ai.speech())
    }
}
