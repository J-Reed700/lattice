//! OpenDocument Text (ODT) extraction.

use super::archive_budget::{run_archive_work, ArchiveBudget};
use super::markup::{attr, decode_entities, tidy_lines, Markup, MarkupCursor};
use super::types::{ContentMetadata, ExtractedContent};
use crate::features::indexing::engine::error::{IndexingError, Result};
use std::fs::File;
use std::path::Path;
use zip::ZipArchive;

/// Extract content from ODT files.
pub async fn extract_odt(path: &Path, max_file_size: u64) -> Result<ExtractedContent> {
    let metadata = tokio::fs::metadata(path)
        .await
        .map_err(|e| IndexingError::Io {
            message: e.to_string(),
            kind: format!("{:?}", e.kind()),
        })?;

    let file_size = metadata.len();
    if file_size > max_file_size {
        return Err(IndexingError::FileTooLarge {
            path: path.display().to_string(),
            size_bytes: file_size,
            max_size_bytes: max_file_size,
        });
    }

    let path_clone = path.to_path_buf();
    let text = run_archive_work(path.to_path_buf(), move || extract_odt_sync(&path_clone)).await?;

    let metadata = ContentMetadata {
        page_count: None,
        word_count: text.split_whitespace().count(),
        char_count: text.chars().count(),
        language: None,
    };

    Ok(ExtractedContent {
        text,
        mime_type: "application/vnd.oasis.opendocument.text".to_string(),
        metadata,
        page_ranges: vec![],
        needs_ocr: Vec::new(),
    })
}

fn extract_odt_sync(path: &Path) -> Result<String> {
    let file = File::open(path).map_err(|e| IndexingError::FileRead {
        path: path.display().to_string(),
        reason: e.to_string(),
    })?;

    let mut archive = ZipArchive::new(file).map_err(|e| IndexingError::ContentExtraction {
        path: path.display().to_string(),
        reason: format!("Failed to open ODT as ZIP: {}", e),
    })?;
    let mut budget = ArchiveBudget::default();
    budget.check_members(&archive, path)?;
    let xml_content = budget.read_part(&mut archive, "content.xml", path)?;

    Ok(extract_text_from_odt_xml(&xml_content))
}

/// ODT element names do not collide by prefix the way OOXML's do, but it
/// shares the tag cursor so tabs (`<text:tab/>`), runs of spaces
/// (`<text:s text:c="3"/>`) and numeric character references survive.
fn extract_text_from_odt_xml(xml: &str) -> String {
    let mut out = String::new();
    let mut in_body = false;
    for event in MarkupCursor::new(xml) {
        match event {
            Markup::Open {
                name: "office:body",
                ..
            } => in_body = true,
            Markup::Close {
                name: "office:body",
            } => in_body = false,
            _ if !in_body => {}
            Markup::Close {
                name: "text:p" | "text:h" | "text:list-item",
            }
            | Markup::Open {
                name: "text:line-break",
                ..
            } => out.push('\n'),
            Markup::Open {
                name: "text:tab", ..
            } => out.push('\t'),
            Markup::Open {
                name: "text:s",
                attrs,
                ..
            } => {
                let count = attr(attrs, "text:c")
                    .and_then(|c| c.parse::<usize>().ok())
                    .unwrap_or(1);
                out.extend(std::iter::repeat_n(' ', count.min(64)));
            }
            Markup::Text(text) => out.push_str(&decode_entities(text)),
            _ => {}
        }
    }
    tidy_lines(&out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn odt_body_keeps_paragraphs_tabs_spaces_and_entities() {
        let xml = r#"<?xml version="1.0"?><office:document-content><office:automatic-styles><style:style style:name="P1"/></office:automatic-styles><office:body><office:text><text:h text:outline-level="1">Title</text:h><text:p>Hello &amp; goodbye<text:tab/>x<text:s text:c="2"/>y &#8212; z</text:p></office:text></office:body></office:document-content>"#;
        assert_eq!(
            extract_text_from_odt_xml(xml),
            "Title\nHello & goodbye\tx  y \u{2014} z"
        );
    }
}
