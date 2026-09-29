//! PowerPoint (PPTX) extraction.

use super::archive_budget::{run_archive_work, ArchiveBudget};
use super::markup::{attr, decode_entities, tidy_lines, Markup, MarkupCursor};
use super::types::{ContentMetadata, ExtractedContent};
use crate::features::indexing::engine::error::{IndexingError, Result};
use std::fs::File;
use std::path::Path;
use zip::ZipArchive;

/// Extract content from PPTX files.
pub async fn extract_pptx(path: &Path, max_file_size: u64) -> Result<ExtractedContent> {
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
    let text = run_archive_work(path.to_path_buf(), move || extract_pptx_sync(&path_clone)).await?;

    let metadata = ContentMetadata {
        page_count: None,
        word_count: text.split_whitespace().count(),
        char_count: text.chars().count(),
        language: None,
    };

    Ok(ExtractedContent {
        text,
        mime_type: "application/vnd.openxmlformats-officedocument.presentationml.presentation"
            .to_string(),
        metadata,
        page_ranges: vec![],
        needs_ocr: Vec::new(),
    })
}

fn extract_pptx_sync(path: &Path) -> Result<String> {
    let file = File::open(path).map_err(|e| IndexingError::FileRead {
        path: path.display().to_string(),
        reason: e.to_string(),
    })?;

    let mut archive = ZipArchive::new(file).map_err(|e| IndexingError::ContentExtraction {
        path: path.display().to_string(),
        reason: format!("Failed to open PPTX as ZIP: {}", e),
    })?;
    let mut budget = ArchiveBudget::default();
    budget.check_members(&archive, path)?;

    // Zip order is whatever the writer chose, and "slide10" sorts before
    // "slide2" as text; the number in the part name is the deck order.
    let mut slides: Vec<(u32, String)> = archive
        .file_names()
        .filter_map(|name| Some((part_number(name, "ppt/slides/slide")?, name.to_owned())))
        .collect();
    slides.sort();

    let mut sections = Vec::new();
    for (number, name) in slides {
        let body = drawing_text(&budget.read_part(&mut archive, &name, path)?);
        let notes = match notes_part(&mut archive, &name, path, &mut budget)? {
            Some(notes_name) => drawing_text(&budget.read_part(&mut archive, &notes_name, path)?),
            None => String::new(),
        };
        if body.is_empty() && notes.is_empty() {
            continue;
        }
        // A heading per slide lets the section splitter keep each slide's
        // text, and its notes, in their own span.
        let mut section = format!("# Slide {number}");
        if !body.is_empty() {
            section.push('\n');
            section.push_str(&body);
        }
        if !notes.is_empty() {
            section.push_str("\nSpeaker notes:\n");
            section.push_str(&notes);
        }
        sections.push(section);
    }

    Ok(sections.join("\n\n"))
}

/// `N` from `{prefix}N.xml`.
fn part_number(name: &str, prefix: &str) -> Option<u32> {
    name.strip_prefix(prefix)?
        .strip_suffix(".xml")?
        .parse()
        .ok()
}

/// The notes part a slide links to. Notes are numbered independently of
/// slides (deleting a slide does not renumber them), so the slide's
/// relationships are the only reliable link.
fn notes_part(
    archive: &mut ZipArchive<File>,
    slide: &str,
    path: &Path,
    budget: &mut ArchiveBudget,
) -> Result<Option<String>> {
    let Some(file_name) = slide.strip_prefix("ppt/slides/") else {
        return Ok(None);
    };
    let rels_name = format!("ppt/slides/_rels/{file_name}.rels");
    if archive.by_name(&rels_name).is_err() {
        return Ok(None);
    }
    let rels = budget.read_part(archive, &rels_name, path)?;
    Ok(notes_target(&rels).filter(|name| archive.by_name(name).is_ok()))
}

fn notes_target(rels: &str) -> Option<String> {
    MarkupCursor::new(rels).find_map(|event| match event {
        Markup::Open {
            name: "Relationship",
            attrs,
            ..
        } if attr(attrs, "Type").is_some_and(|t| t.ends_with("/notesSlide")) => {
            let target = attr(attrs, "Target")?;
            Some(match target.strip_prefix("../") {
                Some(rest) => format!("ppt/{rest}"),
                None => match target.strip_prefix('/') {
                    Some(absolute) => absolute.to_owned(),
                    None => format!("ppt/slides/{target}"),
                },
            })
        }
        _ => None,
    })
}

/// Text of a DrawingML part: runs join within a paragraph, paragraphs end a
/// line. `<a:t>` must match exactly, since `<a:tbl>`, `<a:tc>` and `<a:tab>`
/// share its prefix. Field text (`<a:fld>`: slide numbers, dates) is
/// placeholder output rather than content, so it is left out.
fn drawing_text(xml: &str) -> String {
    let mut out = String::new();
    let mut in_text = false;
    let mut in_field = false;

    for event in MarkupCursor::new(xml) {
        match event {
            Markup::Open {
                name: "a:fld",
                empty,
                ..
            } => in_field = !empty,
            Markup::Close { name: "a:fld" } => in_field = false,
            Markup::Open {
                name: "a:t", empty, ..
            } => in_text = !empty,
            Markup::Close { name: "a:t" } => in_text = false,
            Markup::Open { name: "a:br", .. } => out.push('\n'),
            Markup::Close { name: "a:p" } => out.push('\n'),
            Markup::Text(text) if in_text && !in_field => out.push_str(&decode_entities(text)),
            _ => {}
        }
    }

    tidy_lines(&out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    const TABLE_SLIDE: &str = r#"<?xml version="1.0" encoding="UTF-8"?><p:sld xmlns:a="a" xmlns:p="p"><p:cSld><p:spTree><p:sp><p:txBody><a:p><a:r><a:t>Quarterly</a:t></a:r><a:r><a:t xml:space="preserve"> results</a:t></a:r></a:p></p:txBody></p:sp><p:graphicFrame><a:graphic><a:graphicData><a:tbl><a:tblGrid><a:gridCol w="100"/></a:tblGrid><a:tr h="10"><a:tc><a:txBody><a:p><a:r><a:t>Revenue</a:t></a:r></a:p></a:txBody></a:tc><a:tc><a:txBody><a:p><a:r><a:t>12 &amp; up</a:t></a:r></a:p></a:txBody></a:tc></a:tr></a:tbl></a:graphicData></a:graphic></p:graphicFrame></p:spTree></p:cSld></p:sld>"#;

    fn write_deck(parts: &[(&str, &str)]) -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("deck.pptx");
        let mut zip = zip::ZipWriter::new(File::create(&path).unwrap());
        for (name, body) in parts {
            zip.start_file(*name, zip::write::FileOptions::default())
                .unwrap();
            zip.write_all(body.as_bytes()).unwrap();
        }
        zip.finish().unwrap();
        (dir, path)
    }

    #[test]
    fn a_table_slide_yields_cell_text_and_no_markup() {
        let text = drawing_text(TABLE_SLIDE);
        assert_eq!(text, "Quarterly results\nRevenue\n12 & up");
    }

    #[tokio::test]
    async fn slides_come_in_number_order_with_their_speaker_notes() {
        let slide = |t: &str| format!(r#"<p:sld><a:p><a:r><a:t>{t}</a:t></a:r></a:p></p:sld>"#);
        let rels = r#"<?xml version="1.0"?><Relationships><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/slideLayout" Target="../slideLayouts/slideLayout1.xml"/><Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/notesSlide" Target="../notesSlides/notesSlide7.xml"/></Relationships>"#;
        let notes = r#"<p:notes><a:p><a:r><a:t>Say the numbers slowly.</a:t></a:r></a:p><a:p><a:fld type="slidenum"><a:t>10</a:t></a:fld></a:p></p:notes>"#;
        let (_dir, path) = write_deck(&[
            ("ppt/slides/slide10.xml", &slide("Tenth")),
            ("ppt/slides/slide2.xml", &slide("Second")),
            ("ppt/slides/_rels/slide10.xml.rels", rels),
            ("ppt/notesSlides/notesSlide7.xml", notes),
            ("ppt/slides/slide1.xml", TABLE_SLIDE),
        ]);

        let text = extract_pptx(&path, 1 << 20).await.unwrap().text;
        assert_eq!(
            text,
            "# Slide 1\nQuarterly results\nRevenue\n12 & up\n\n# Slide 2\nSecond\n\n# Slide 10\nTenth\nSpeaker notes:\nSay the numbers slowly."
        );
    }
}
