//! OCR wiring.
//!
//! Construction is cheap — no file I/O, no process, no model load — so binding
//! it costs app startup nothing. The server starts on the first scanned page.

use std::sync::Arc;

use tracing::warn;

use super::{DownloadedOcrModel, LlamaServerLauncher, VisionOcr};
use crate::application::ports::{ModelStoragePort, NoopOcr, OcrPort};
use crate::shared::runtime::background::BackgroundTasks;

/// The app's OCR port: the bundled `llama-server` reading pages with whichever
/// catalog OCR model is downloaded. Until one is, every page answers
/// `NotConfigured`, exactly as `NoopOcr` does.
pub fn build(
    models: Arc<dyn ModelStoragePort>,
    background: Arc<BackgroundTasks>,
) -> Arc<dyn OcrPort> {
    match VisionOcr::new(
        Arc::new(DownloadedOcrModel::new(models)),
        Arc::new(LlamaServerLauncher::bundled()),
        background,
    ) {
        Ok(ocr) => Arc::new(ocr),
        Err(error) => {
            warn!(%error, "OCR is off: scanned pages will stay unindexed");
            Arc::new(NoopOcr)
        }
    }
}
