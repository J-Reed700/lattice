//! Streaming text extraction for large files.

use crate::infrastructure::indexing::error::{IndexingError, Result};
use tokio::io::{AsyncBufReadExt, AsyncRead, BufReader};

/// Extract text from a stream line by line.
pub async fn extract_text_streaming<R: AsyncRead + Unpin>(
    reader: R,
    mime_type: &str,
) -> Result<Vec<String>> {
    match mime_type {
        "text/plain" | "text/markdown" => stream_text(reader).await,
        _ => Err(IndexingError::UnsupportedFileType {
            path: "stream".to_string(),
            detected_type: mime_type.to_string(),
        }),
    }
}

/// Stream text content line by line.
async fn stream_text<R: AsyncRead + Unpin>(reader: R) -> Result<Vec<String>> {
    let mut lines = BufReader::new(reader).lines();
    let mut result = Vec::new();

    while let Some(line) = lines
        .next_line()
        .await
        .map_err(|e| IndexingError::FileRead {
            path: "stream".to_string(),
            reason: e.to_string(),
        })?
    {
        result.push(line);
    }

    Ok(result)
}
