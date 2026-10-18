//! Font metrics: glyph advances from `/Widths` and `/W`, and bold detection.

use std::collections::BTreeMap;

use lopdf::{Dictionary, Document, Encoding, Object, ObjectId};

/// Rough advance of one glyph in ems, good enough to say where a run ends.
///
/// Reading real widths would mean parsing `/Widths` or the embedded font
/// program for every subset on every page. These are Helvetica-ish averages;
/// what matters is that an `i` is not assumed to be as wide as a `W`, because
/// the error in that assumption is what decides whether a gap between two runs
/// looks like a space.
pub(super) fn glyph_width(character: char) -> f32 {
    match character {
        'i' | 'j' | 'l' | 't' | 'f' | 'I' | '.' | ',' | ';' | ':' | '\'' | '`' | '!' | '|' => 0.3,
        ' ' => 0.28,
        'm' | 'w' | 'M' | 'W' | '@' => 0.85,
        '0'..='9' => 0.55,
        'A'..='Z' => 0.68,
        _ => 0.5,
    }
}

/// The advances a font declares for its character codes, in ems.
///
/// This is the difference between guessing where a run ends and knowing. A PDF
/// produced by a browser sets one glyph per positioned run with no space
/// characters anywhere: the only way to tell "Joshua Reed" from "JoshuaReed"
/// is to subtract the real glyph advance from the distance the producer moved.
pub(super) struct FontWidths {
    /// Character code to advance in ems.
    by_code: BTreeMap<u32, f32>,
    /// Advance for a code the font does not list.
    default: f32,
    /// Codes are two bytes (a CID font). Word spacing does not apply to those.
    pub(super) two_byte: bool,
    /// The font declared real metrics, so gaps can be trusted.
    pub(super) measured: bool,
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
    pub(super) fn measure(&self, bytes: &[u8]) -> (f32, usize, usize) {
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
pub(super) fn font_widths(doc: &Document, font: &Dictionary) -> FontWidths {
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
pub(super) struct FontInfo<'a> {
    pub(super) bold: bool,
    pub(super) encoding: Option<Encoding<'a>>,
    pub(super) widths: FontWidths,
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

pub(super) fn looks_bold(bytes: &[u8]) -> bool {
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

pub(super) fn page_fonts<'a>(
    doc: &'a Document,
    page_id: ObjectId,
) -> Result<BTreeMap<Vec<u8>, FontInfo<'a>>, String> {
    let fonts = doc
        .get_page_fonts(page_id)
        .map_err(|error| format!("page fonts could not be read: {error}"))?;
    fonts
        .into_iter()
        .map(|(name, dictionary)| {
            let base_font = dictionary.get(b"BaseFont").and_then(Object::as_name).ok();
            let bold = looks_bold(&name)
                || base_font.is_some_and(looks_bold)
                || descriptor_is_bold(doc, dictionary);
            let encoding = match dictionary
                .get_font_encoding_with_limit(doc, super::super::pdf::MAX_PDF_STREAM_BYTES)
            {
                Ok(encoding) => Some(encoding),
                Err(
                    error @ lopdf::Error::Decompress(lopdf::DecompressError::MemoryLimitExceeded {
                        ..
                    }),
                ) => return Err(error.to_string()),
                Err(_) => None,
            };
            let info = FontInfo {
                bold,
                encoding,
                widths: font_widths(doc, dictionary),
            };
            Ok((name, info))
        })
        .collect()
}
