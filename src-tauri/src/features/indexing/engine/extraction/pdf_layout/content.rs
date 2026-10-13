//! Walking a page's content-stream operators into positioned text runs.

use lopdf::content::Content;
use lopdf::{Dictionary, Document, Encoding, Object, ObjectId};

use super::fonts::{glyph_width, page_fonts, FontInfo};
use super::{Run, SPACE_KERNING};

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

/// Whether the page paints an image. Together with "almost no text" this is
/// what makes a page a scan rather than a deliberately blank one.
pub(super) fn page_has_image(doc: &Document, page_id: ObjectId) -> bool {
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

/// Walk one page's operators, emitting one run per text-showing operator.
///
/// Errors carry the reason the walk could not be trusted, which is the signal
/// to fall back to the plain path rather than index a page as empty.
pub(super) fn page_runs(doc: &Document, page_id: ObjectId) -> Result<Vec<Run>, String> {
    let fonts = page_fonts(doc, page_id)?;
    let page_content = doc
        .get_page_content_with_limit(page_id, super::super::pdf::MAX_PDF_STREAM_BYTES)
        .map_err(|error| format!("page content exceeds budget or is invalid: {error}"))?;
    let content = Content::decode(&page_content)
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
