//! [`VisionOcr`]: the [`OcrPort`] the indexer calls for each scanned page.

use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use tracing::info;

use super::client::OcrClient;
use super::model::OcrModelSource;
use super::render;
use super::server::{OcrServer, ServerLauncher};
use crate::application::ports::{OcrError, OcrPort};
use crate::shared::runtime::background::BackgroundTasks;

/// Bound on one page's request to the server. Apple Silicon reads a dense
/// page in about five seconds; the CPU-only build on an old laptop can take a
/// minute. Past this the model is stuck, and the page is left unindexed.
pub const OCR_PAGE_TIMEOUT: Duration = Duration::from_secs(180);

/// Reads pages with the downloaded OCR model, in a server of its own.
pub struct VisionOcr {
    models: Arc<dyn OcrModelSource>,
    server: OcrServer,
    client: OcrClient,
    page_timeout: Duration,
}

impl VisionOcr {
    pub fn new(
        models: Arc<dyn OcrModelSource>,
        launcher: Arc<dyn ServerLauncher>,
        background: Arc<BackgroundTasks>,
    ) -> Result<Self, OcrError> {
        Ok(Self {
            models,
            server: OcrServer::new(launcher, background),
            client: OcrClient::new()?,
            page_timeout: OCR_PAGE_TIMEOUT,
        })
    }

    pub fn with_page_timeout(mut self, page_timeout: Duration) -> Self {
        self.page_timeout = page_timeout;
        self
    }

    /// Stop the OCR server now rather than at the idle timeout.
    pub async fn stop(&self) {
        self.server.stop().await;
    }
}

#[async_trait]
impl OcrPort for VisionOcr {
    async fn recognize_page(&self, document: &Path, page: u32) -> Result<String, OcrError> {
        // Before rendering: without a model there is nothing to render for.
        let Some(model) = self.models.locate().await? else {
            return Err(OcrError::NotConfigured);
        };
        let started = Instant::now();
        let path = document.to_path_buf();
        // Rendering is CPU-bound and CoreGraphics may autorelease while it
        // decodes images, so it runs on the blocking pool inside a pool drain.
        let image = tokio::task::spawn_blocking(move || {
            crate::shared::runtime::with_autorelease_pool(|| render::page_image(&path, page))
        })
        .await
        .map_err(|error| OcrError::PageRender {
            page,
            reason: format!("the render task failed: {error}"),
        })??;
        let rendered = started.elapsed();
        let prompt = model.prompt.as_str();
        let text = self
            .server
            .run(&model, |server| async move {
                self.client
                    .recognize(&server, &image, prompt, page, self.page_timeout)
                    .await
            })
            .await?;
        info!(
            document = %document.display(),
            page,
            render_ms = rendered.as_millis() as u64,
            total_ms = started.elapsed().as_millis() as u64,
            chars = text.chars().count(),
            "OCR read a page"
        );
        Ok(text)
    }
}

#[cfg(test)]
#[path = "recognizer_tests.rs"]
mod tests;
