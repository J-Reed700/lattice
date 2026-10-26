//! What a fetched body is, and its text when it is not HTML.
//!
//! Every body used to be decoded and parsed as HTML. A PDF, a plain-text RFC,
//! a raw file on GitHub or a JSON answer then came back as no text at all, and
//! was kept as a page of nothing: the open-access PDFs the paper finders lead
//! to could not be read.

use lattice_core::shared::error::{AppError, Result};

use super::decode_response_text;
use crate::features::indexing::engine::extraction::ContentExtractor;

/// How a body is read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum BodyKind {
    Html,
    Pdf,
    /// Text as it is: plain text, markdown, CSV, JSON, XML.
    Text,
}

/// What `body` is, from its declared type and, for a PDF served under a
/// generic one, its first bytes.
pub(super) fn body_kind(content_type: Option<&str>, body: &[u8]) -> BodyKind {
    if body.starts_with(b"%PDF-") {
        return BodyKind::Pdf;
    }
    let declared = content_type
        .and_then(|value| value.split(';').next())
        .map(|value| value.trim().to_ascii_lowercase())
        .unwrap_or_default();
    match declared.as_str() {
        "application/pdf" | "application/x-pdf" => BodyKind::Pdf,
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
        BodyKind::Text | BodyKind::Html => {
            Ok(decode_response_text(body, content_type).trim().to_string())
        }
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
    }
}
