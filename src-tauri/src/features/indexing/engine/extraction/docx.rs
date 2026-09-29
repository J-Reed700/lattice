//! Word document (DOCX) extraction.

use super::archive_budget::{run_archive_work, ArchiveBudget};
use super::markup::{decode_entities, tidy_lines, Markup, MarkupCursor};
use super::types::{ContentMetadata, ExtractedContent};
use crate::features::indexing::engine::error::{IndexingError, Result};
use std::fs::File;
use std::path::Path;
use zip::ZipArchive;

/// Extract content from DOCX files.
pub async fn extract_docx(path: &Path, max_file_size: u64) -> Result<ExtractedContent> {
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

    let text = run_archive_work(path.to_path_buf(), move || extract_docx_sync(&path_clone)).await?;

    let metadata = ContentMetadata {
        page_count: None,
        word_count: text.split_whitespace().count(),
        char_count: text.chars().count(),
        language: None,
    };

    Ok(ExtractedContent {
        text,
        mime_type: "application/vnd.openxmlformats-officedocument.wordprocessingml.document"
            .to_string(),
        metadata,
        page_ranges: vec![],
        needs_ocr: Vec::new(),
    })
}

/// Extract DOCX content synchronously (used in blocking task).
fn extract_docx_sync(path: &Path) -> Result<String> {
    let file = File::open(path).map_err(|e| IndexingError::FileRead {
        path: path.display().to_string(),
        reason: e.to_string(),
    })?;

    let mut archive = ZipArchive::new(file).map_err(|e| IndexingError::ContentExtraction {
        path: path.display().to_string(),
        reason: format!("Failed to open DOCX as ZIP: {}", e),
    })?;
    let mut budget = ArchiveBudget::default();
    budget.check_members(&archive, path)?;
    let xml_content = budget.read_part(&mut archive, "word/document.xml", path)?;

    let text = extract_text_from_docx_xml(&xml_content);

    Ok(text)
}

/// Extract text from DOCX XML content.
///
/// Word writes `document.xml` as a single line, so this walks the markup tag
/// by tag. Only `<w:t>` holds body text: `<w:delText>` (deleted revisions) and
/// `<w:instrText>` (field codes) are left out, and `<w:tab/>` / `<w:br/>` count
/// only inside a run, since `<w:tabs><w:tab …/>` in paragraph properties
/// defines tab stops rather than typing a tab.
fn extract_text_from_docx_xml(xml: &str) -> String {
    let mut out = String::new();
    let mut in_run = false;
    let mut in_text = false;

    for event in MarkupCursor::new(xml) {
        match event {
            Markup::Open {
                name: "w:r", empty, ..
            } => in_run = !empty,
            Markup::Close { name: "w:r" } => in_run = false,
            Markup::Open {
                name: "w:t", empty, ..
            } => in_text = !empty,
            Markup::Close { name: "w:t" } => in_text = false,
            Markup::Open { name: "w:tab", .. } if in_run => out.push('\t'),
            Markup::Open {
                name: "w:br" | "w:cr",
                ..
            } if in_run => out.push('\n'),
            Markup::Close { name: "w:p" } => out.push('\n'),
            Markup::Text(text) if in_text => out.push_str(&decode_entities(text)),
            _ => {}
        }
    }

    tidy_lines(&out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A body shaped the way Word saves it: one line, runs split mid-word, a
    /// tab stop definition, a typed tab, a line break and a two-cell table.
    const DOCUMENT_XML: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body><w:p><w:pPr><w:pStyle w:val="Title"/><w:tabs><w:tab w:val="left" w:pos="720"/></w:tabs></w:pPr><w:r><w:t>Cover</w:t></w:r><w:r><w:rPr><w:b/></w:rPr><w:t xml:space="preserve"> Letter</w:t></w:r></w:p><w:p><w:r><w:t>Name</w:t></w:r><w:r><w:tab/><w:t>Jo &amp; Co</w:t></w:r><w:r><w:br/><w:t>it&#8217;s here</w:t></w:r></w:p><w:tbl><w:tblPr><w:tblW w:w="0" w:type="auto"/></w:tblPr><w:tr><w:tc><w:p><w:r><w:t>Cell A</w:t></w:r></w:p></w:tc><w:tc><w:p><w:r><w:t>Cell B</w:t></w:r></w:p></w:tc></w:tr></w:tbl><w:p><w:r><w:delText>gone</w:delText></w:r><w:r><w:t>Last paragraph.</w:t></w:r></w:p><w:sectPr/></w:body></w:document>"#;

    #[test]
    fn a_single_line_document_keeps_every_run_and_paragraph() {
        let text = extract_text_from_docx_xml(DOCUMENT_XML);
        assert_eq!(
            text,
            "Cover Letter\nName\tJo & Co\nit\u{2019}s here\nCell A\nCell B\nLast paragraph."
        );
        assert!(!text.contains('<'));
    }

    #[tokio::test]
    async fn a_docx_file_is_read_from_its_zip() {
        use std::io::Write;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("letter.docx");
        let mut zip = zip::ZipWriter::new(File::create(&path).unwrap());
        zip.start_file("word/document.xml", zip::write::FileOptions::default())
            .unwrap();
        zip.write_all(DOCUMENT_XML.as_bytes()).unwrap();
        zip.finish().unwrap();

        let content = extract_docx(&path, 1 << 20).await.unwrap();
        assert!(content.text.starts_with("Cover Letter\n"));
        assert!(content.text.ends_with("Last paragraph."));
    }
}
