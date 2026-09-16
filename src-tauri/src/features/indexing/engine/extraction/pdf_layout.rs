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

use lopdf::content::Content;
use lopdf::{Dictionary, Document, Encoding, Object, ObjectId};
use std::collections::BTreeMap;

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

/// Rough advance of one glyph in ems, good enough to say where a run ends.
///
/// Reading real widths would mean parsing `/Widths` or the embedded font
/// program for every subset on every page. These are Helvetica-ish averages;
/// what matters is that an `i` is not assumed to be as wide as a `W`, because
/// the error in that assumption is what decides whether a gap between two runs
/// looks like a space.
fn glyph_width(character: char) -> f32 {
    match character {
        'i' | 'j' | 'l' | 't' | 'f' | 'I' | '.' | ',' | ';' | ':' | '\'' | '`' | '!' | '|' => 0.3,
        ' ' => 0.28,
        'm' | 'w' | 'M' | 'W' | '@' => 0.85,
        '0'..='9' => 0.55,
        'A'..='Z' => 0.68,
        _ => 0.5,
    }
}

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

/// 2×3 affine matrix in PDF row-vector form: `[a b 0; c d 0; e f 1]`.
#[derive(Debug, Clone, Copy)]
struct Matrix {
    a: f32,
    b: f32,
    c: f32,
    d: f32,
    e: f32,
    f: f32,
}

impl Matrix {
    const IDENTITY: Self = Self {
        a: 1.0,
        b: 0.0,
        c: 0.0,
        d: 1.0,
        e: 0.0,
        f: 0.0,
    };

    fn translate(tx: f32, ty: f32) -> Self {
        Self {
            e: tx,
            f: ty,
            ..Self::IDENTITY
        }
    }

    /// `self` applied first, then `other`.
    fn then(self, other: Self) -> Self {
        Self {
            a: self.a * other.a + self.b * other.c,
            b: self.a * other.b + self.b * other.d,
            c: self.c * other.a + self.d * other.c,
            d: self.c * other.b + self.d * other.d,
            e: self.e * other.a + self.f * other.c + other.e,
            f: self.e * other.b + self.f * other.d + other.f,
        }
    }

    /// Uniform scale factor, used to turn a font size into device space.
    fn scale(&self) -> f32 {
        let determinant = (self.a * self.d - self.b * self.c).abs();
        if determinant.is_finite() && determinant > 0.0 {
            determinant.sqrt()
        } else {
            1.0
        }
    }
}

/// The advances a font declares for its character codes, in ems.
///
/// This is the difference between guessing where a run ends and knowing. A PDF
/// produced by a browser sets one glyph per positioned run with no space
/// characters anywhere: the only way to tell "Joshua Reed" from "JoshuaReed"
/// is to subtract the real glyph advance from the distance the producer moved.
struct FontWidths {
    /// Character code to advance in ems.
    by_code: BTreeMap<u32, f32>,
    /// Advance for a code the font does not list.
    default: f32,
    /// Codes are two bytes (a CID font). Word spacing does not apply to those.
    two_byte: bool,
    /// The font declared real metrics, so gaps can be trusted.
    measured: bool,
}

impl FontWidths {
    fn estimated() -> Self {
        Self {
            by_code: BTreeMap::new(),
            default: 0.5,
            two_byte: false,
            measured: false,
        }
    }

    /// Total advance of a shown string, plus the counts the spacing operators
    /// need: how many glyphs, and how many of them are single-byte spaces.
    fn measure(&self, bytes: &[u8]) -> (f32, usize, usize) {
        let mut width = 0.0;
        let mut glyphs = 0;
        let mut spaces = 0;
        if self.two_byte {
            for pair in bytes.chunks_exact(2) {
                let code = pair
                    .first()
                    .zip(pair.get(1))
                    .map(|(high, low)| u32::from(*high) << 8 | u32::from(*low))
                    .unwrap_or(0);
                width += self.by_code.get(&code).copied().unwrap_or(self.default);
                glyphs += 1;
            }
        } else {
            for byte in bytes {
                let code = u32::from(*byte);
                width += self.by_code.get(&code).copied().unwrap_or(self.default);
                glyphs += 1;
                if code == 32 {
                    spaces += 1;
                }
            }
        }
        (width, glyphs, spaces)
    }
}

/// Resolve a possibly indirect dictionary entry.
fn entry<'a>(doc: &'a Document, dictionary: &'a Dictionary, key: &[u8]) -> Option<&'a Object> {
    dictionary
        .get(key)
        .ok()
        .and_then(|object| doc.dereference(object).ok())
        .map(|(_, object)| object)
}

/// Read a font's advances from `/Widths` (simple fonts) or the descendant
/// CIDFont's `/W` (Type0 fonts). Falls back to per-character estimates.
fn font_widths(doc: &Document, font: &Dictionary) -> FontWidths {
    let subtype = font.get(b"Subtype").and_then(Object::as_name).ok();
    if subtype == Some(b"Type0") {
        return cid_widths(doc, font);
    }
    let first = entry(doc, font, b"FirstChar")
        .and_then(|object| object.as_i64().ok())
        .unwrap_or(0);
    let widths = entry(doc, font, b"Widths").and_then(|object| object.as_array().ok());
    let mut by_code = BTreeMap::new();
    for (offset, width) in widths.into_iter().flatten().enumerate() {
        let width = doc
            .dereference(width)
            .ok()
            .and_then(|(_, object)| object.as_float().ok());
        if let Some(width) = width.filter(|width| *width > 0.0) {
            let code = first.saturating_add(offset as i64);
            if (0..=255).contains(&code) {
                by_code.insert(code as u32, width / 1000.0);
            }
        }
    }
    FontWidths {
        measured: !by_code.is_empty(),
        by_code,
        default: 0.5,
        two_byte: false,
    }
}

/// Parse a CIDFont's `/W` array: `[ c [w1 w2 …] | cFirst cLast w ]*`.
fn cid_widths(doc: &Document, font: &Dictionary) -> FontWidths {
    let descendant = entry(doc, font, b"DescendantFonts")
        .and_then(|fonts| fonts.as_array().ok())
        .and_then(|fonts| fonts.first())
        .and_then(|font| doc.dereference(font).ok())
        .and_then(|(_, font)| font.as_dict().ok());
    let Some(descendant) = descendant else {
        return FontWidths {
            two_byte: true,
            ..FontWidths::estimated()
        };
    };
    let default = entry(doc, descendant, b"DW")
        .and_then(|width| width.as_float().ok())
        .unwrap_or(1000.0)
        / 1000.0;
    let mut by_code = BTreeMap::new();
    if let Some(widths) = entry(doc, descendant, b"W").and_then(|w| w.as_array().ok()) {
        let mut items = widths.iter();
        while let Some(start) = items.next().and_then(|object| object.as_i64().ok()) {
            match items.next() {
                Some(Object::Array(list)) => {
                    for (offset, width) in list.iter().enumerate() {
                        if let Ok(width) = width.as_float() {
                            let code = start.saturating_add(offset as i64);
                            if (0..=u32::MAX as i64).contains(&code) {
                                by_code.insert(code as u32, width / 1000.0);
                            }
                        }
                    }
                }
                Some(object) => {
                    let Ok(end) = object.as_i64() else { continue };
                    let Some(width) = items.next().and_then(|w| w.as_float().ok()) else {
                        continue;
                    };
                    // A malformed range must not allocate a map of millions.
                    if end < start || end.saturating_sub(start) > 0xFFFF {
                        continue;
                    }
                    for code in start..=end {
                        if (0..=u32::MAX as i64).contains(&code) {
                            by_code.insert(code as u32, width / 1000.0);
                        }
                    }
                }
                None => break,
            }
        }
    }
    FontWidths {
        measured: !by_code.is_empty(),
        by_code,
        default,
        two_byte: true,
    }
}

/// What a font resource contributes to layout.
struct FontInfo<'a> {
    bold: bool,
    encoding: Option<Encoding<'a>>,
    widths: FontWidths,
}

fn number(operands: &[Object], index: usize) -> f32 {
    operands
        .get(index)
        .and_then(|operand| operand.as_float().ok())
        .filter(|value| value.is_finite())
        .unwrap_or(0.0)
}

fn matrix_operand(operands: &[Object]) -> Matrix {
    Matrix {
        a: number(operands, 0),
        b: number(operands, 1),
        c: number(operands, 2),
        d: number(operands, 3),
        e: number(operands, 4),
        f: number(operands, 5),
    }
}

/// Weight markers that appear in PostScript font names. Subset prefixes
/// (`ABCDEF+`) and style suffixes both keep the marker, so a substring test is
/// enough — `ABCDEF+PalatinoLinotype-Bd` is as bold as `Helvetica-Bold`.
const BOLD_NAMES: [&str; 5] = ["bold", "-bd", "black", "heavy", "semib"];

/// The `ForceBold` flag in a font descriptor (PDF 32000-1 table 123, bit 19).
const FORCE_BOLD_FLAG: i64 = 1 << 18;

/// Stem thickness at or above which a font is a bold weight. Regular text
/// faces sit near 80; bold ones near 140.
const BOLD_STEM_V: f32 = 120.0;

fn looks_bold(bytes: &[u8]) -> bool {
    let name = String::from_utf8_lossy(bytes).to_ascii_lowercase();
    BOLD_NAMES.iter().any(|marker| name.contains(marker))
}

/// Whether the font's own descriptor declares a bold weight. Names lie or go
/// missing often enough — especially on subset CID fonts — that the metrics
/// are worth consulting.
fn descriptor_is_bold(doc: &Document, font: &Dictionary) -> bool {
    // A Type0 font keeps its metrics on the descendant CIDFont.
    let descendant = entry(doc, font, b"DescendantFonts")
        .and_then(|fonts| fonts.as_array().ok())
        .and_then(|fonts| fonts.first())
        .and_then(|font| doc.dereference(font).ok())
        .and_then(|(_, font)| font.as_dict().ok());
    let descriptor = entry(doc, descendant.unwrap_or(font), b"FontDescriptor")
        .and_then(|descriptor| descriptor.as_dict().ok());
    let Some(descriptor) = descriptor else {
        return false;
    };
    let forced = descriptor
        .get(b"Flags")
        .and_then(Object::as_i64)
        .is_ok_and(|flags| flags & FORCE_BOLD_FLAG != 0);
    let stems = descriptor
        .get(b"StemV")
        .and_then(Object::as_float)
        .is_ok_and(|stem| stem >= BOLD_STEM_V);
    let named = descriptor
        .get(b"FontName")
        .and_then(Object::as_name)
        .is_ok_and(looks_bold);
    forced || stems || named
}

fn page_fonts<'a>(
    doc: &'a Document,
    page_id: ObjectId,
) -> Result<BTreeMap<Vec<u8>, FontInfo<'a>>, String> {
    let fonts = doc
        .get_page_fonts(page_id)
        .map_err(|error| format!("page fonts could not be read: {error}"))?;
    Ok(fonts
        .into_iter()
        .map(|(name, dictionary)| {
            let base_font = dictionary.get(b"BaseFont").and_then(Object::as_name).ok();
            let bold = looks_bold(&name)
                || base_font.is_some_and(looks_bold)
                || descriptor_is_bold(doc, dictionary);
            let info = FontInfo {
                bold,
                encoding: dictionary.get_font_encoding(doc).ok(),
                widths: font_widths(doc, dictionary),
            };
            (name, info)
        })
        .collect())
}

/// Whether the page paints an image. Together with "almost no text" this is
/// what makes a page a scan rather than a deliberately blank one.
fn page_has_image(doc: &Document, page_id: ObjectId) -> bool {
    let Ok((resource_dict, resource_ids)) = doc.get_page_resources(page_id) else {
        return false;
    };
    let inherited = resource_ids
        .into_iter()
        .filter_map(|id| doc.get_dictionary(id).ok());
    resource_dict
        .into_iter()
        .chain(inherited)
        .any(|resources| dictionary_has_image(doc, resources))
}

fn dictionary_has_image(doc: &Document, resources: &Dictionary) -> bool {
    let Ok(xobjects) = resources.get(b"XObject") else {
        return false;
    };
    let xobjects = match xobjects {
        Object::Reference(id) => doc.get_object(*id).and_then(Object::as_dict).ok(),
        Object::Dictionary(dictionary) => Some(dictionary),
        _ => None,
    };
    let Some(xobjects) = xobjects else {
        return false;
    };
    xobjects.iter().any(|(_, value)| {
        let object = match value {
            Object::Reference(id) => doc.get_object(*id).ok(),
            other => Some(other),
        };
        object
            .and_then(|object| object.as_stream().ok())
            .and_then(|stream| stream.dict.get(b"Subtype").ok())
            .and_then(|subtype| subtype.as_name().ok())
            .is_some_and(|subtype| subtype == b"Image")
    })
}

/// Read one page's geometry.
///
/// Falls back to lopdf's plain extraction when the operator walk cannot be
/// trusted, calling `on_fallback` with the reason so the caller can log it.
/// Errors only when the page can be read neither way — an unreadable page must
/// fail the import rather than become an empty passage in a document the rest
/// of the application treats as fully indexed.
pub(super) fn read_page(
    doc: &Document,
    page: u32,
    page_id: ObjectId,
    on_fallback: impl FnOnce(String),
) -> Result<PageLayout, String> {
    let has_image = page_has_image(doc, page_id);
    let plain = |doc: &Document| {
        doc.extract_text(&[page])
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
            let recovered = plain(doc).ok().filter(|text| !text.is_empty());
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
                fallback_text: Some(plain(doc).map_err(|error| error.to_string())?),
                has_image,
            })
        }
    }
}

/// Walk one page's operators, emitting one run per text-showing operator.
///
/// Errors carry the reason the walk could not be trusted, which is the signal
/// to fall back to the plain path rather than index a page as empty.
fn page_runs(doc: &Document, page_id: ObjectId) -> Result<Vec<Run>, String> {
    let fonts = page_fonts(doc, page_id)?;
    let content = Content::decode(&doc.get_page_content(page_id))
        .map_err(|error| format!("content stream could not be decoded: {error}"))?;

    let mut runs = Vec::new();
    let mut text_matrix = Matrix::IDENTITY;
    let mut line_matrix = Matrix::IDENTITY;
    let mut transform = Matrix::IDENTITY;
    let mut transform_stack: Vec<Matrix> = Vec::new();
    let mut leading = 0.0_f32;
    let mut state = TextState::default();
    let mut font: Option<&FontInfo> = None;

    for operation in &content.operations {
        let operands = operation.operands.as_slice();
        match operation.operator.as_str() {
            "q" => transform_stack.push(transform),
            "Q" => transform = transform_stack.pop().unwrap_or(Matrix::IDENTITY),
            "cm" => transform = matrix_operand(operands).then(transform),
            "BT" => {
                text_matrix = Matrix::IDENTITY;
                line_matrix = Matrix::IDENTITY;
            }
            "Tf" => {
                font = operands
                    .first()
                    .and_then(|operand| operand.as_name().ok())
                    .and_then(|name| fonts.get(name));
                state.size = number(operands, 1);
            }
            "TL" => leading = number(operands, 0),
            "Tc" => state.char_spacing = number(operands, 0),
            "Tw" => state.word_spacing = number(operands, 0),
            "Tz" => state.horizontal = number(operands, 0) / 100.0,
            "Td" => {
                line_matrix =
                    Matrix::translate(number(operands, 0), number(operands, 1)).then(line_matrix);
                text_matrix = line_matrix;
            }
            "TD" => {
                leading = -number(operands, 1);
                line_matrix =
                    Matrix::translate(number(operands, 0), number(operands, 1)).then(line_matrix);
                text_matrix = line_matrix;
            }
            "Tm" => {
                line_matrix = matrix_operand(operands);
                text_matrix = line_matrix;
            }
            "T*" => {
                line_matrix = Matrix::translate(0.0, -leading).then(line_matrix);
                text_matrix = line_matrix;
            }
            // PDF 32000-1 §9.4.3: `'` is `T* Tj`, `"` is `aw Tw ac Tc T* Tj`.
            "Tj" | "TJ" | "'" | "\"" => {
                if operation.operator != "Tj" && operation.operator != "TJ" {
                    line_matrix = Matrix::translate(0.0, -leading).then(line_matrix);
                    text_matrix = line_matrix;
                }
                let shown: &[Object] = if operation.operator == "\"" {
                    operands.get(2).map(std::slice::from_ref).unwrap_or(&[])
                } else {
                    operands
                };
                // `"` also sets word and character spacing from its operands.
                if operation.operator == "\"" {
                    state.word_spacing = number(operands, 0);
                    state.char_spacing = number(operands, 1);
                }
                if let Some(run) = show_text(shown, font, &state, text_matrix.then(transform)) {
                    text_matrix = Matrix::translate(run.advance, 0.0).then(text_matrix);
                    if !run.run.text.trim().is_empty() {
                        runs.push(run.run);
                    }
                }
            }
            _ => {}
        }
    }
    Ok(runs)
}

/// Text state that decides how far a shown string advances.
#[derive(Debug, Clone, Copy)]
struct TextState {
    size: f32,
    char_spacing: f32,
    word_spacing: f32,
    /// Horizontal scaling as a multiplier (`Tz` / 100).
    horizontal: f32,
}

impl Default for TextState {
    fn default() -> Self {
        Self {
            size: 0.0,
            char_spacing: 0.0,
            word_spacing: 0.0,
            horizontal: 1.0,
        }
    }
}

/// A run plus the unscaled text-space advance it consumed.
struct ShownText {
    run: Run,
    advance: f32,
}

fn show_text(
    operands: &[Object],
    font: Option<&FontInfo>,
    state: &TextState,
    render_matrix: Matrix,
) -> Option<ShownText> {
    let font = font?;
    let encoding = font.encoding.as_ref()?;
    let mut text = String::new();
    // Advance in unscaled text space, so it can be fed back into the text
    // matrix the same way a real glyph advance would be.
    let mut advance = 0.0_f32;

    fn collect(
        operands: &[Object],
        font: &FontInfo,
        encoding: &Encoding,
        state: &TextState,
        text: &mut String,
        advance: &mut f32,
    ) {
        for operand in operands {
            match operand {
                Object::String(bytes, _) => {
                    let before = text.chars().count();
                    let _ = encoding.write_to_string(bytes, text);
                    let (width, glyphs, spaces) = if font.widths.measured {
                        font.widths.measure(bytes)
                    } else {
                        // No declared metrics: fall back to per-character
                        // averages over what the encoding actually produced.
                        let written: Vec<char> = text.chars().skip(before).collect();
                        let width = written.iter().copied().map(glyph_width).sum();
                        let spaces = written.iter().filter(|c| **c == ' ').count();
                        (width, written.len(), spaces)
                    };
                    *advance += (width * state.size
                        + glyphs as f32 * state.char_spacing
                        + spaces as f32 * state.word_spacing)
                        * state.horizontal;
                }
                Object::Array(items) => collect(items, font, encoding, state, text, advance),
                // Kerning, in thousandths of text space. A large negative
                // adjustment is how many producers render a space.
                Object::Integer(_) | Object::Real(_) => {
                    let adjustment = operand.as_float().unwrap_or(0.0);
                    if adjustment < SPACE_KERNING && !text.ends_with(' ') {
                        text.push(' ');
                    }
                    *advance -= adjustment / 1000.0 * state.size * state.horizontal;
                }
                _ => {}
            }
        }
    }

    collect(operands, font, encoding, state, &mut text, &mut advance);

    let scale = render_matrix.scale();
    Some(ShownText {
        run: Run {
            x: render_matrix.e,
            y: render_matrix.f,
            width: advance * scale,
            size: state.size * scale,
            bold: font.bold,
            measured: font.widths.measured,
            text,
        },
        advance,
    })
}

/// Group runs into visual lines, keeping the order the producer wrote them in.
///
/// Reading order is the producer's, not the geometry's. A two-column page is
/// normally written one column at a time, so sorting by descending y would
/// interleave the columns and destroy sentences; a page written row by row
/// across both columns still groups correctly here, because those runs are
/// adjacent in the stream. The cost is that a line whose runs are written out
/// of order (all the bold ones first, say) splits into several lines.
fn group_lines(runs: Vec<Run>) -> Vec<Line> {
    let mut lines: Vec<Line> = Vec::new();
    for run in runs {
        let tolerance = (run.size * 0.35).max(1.0);
        match lines.last_mut() {
            Some(line) if (line.y - run.y).abs() <= tolerance => line.runs.push(run),
            _ => lines.push(Line {
                y: run.y,
                runs: vec![run],
            }),
        }
    }
    for line in &mut lines {
        line.runs.sort_by(|left, right| left.x.total_cmp(&right.x));
    }
    lines
}

/// Round a size into half-point buckets so that 11.999 and 12.0 are one tier.
fn bucket(size: f32) -> u32 {
    (size.max(0.0) * 2.0).round() as u32
}

/// The document's body size: the size that sets the most characters.
///
/// A mode rather than a mean, because a mean is dragged upward by title pages
/// and downward by footnotes, and the body size is what every other decision
/// here is relative to.
fn body_size(pages: &[PageLayout]) -> f32 {
    let mut weights: BTreeMap<u32, usize> = BTreeMap::new();
    for line in pages.iter().flat_map(|page| page.lines.iter()) {
        *weights.entry(bucket(line.size())).or_default() += line.char_count();
    }
    weights
        .into_iter()
        // Ties go to the smaller size: body text outnumbers headings, so a tie
        // means the larger tier is decoration.
        .max_by_key(|(size, weight)| (*weight, std::cmp::Reverse(*size)))
        .map(|(size, _)| size as f32 / 2.0)
        .unwrap_or(0.0)
}

/// Size buckets that count as headings, largest first.
fn heading_tiers(pages: &[PageLayout], body: f32) -> Vec<u32> {
    if body <= 0.0 {
        return Vec::new();
    }
    let mut tiers: Vec<u32> = pages
        .iter()
        .flat_map(|page| page.lines.iter())
        .filter(|line| line.size() >= body * HEADING_RATIO && is_heading_shaped(line))
        .map(|line| bucket(line.size()))
        .collect();
    tiers.sort_unstable();
    tiers.dedup();
    tiers.reverse();
    tiers
}

/// A heading must be short enough that the chunker will recognise it.
fn is_heading_shaped(line: &Line) -> bool {
    let text = line.text();
    !text.is_empty() && text.chars().count() <= MAX_HEADING_CHARS
}

/// The markdown level for a line, or `None` if it is body text.
///
/// Known failure modes: a document typeset entirely in one size has no
/// headings (correctly — there is nothing to find); a document whose body is
/// bold throughout would promote every line, which the `size >= body`
/// requirement plus the ratio test for the non-bold case keeps rare.
fn heading_level(line: &Line, body: f32, tiers: &[u32]) -> Option<usize> {
    if body <= 0.0 || !is_heading_shaped(line) {
        return None;
    }
    let size = line.size();
    // Bold body-size lines sit below every size tier.
    let deepest = (2 + tiers.len()).min(DEEPEST_HEADING_LEVEL);
    if size >= body * HEADING_RATIO && !tiers.is_empty() {
        let tier = tiers
            .iter()
            .position(|tier| *tier == bucket(size))
            .unwrap_or(tiers.len());
        return Some((2 + tier).min(DEEPEST_HEADING_LEVEL));
    }
    // A bold run at body size is the other way documents mark a subheading.
    if line.bold() && size >= body {
        return Some(deepest);
    }
    None
}

/// Longest cell text a table row may carry. A cell is a datum; a sentence
/// that happens to start at a shared x is prose in a column layout.
const MAX_TABLE_CELL_CHARS: usize = 40;

/// Columns shared by a candidate table block, if there are enough of them.
///
/// Every run of every row must sit on one of the columns. That strictness is
/// deliberate: a two-column page of prose has its lines starting at exactly
/// two x-positions with words scattered between them, and admitting it as a
/// table would shred paragraphs into cells. The cost is that a table whose
/// cells are kerned into several positioned runs is not recognised.
///
/// Known failure modes: right-aligned numeric columns anchor at a different x
/// per row and will not cluster; a cell that wraps becomes its own row; a
/// three-column table with a merged cell is rejected.
fn table_columns(lines: &[Line], tolerance: f32) -> Option<Vec<f32>> {
    let first = lines.first()?;
    if first.runs.len() < MIN_TABLE_COLUMNS {
        return None;
    }
    let mut columns: Vec<f32> = Vec::new();
    for run in &first.runs {
        if !columns.iter().any(|x| (x - run.x).abs() <= tolerance) {
            columns.push(run.x);
        }
    }
    if columns.len() < MIN_TABLE_COLUMNS {
        return None;
    }
    for line in lines {
        if line.runs.len() != columns.len() {
            return None;
        }
        let aligned = line.runs.iter().all(|run| {
            run.text.chars().count() <= MAX_TABLE_CELL_CHARS
                && columns.iter().any(|x| (x - run.x).abs() <= tolerance)
        });
        if !aligned {
            return None;
        }
    }
    Some(columns)
}

/// The longest run of consecutive lines from `start` that share columns.
fn table_block(
    lines: &[Line],
    headings: &[bool],
    start: usize,
    tolerance: f32,
) -> Option<(usize, Vec<f32>)> {
    let mut end = start;
    let mut columns = None;
    while let Some(line) = lines.get(end) {
        if line.runs.len() < MIN_TABLE_COLUMNS || headings.get(end) == Some(&true) {
            break;
        }
        let candidate = lines
            .get(start..=end)
            .and_then(|block| table_columns(block, tolerance));
        match candidate {
            Some(found) => {
                columns = Some(found);
                end += 1;
            }
            None => break,
        }
    }
    let columns = columns?;
    if end.saturating_sub(start) < MIN_TABLE_ROWS {
        return None;
    }
    Some((end, columns))
}

fn escape_cell(text: &str) -> String {
    text.replace('|', "\\|")
}

fn table_row(line: &Line, columns: &[f32]) -> String {
    let mut cells = vec![String::new(); columns.len()];
    for run in &line.runs {
        let index = columns
            .iter()
            .enumerate()
            .min_by(|(_, left), (_, right)| {
                (*left - run.x).abs().total_cmp(&(*right - run.x).abs())
            })
            .map(|(index, _)| index)
            .unwrap_or(0);
        if let Some(cell) = cells.get_mut(index) {
            if !cell.is_empty() {
                cell.push(' ');
            }
            cell.push_str(run.text.trim());
        }
    }
    let cells: Vec<String> = cells.iter().map(|cell| escape_cell(cell.trim())).collect();
    format!("| {} |", cells.join(" | "))
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

fn render_page(page: &PageLayout, body: f32, tiers: &[u32], tolerance: f32) -> String {
    if let Some(text) = &page.fallback_text {
        return text.clone();
    }
    let levels: Vec<Option<usize>> = page
        .lines
        .iter()
        .map(|line| heading_level(line, body, tiers))
        .collect();
    let headings: Vec<bool> = levels.iter().map(Option::is_some).collect();
    let mut out = String::new();
    let mut index = 0;
    while let Some(line) = page.lines.get(index) {
        let level = levels.get(index).copied().flatten();
        // A heading is never a table row, even when it happens to have three
        // runs: promoting it would hide the structure the chunker wants most.
        if level.is_none() {
            if let Some((end, columns)) = table_block(&page.lines, &headings, index, tolerance) {
                for row_index in index..end {
                    let Some(row) = page.lines.get(row_index) else {
                        break;
                    };
                    out.push_str(&table_row(row, &columns));
                    out.push('\n');
                    if row_index == index {
                        let separator: Vec<&str> = columns.iter().map(|_| "---").collect();
                        out.push_str(&format!("| {} |\n", separator.join(" | ")));
                    }
                }
                index = end;
                continue;
            }
        }
        let text = line.text();
        if !text.is_empty() {
            if let Some(level) = level {
                out.push_str(&"#".repeat(level));
                out.push(' ');
            }
            out.push_str(&text);
            out.push('\n');
        }
        index += 1;
    }
    out.trim_end().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use lopdf::dictionary;

    fn run(x: f32, y: f32, size: f32, text: &str) -> Run {
        Run {
            x,
            y,
            width: text.chars().map(glyph_width).sum::<f32>() * size,
            size,
            bold: false,
            measured: false,
            text: text.into(),
        }
    }

    fn page(lines: Vec<Line>) -> PageLayout {
        PageLayout {
            page: 1,
            lines,
            fallback_text: None,
            has_image: false,
        }
    }

    #[test]
    fn body_size_is_the_size_that_sets_the_most_characters() {
        let pages = vec![page(group_lines(vec![
            run(50.0, 700.0, 24.0, "Title"),
            run(50.0, 660.0, 10.0, "a paragraph of ordinary body text"),
            run(50.0, 640.0, 10.0, "another paragraph of body text here"),
        ]))];

        assert_eq!(body_size(&pages), 10.0);
    }

    #[test]
    fn runs_on_the_same_baseline_become_one_line_in_reading_order() {
        let lines = group_lines(vec![
            run(200.0, 700.0, 10.0, "second"),
            run(50.0, 700.3, 10.0, "first"),
            run(50.0, 680.0, 10.0, "next line"),
        ]);

        assert_eq!(lines.len(), 2);
        assert_eq!(lines.first().map(Line::text), Some("first second".into()));
        assert_eq!(lines.get(1).map(Line::text), Some("next line".into()));
    }

    #[test]
    fn a_three_column_block_becomes_a_markdown_table() {
        let pages = vec![page(group_lines(vec![
            run(50.0, 700.0, 10.0, "Name"),
            run(200.0, 700.0, 10.0, "Role"),
            run(350.0, 700.0, 10.0, "Team"),
            run(50.0, 685.0, 10.0, "Ada"),
            run(200.0, 685.0, 10.0, "Engineer"),
            run(350.0, 685.0, 10.0, "Core"),
        ]))];

        let rendered = render(&pages).first().cloned().unwrap_or_default();

        assert_eq!(
            rendered,
            "| Name | Role | Team |\n| --- | --- | --- |\n| Ada | Engineer | Core |"
        );
    }

    #[test]
    fn a_single_wide_row_is_not_a_table() {
        let pages = vec![page(group_lines(vec![
            run(50.0, 700.0, 10.0, "one"),
            run(200.0, 700.0, 10.0, "two"),
            run(350.0, 700.0, 10.0, "three"),
            run(50.0, 685.0, 10.0, "an ordinary sentence follows the header"),
        ]))];

        let rendered = render(&pages).first().cloned().unwrap_or_default();

        assert!(!rendered.contains("---"), "unexpected table: {rendered}");
    }

    #[test]
    fn size_tiers_become_nested_markdown_headings() {
        let pages = vec![page(group_lines(vec![
            run(50.0, 700.0, 24.0, "Document Title"),
            run(50.0, 660.0, 14.0, "First Section"),
            run(
                50.0,
                640.0,
                10.0,
                "Body text that outweighs every heading on the page.",
            ),
            run(
                50.0,
                620.0,
                10.0,
                "More body text, so ten point is clearly the body.",
            ),
        ]))];

        let rendered = render(&pages).first().cloned().unwrap_or_default();

        assert!(rendered.starts_with("## Document Title\n"));
        assert!(rendered.contains("### First Section\n"));
        assert!(!rendered.contains("# Body text"));
    }

    #[test]
    fn a_bold_line_at_body_size_is_still_a_heading() {
        let mut bold = run(50.0, 660.0, 10.0, "Bold Subheading");
        bold.bold = true;
        let pages = vec![page(group_lines(vec![
            run(50.0, 700.0, 20.0, "Title"),
            bold,
            run(
                50.0,
                640.0,
                10.0,
                "Body text that outweighs every heading here.",
            ),
            run(
                50.0,
                620.0,
                10.0,
                "More body text so ten point wins the mode.",
            ),
        ]))];

        let rendered = render(&pages).first().cloned().unwrap_or_default();

        assert!(rendered.contains("### Bold Subheading"), "{rendered}");
    }

    #[test]
    fn an_overlong_line_is_never_promoted_to_a_heading() {
        let long = "word ".repeat(40);
        let pages = vec![page(group_lines(vec![
            run(50.0, 700.0, 24.0, &long),
            run(
                50.0,
                660.0,
                10.0,
                "Body text that outweighs the giant line.",
            ),
            run(
                50.0,
                640.0,
                10.0,
                "More body text so ten point wins the mode.",
            ),
        ]))];

        let rendered = render(&pages).first().cloned().unwrap_or_default();

        assert!(!rendered.contains('#'), "{rendered}");
    }

    #[test]
    fn simple_font_widths_come_from_the_widths_array() {
        let doc = Document::new();
        let font = dictionary! {
            "Type" => "Font",
            "Subtype" => "Type1",
            "FirstChar" => 65,
            "Widths" => vec![722.into(), 500.into(), 250.into()],
        };

        let widths = font_widths(&doc, &font);

        assert!(widths.measured);
        assert!(!widths.two_byte);
        // "AB" plus a space the font does not list.
        let (width, glyphs, spaces) = widths.measure(b"AB ");
        assert!((width - (0.722 + 0.5 + 0.5)).abs() < 0.001, "{width}");
        assert_eq!((glyphs, spaces), (3, 1));
    }

    #[test]
    fn cid_font_widths_parse_both_forms_of_the_w_array() {
        let mut doc = Document::new();
        let descendant = doc.add_object(dictionary! {
            "Type" => "Font",
            "Subtype" => "CIDFontType2",
            "DW" => 1000,
            // 1 => [600 700], then the range 10..=11 => 300.
            "W" => vec![
                1.into(),
                vec![600.into(), 700.into()].into(),
                10.into(),
                11.into(),
                300.into(),
            ],
        });
        let font = dictionary! {
            "Type" => "Font",
            "Subtype" => "Type0",
            "Encoding" => "Identity-H",
            "DescendantFonts" => vec![descendant.into()],
        };

        let widths = font_widths(&doc, &font);

        assert!(widths.measured);
        assert!(widths.two_byte);
        // Codes 1, 2 and 10 as two-byte big-endian, then an unlisted code.
        let (width, glyphs, spaces) = widths.measure(&[0, 1, 0, 2, 0, 10, 0, 99]);
        assert!((width - (0.6 + 0.7 + 0.3 + 1.0)).abs() < 0.001, "{width}");
        // Word spacing never applies to two-byte codes, so no space is counted.
        assert_eq!((glyphs, spaces), (4, 0));
    }

    #[test]
    fn a_font_without_metrics_falls_back_to_estimates() {
        let doc = Document::new();
        let font = dictionary! { "Type" => "Font", "Subtype" => "Type1" };

        assert!(!font_widths(&doc, &font).measured);
    }

    #[test]
    fn bold_names_are_recognised_through_subset_prefixes() {
        assert!(looks_bold(b"ABCDEF+Helvetica-Bold"));
        assert!(!looks_bold(b"Helvetica"));
    }
}
