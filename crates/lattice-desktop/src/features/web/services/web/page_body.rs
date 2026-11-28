//! What a fetched body is, and its text when it is not HTML: PDFs, plain
//! text, raw files and JSON are read rather than parsed as empty HTML.

use lattice_core::shared::error::{AppError, Result};

use super::decode_response_text;
use crate::features::indexing::engine::extraction::ContentExtractor;

/// How a body is read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum BodyKind {
    Html,
    Pdf,
    /// Binary ZIP / OpenXML archives that must not be decoded as raw text.
    Archive,
    /// Text as it is: plain text, markdown, CSV, JSON, XML.
    Text,
}

/// What `body` is, from its declared type and, for a PDF served under a
/// generic one, its first bytes.
pub(super) fn body_kind(content_type: Option<&str>, body: &[u8]) -> BodyKind {
    if body.starts_with(b"%PDF-") {
        return BodyKind::Pdf;
    }
    if body.starts_with(b"PK\x03\x04") || body.starts_with(b"PK\x05\x06") {
        return BodyKind::Archive;
    }
    let declared = content_type
        .and_then(|value| value.split(';').next())
        .map(|value| value.trim().to_ascii_lowercase())
        .unwrap_or_default();
    match declared.as_str() {
        "application/pdf" | "application/x-pdf" => BodyKind::Pdf,
        "application/zip" | "application/x-zip-compressed" => BodyKind::Archive,
        other if other.starts_with("application/vnd.openxmlformats-officedocument.") => {
            BodyKind::Archive
        }
        "text/html" | "application/xhtml+xml" | "" => BodyKind::Html,
        "application/json" | "application/xml" | "text/xml" => BodyKind::Text,
        other if other.starts_with("text/") => BodyKind::Text,
        other if other.ends_with("+json") || other.ends_with("+xml") => BodyKind::Text,
        _ => BodyKind::Html,
    }
}

/// The text of a non-HTML body. A PDF is read by the same extractor the
/// library indexes PDFs with, from a temporary file it is written to.
pub(super) async fn body_text(
    kind: BodyKind,
    body: Vec<u8>,
    content_type: Option<&str>,
) -> Result<String> {
    match kind {
        BodyKind::Pdf => pdf_text(body).await,
        BodyKind::Archive => Err(AppError::InvalidInput(
            "Fetched URL returned binary ZIP/archive bytes instead of readable text.".into(),
        )),
        BodyKind::Text | BodyKind::Html => Ok(lattice_core::shared::text::strip_scraped_chrome(
            decode_response_text(body, content_type).trim(),
        )),
    }
}

async fn pdf_text(body: Vec<u8>) -> Result<String> {
    let file = tokio::task::spawn_blocking(move || -> std::io::Result<tempfile::TempPath> {
        let mut file = tempfile::Builder::new().suffix(".pdf").tempfile()?;
        std::io::Write::write_all(&mut file, &body)?;
        Ok(file.into_temp_path())
    })
    .await
    .map_err(|error| AppError::Other(format!("Could not hold the PDF to read it: {error}")))?
    .map_err(|error| AppError::Other(format!("Could not hold the PDF to read it: {error}")))?;
    let extracted = ContentExtractor::new()
        .extract_from_file(&file)
        .await
        .map_err(|error| AppError::Other(format!("Could not read the PDF's text: {error}")))?;
    Ok(extracted.text.trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_body_is_read_by_what_it_is() {
        assert_eq!(
            body_kind(Some("text/html; charset=utf-8"), b"<html>"),
            BodyKind::Html
        );
        assert_eq!(body_kind(None, b"<html>"), BodyKind::Html);
        assert_eq!(body_kind(Some("application/pdf"), b"x"), BodyKind::Pdf);
        // A PDF served as a download is still a PDF.
        assert_eq!(
            body_kind(Some("application/octet-stream"), b"%PDF-1.7\n"),
            BodyKind::Pdf
        );
        assert_eq!(body_kind(Some("text/plain"), b"RFC 9110"), BodyKind::Text);
        assert_eq!(body_kind(Some("application/json"), b"{}"), BodyKind::Text);
        assert_eq!(
            body_kind(Some("application/ld+json"), b"{}"),
            BodyKind::Text
        );
    }

    #[tokio::test]
    async fn a_text_body_is_its_own_text() {
        let text = body_text(
            BodyKind::Text,
            b"  Section 1. Scope  ".to_vec(),
            Some("text/plain"),
        )
        .await
        .unwrap();
        assert_eq!(text, "Section 1. Scope");

        let stripped = body_text(
            BodyKind::Text,
            b"[View a markdown version of this page](https://docs.aws.amazon.com/guide.md)\n\n# Real Heading".to_vec(),
            Some("text/markdown"),
        )
        .await
        .unwrap();
        assert_eq!(stripped, "# Real Heading");

        assert_eq!(
            body_kind(Some("text/html"), b"PK\x03\x04\x14\x00[Content_Types].xml"),
            BodyKind::Archive
        );
        assert_eq!(
            body_kind(Some("application/zip"), b"binary"),
            BodyKind::Archive
        );
        assert_eq!(
            body_kind(
                Some("application/vnd.openxmlformats-officedocument.wordprocessingml.document"),
                b"binary",
            ),
            BodyKind::Archive
        );
        assert!(body_text(
            BodyKind::Archive,
            b"PK\x03\x04[Content_Types].xml".to_vec(),
            Some("application/octet-stream"),
        )
        .await
        .is_err());
    }
}
