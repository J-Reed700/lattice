//! Layout-aware reconstruction of a PDF page from its content-stream operators.
//!
//! `lopdf::Document::extract_text` concatenates every shown string in stream
//! order and throws the geometry away, so a heading and its body arrive as one
//! undifferentiated blob and a table arrives as a column-major word salad. The
//! chunker downstream splits on markdown headings, so losing them costs real
//! retrieval quality.
//!
//! This module walks the same operators but keeps the text state that matters:
//! the font size in force (`Tf`, scaled by the text and current transformation
//! matrices), the position of every shown run (`Td`/`TD`/`Tm`/`T*`/`cm`), and
//! how far each run advances — read from the font's own `/Widths` or `/W`
//! metrics, because a browser-produced PDF sets one glyph per positioned run
//! with no space characters at all, and only the leftover gap between a
//! glyph's real advance and the next glyph's origin says where words end.
//! From those it rebuilds visual lines, then marks up the two structures the
//! chunker understands:
//!
//! - **Headings** — a line set in at least [`HEADING_RATIO`]× the document's
//!   body size becomes `## `/`### `/`#### ` by size tier; a bold line at body
//!   size becomes the deepest tier.
//! - **Tables** — consecutive lines whose runs land on the same three or more
//!   x-positions become a markdown table with a header separator row.
//!
//! Everything here is a heuristic over a format that only describes ink. The
//! known failure modes are documented on the functions that own them.

use lopdf::{Document, ObjectId};

mod content;
mod fonts;
mod render;

use content::{page_has_image, page_runs};
use render::{body_size, group_lines, heading_tiers, render_page};

/// A line at or above this multiple of the body size is a heading.
const HEADING_RATIO: f32 = 1.2;

/// Deepest markdown level a size tier can produce. `embedding_input` accepts
/// `#`..`######`; capping at 4 keeps a document with many near-body sizes from
/// producing a heading hierarchy nobody wrote.
const DEEPEST_HEADING_LEVEL: usize = 4;

/// A heading is a title, not a paragraph. Matches the length ceiling in
/// `embedding_input::heading`, which would otherwise ignore the marker we emit
/// and leave a stray `## ` in the indexed text.
const MAX_HEADING_CHARS: usize = 140;

/// Minimum columns for a line to look like a table row.
const MIN_TABLE_COLUMNS: usize = 3;

/// Minimum rows (including the header) for a block to be worth a table.
const MIN_TABLE_ROWS: usize = 2;

/// Gap between two runs, in ems, that means a word boundary rather than
/// kerning, when the font's real advances are known. With real metrics an
/// intra-word gap measures within a hundredth of an em of zero, while a
/// tightly tracked space still clears a tenth, so the bar sits low.
const MEASURED_GAP_SPACE: f32 = 0.10;

/// The same threshold when the run's width had to be estimated per character.
/// The estimate can be half an em wrong on a short run, so the bar is higher:
/// a missing space between two runs is a smaller loss than a space inserted
/// into the middle of a kerned word, which corrupts the token the search index
/// is built from.
const ESTIMATED_GAP_SPACE: f32 = 0.6;

/// A `TJ` adjustment below this many thousandths of an em is a rendered space
/// rather than a kerning pair. Kerning pairs are rarely wider than a tenth of
/// an em; a typeset space is a quarter to a third.
const SPACE_KERNING: f32 = -200.0;

/// A shown text run, anchored where it starts on the page.
#[derive(Debug, Clone)]
struct Run {
    /// Device-space x of the first glyph.
    x: f32,
    /// Device-space y of the baseline. PDF y grows upward.
    y: f32,
    /// Estimated device-space width.
    width: f32,
    /// Effective font size in device space.
    size: f32,
    bold: bool,
    /// The font declared real advances, so `width` is exact rather than a
    /// per-character guess.
    measured: bool,
    text: String,
}

/// Runs that share a baseline, in left-to-right order.
#[derive(Debug, Clone)]
struct Line {
    y: f32,
    runs: Vec<Run>,
}

impl Line {
    /// The largest size on the line: a line is as prominent as its biggest run.
    fn size(&self) -> f32 {
        self.runs.iter().map(|r| r.size).fold(0.0_f32, f32::max)
    }

    fn bold(&self) -> bool {
        !self.runs.is_empty() && self.runs.iter().all(|r| r.bold)
    }

    fn char_count(&self) -> usize {
        self.runs.iter().map(|r| r.text.chars().count()).sum()
    }

    /// Joined text, inserting a space wherever one run starts clear of where
    /// the previous one ended.
    ///
    /// Browser-produced PDFs set one glyph per positioned run and no space
    /// characters at all, so this is the only place their word boundaries can
    /// come from.
    fn text(&self) -> String {
        let mut out = String::new();
        let mut previous_end: Option<(f32, bool)> = None;
        for run in &self.runs {
            if let Some((end, measured)) = previous_end {
                let gap = run.x - end;
                let threshold = if measured {
                    MEASURED_GAP_SPACE
                } else {
                    ESTIMATED_GAP_SPACE
                };
                let needs_space = gap > run.size * threshold
                    && !out.ends_with(' ')
                    && !run.text.starts_with(' ')
                    && !out.is_empty();
                if needs_space {
                    out.push(' ');
                }
            }
            out.push_str(&run.text);
            previous_end = Some((run.x + run.width, run.measured));
        }
        out.trim().to_string()
    }
}

/// One page as geometry, before document-wide decisions are made.
pub(super) struct PageLayout {
    pub page: u32,
    lines: Vec<Line>,
    /// Set when the operator walk failed and lopdf's plain extraction stood in.
    fallback_text: Option<String>,
    /// The page draws at least one image XObject.
    pub has_image: bool,
}

impl PageLayout {
    pub(super) fn retained_bytes(&self) -> usize {
        std::mem::size_of::<Self>()
            + self.fallback_text.as_ref().map_or(0, String::capacity)
            + self.lines.capacity() * std::mem::size_of::<Line>()
            + self
                .lines
                .iter()
                .map(|line| {
                    line.runs.capacity() * std::mem::size_of::<Run>()
                        + line
                            .runs
                            .iter()
                            .map(|run| run.text.capacity())
                            .sum::<usize>()
                })
                .sum::<usize>()
    }
}

/// Read one page's geometry.
///
/// Falls back to lopdf's plain extraction when the operator walk cannot be
/// trusted, calling `on_fallback` with the reason so the caller can log it.
/// Errors only when the page can be read neither way — an unreadable page must
/// fail the import rather than become an empty passage in a document the rest
/// of the application treats as fully indexed. The error is lopdf's, so the
/// caller can name the file it came from.
pub(super) fn read_page(
    doc: &Document,
    page: u32,
    page_id: ObjectId,
    on_fallback: impl FnOnce(String),
) -> lopdf::Result<PageLayout> {
    let has_image = page_has_image(doc, page_id);
    let plain = |doc: &Document| {
        doc.extract_text_with_limit(&[page], super::pdf::MAX_PDF_STREAM_BYTES)
            .map(|text| text.trim().to_string())
    };
    match page_runs(doc, page_id) {
        Ok(runs) if !runs.is_empty() => Ok(PageLayout {
            page,
            lines: group_lines(runs),
            fallback_text: None,
            has_image,
        }),
        // The walk found no positioned text. That is what a scanned page looks
        // like, but also what a font we cannot decode looks like, so give the
        // plain path a chance before calling the page textless.
        Ok(_) => {
            let text = plain(doc)?;
            let recovered = (!text.is_empty()).then_some(text);
            if recovered.is_some() {
                on_fallback("no positioned text runs were recovered".into());
            }
            Ok(PageLayout {
                page,
                lines: Vec::new(),
                fallback_text: recovered,
                has_image,
            })
        }
        Err(reason) => {
            on_fallback(reason);
            Ok(PageLayout {
                page,
                lines: Vec::new(),
                fallback_text: Some(plain(doc)?),
                has_image,
            })
        }
    }
}

/// Render every page to markdown, using document-wide type sizes.
pub(super) fn render(pages: &[PageLayout]) -> Vec<String> {
    let body = body_size(pages);
    let tiers = heading_tiers(pages, body);
    let tolerance = (body * 0.6).max(4.0);
    pages
        .iter()
        .map(|page| render_page(page, body, &tiers, tolerance))
        .collect()
}

#[cfg(test)]
mod tests;
