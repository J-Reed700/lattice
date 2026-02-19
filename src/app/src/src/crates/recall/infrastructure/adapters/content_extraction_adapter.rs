//! Content extraction adapter implementing ContentExtractionPort.

use crate::application::ports::content_extraction_port::{
    ContentExtractionPort, ExtractedContentData,
};
use crate::infrastructure::indexing::extraction::ContentExtractor;
use crate::shared::error::Result;
use async_trait::async_trait;
use std::path::Path;
use std::time::Instant;
use tracing::{info, instrument};

/// Adapter wrapping ContentExtractor as ContentExtractionPort.
pub struct ContentExtractionAdapter {
    extractor: ContentExtractor,
}

impl ContentExtractionAdapter {
    pub fn new() -> Self {
        Self {
            extractor: ContentExtractor::new(),
        }
    }
}

impl Default for ContentExtractionAdapter {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ContentExtractionPort for ContentExtractionAdapter {
    #[instrument(skip(self), fields(file_path = %path.display()))]
    async fn extract_content(&self, path: &Path) -> Result<ExtractedContentData> {
        let start = Instant::now();

        let extracted = self.extractor.extract_from_file(path).await?;

        let duration = start.elapsed();
        let file_extension = path
            .extension()
            .and_then(|ext| ext.to_str())
            .unwrap_or("unknown");

        info!(
            duration_ms = duration.as_millis(),
            file_type = file_extension,
            mime_type = %extracted.mime_type,
            word_count = extracted.metadata.word_count,
            char_count = extracted.metadata.char_count,
            page_count = ?extracted.metadata.page_count,
            "Content extraction completed"
        );

        Ok(ExtractedContentData {
            text: extracted.text,
            mime_type: extracted.mime_type,
            page_count: extracted.metadata.page_count,
            word_count: extracted.metadata.word_count,
            char_count: extracted.metadata.char_count,
        })
    }

    fn is_supported(&self, path: &Path) -> bool {
        self.extractor.is_supported(path)
    }
}
