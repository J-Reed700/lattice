//! Turning a document page into an image the OCR model can read.
//!
//! PDF pages are rasterized by CoreGraphics on macOS: it decodes every image
//! codec a scan can use (CCITT, JBIG2, JPX, DCT) and composites whatever else
//! the page draws, which is why the port hands over a page number rather than
//! an extracted image. PNG and JPEG files are passed through untouched;
//! `llama-server` decodes both.

use std::path::Path;

use base64::prelude::{Engine as _, BASE64_STANDARD};

use lattice_core::application::ports::OcrError;

/// Nominal rendering resolution. 150 dpi keeps 8-point type legible.
const RENDER_DPI: f64 = 150.0;

/// Pixel ceiling for one rendered page: 150 dpi on a US Letter page. The
/// projector resizes every image to at most 1,003,520 pixels anyway, so a
/// poster or an A3 plan rendered at full resolution would only cost encode
/// time and request size.
const MAX_RENDER_PIXELS: f64 = 2_200_000.0;

/// Largest image file sent as-is. A phone photo is a few megabytes; anything
/// past this is not a document snapshot.
const MAX_IMAGE_FILE_BYTES: u64 = 25 * 1024 * 1024;

/// An encoded page image.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PageImage {
    pub bytes: Vec<u8>,
    pub mime: &'static str,
}

impl PageImage {
    /// The image as an OpenAI-style `image_url` value.
    pub fn data_uri(&self) -> String {
        format!(
            "data:{};base64,{}",
            self.mime,
            BASE64_STANDARD.encode(&self.bytes)
        )
    }
}

/// The MIME type of an image file the OCR model can read directly.
pub fn image_mime(path: &Path) -> Option<&'static str> {
    let extension = path.extension()?.to_str()?.to_ascii_lowercase();
    match extension.as_str() {
        "png" => Some("image/png"),
        "jpg" | "jpeg" => Some("image/jpeg"),
        _ => None,
    }
}

/// Produce the image for one-based `page` of `document`. Blocking: run it on
/// the blocking pool.
pub fn page_image(document: &Path, page: u32) -> Result<PageImage, OcrError> {
    match image_mime(document) {
        Some(mime) => image_file(document, page, mime),
        None => render_pdf_page(document, page),
    }
}

fn image_file(path: &Path, page: u32, mime: &'static str) -> Result<PageImage, OcrError> {
    let fail = |reason: String| OcrError::PageRender { page, reason };
    if page != 1 {
        return Err(fail("an image file has only page 1".into()));
    }
    // repository-barrier-allow: size check on the image being read, before it is read whole.
    let size = std::fs::metadata(path)
        .map_err(|error| fail(error.to_string()))?
        .len();
    if size > MAX_IMAGE_FILE_BYTES {
        return Err(fail(format!(
            "the image is {} MB; the largest read is {} MB",
            size / (1024 * 1024),
            MAX_IMAGE_FILE_BYTES / (1024 * 1024)
        )));
    }
    let bytes = std::fs::read(path).map_err(|error| fail(error.to_string()))?;
    // The extension is a claim; the server rejects the whole request on bytes
    // it cannot decode, so check the signature first and say which file.
    let signature_ok = match mime {
        "image/png" => bytes.starts_with(b"\x89PNG\r\n\x1a\n"),
        _ => bytes.starts_with(&[0xFF, 0xD8, 0xFF]),
    };
    if !signature_ok {
        return Err(fail(format!("the file is not a {mime} image")));
    }
    Ok(PageImage { bytes, mime })
}

/// Pixel dimensions and scale (pixels per point) for a page of
/// `width_pt` × `height_pt` points, or `None` for a page with no area.
pub(crate) fn render_size(width_pt: f64, height_pt: f64) -> Option<(usize, usize, f64)> {
    if !(width_pt.is_finite() && height_pt.is_finite() && width_pt >= 1.0 && height_pt >= 1.0) {
        return None;
    }
    let mut scale = RENDER_DPI / 72.0;
    let pixels = width_pt * height_pt * scale * scale;
    if pixels > MAX_RENDER_PIXELS {
        scale *= (MAX_RENDER_PIXELS / pixels).sqrt();
    }
    let width = (width_pt * scale).round().max(1.0) as usize;
    let height = (height_pt * scale).round().max(1.0) as usize;
    Some((width, height, scale))
}

/// Encode tightly packed 8-bit RGB rows as a PNG.
#[cfg(target_os = "macos")]
fn encode_png(rgb: &[u8], width: usize, height: usize) -> Result<Vec<u8>, String> {
    let width = u32::try_from(width).map_err(|error| error.to_string())?;
    let height = u32::try_from(height).map_err(|error| error.to_string())?;
    let mut out = Vec::new();
    let mut encoder = png::Encoder::new(&mut out, width, height);
    encoder.set_color(png::ColorType::Rgb);
    encoder.set_depth(png::BitDepth::Eight);
    // Fast: the image is decoded once, a few milliseconds later, on the same
    // machine. Size only matters for the base64 request body.
    encoder.set_compression(png::Compression::Fast);
    let mut writer = encoder.write_header().map_err(|error| error.to_string())?;
    writer
        .write_image_data(rgb)
        .map_err(|error| error.to_string())?;
    writer.finish().map_err(|error| error.to_string())?;
    Ok(out)
}

#[cfg(target_os = "macos")]
fn render_pdf_page(path: &Path, page: u32) -> Result<PageImage, OcrError> {
    macos::render(path, page)
}

#[cfg(not(target_os = "macos"))]
fn render_pdf_page(_path: &Path, _page: u32) -> Result<PageImage, OcrError> {
    Err(OcrError::Unavailable {
        reason: "reading scanned PDF pages is only available on macOS for now".into(),
    })
}

#[cfg(target_os = "macos")]
mod macos {
    use std::path::Path;

    use objc2_core_foundation::{CGPoint, CGRect, CGSize, CFURL};
    use objc2_core_graphics::{
        CGBitmapContextCreate, CGColorSpace, CGContext, CGImageAlphaInfo, CGInterpolationQuality,
        CGPDFBox, CGPDFDocument, CGPDFPage,
    };

    use super::{encode_png, render_size, PageImage};
    use lattice_core::application::ports::OcrError;

    pub(super) fn render(path: &Path, page: u32) -> Result<PageImage, OcrError> {
        let fail = |reason: &str| OcrError::PageRender {
            page,
            reason: reason.to_string(),
        };
        let url = CFURL::from_file_path(path).ok_or_else(|| fail("the path is not a file"))?;
        let document = CGPDFDocument::with_url(Some(&url))
            .ok_or_else(|| fail("CoreGraphics could not open the PDF"))?;
        if !CGPDFDocument::is_unlocked(Some(&document)) {
            return Err(fail("the PDF is password-protected"));
        }
        let pdf_page = CGPDFDocument::page(Some(&document), page as usize)
            .ok_or_else(|| fail("the PDF has no such page"))?;

        let crop = CGPDFPage::box_rect(Some(&pdf_page), CGPDFBox::CropBox);
        let quarter_turn = matches!(
            CGPDFPage::rotation_angle(Some(&pdf_page)).rem_euclid(360),
            90 | 270
        );
        let (width_pt, height_pt) = if quarter_turn {
            (crop.size.height, crop.size.width)
        } else {
            (crop.size.width, crop.size.height)
        };
        let (width, height, scale) =
            render_size(width_pt, height_pt).ok_or_else(|| fail("the page has no area"))?;
        let rgbx = draw(&pdf_page, width, height, scale)
            .ok_or_else(|| fail("CoreGraphics could not create a bitmap"))?;
        let rgb: Vec<u8> = rgbx
            .chunks_exact(4)
            .flat_map(|pixel| pixel.iter().take(3).copied())
            .collect();
        let bytes = encode_png(&rgb, width, height).map_err(|reason| fail(&reason))?;
        Ok(PageImage {
            bytes,
            mime: "image/png",
        })
    }

    /// Draw `page` into a white `width` × `height` RGBX bitmap, top row first.
    fn draw(page: &CGPDFPage, width: usize, height: usize, scale: f64) -> Option<Vec<u8>> {
        let bytes_per_row = width.checked_mul(4)?;
        let mut pixels = vec![0u8; bytes_per_row.checked_mul(height)?];
        let space = CGColorSpace::new_device_rgb()?;
        // SAFETY: `pixels` holds exactly `bytes_per_row * height` bytes, the
        // size an 8-bit, 4-byte-per-pixel (RGB + skipped alpha) bitmap of
        // these dimensions needs, and it outlives `context`, which is dropped
        // below before `pixels` is read or moved.
        #[allow(unsafe_code)]
        let context = unsafe {
            CGBitmapContextCreate(
                pixels.as_mut_ptr().cast(),
                width,
                height,
                8,
                bytes_per_row,
                Some(&space),
                CGImageAlphaInfo::NoneSkipLast.0,
            )
        }?;
        let ctx = Some(&*context);
        let full = CGRect::new(CGPoint::ZERO, CGSize::new(width as f64, height as f64));
        // A scan composited over nothing would come out on black.
        CGContext::set_rgb_fill_color(ctx, 1.0, 1.0, 1.0, 1.0);
        CGContext::fill_rect(ctx, full);
        CGContext::set_interpolation_quality(ctx, CGInterpolationQuality::High);
        CGContext::scale_ctm(ctx, scale, scale);
        // The drawing transform applies /Rotate and moves the crop box to the
        // origin, but never scales up — hence the separate scale above and a
        // target rectangle in points.
        let target = CGRect::new(
            CGPoint::ZERO,
            CGSize::new(width as f64 / scale, height as f64 / scale),
        );
        CGContext::concat_ctm(
            ctx,
            CGPDFPage::drawing_transform(Some(page), CGPDFBox::CropBox, target, 0, true),
        );
        CGContext::clip_to_rect(ctx, CGPDFPage::box_rect(Some(page), CGPDFBox::CropBox));
        CGContext::draw_pdf_page(ctx, Some(page));
        drop(context);
        Some(pixels)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]
mod tests {
    use super::*;

    #[test]
    fn a_letter_page_renders_at_150_dpi() {
        let (width, height, scale) = render_size(612.0, 792.0).unwrap();
        assert_eq!((width, height), (1275, 1650));
        assert!((scale - 150.0 / 72.0).abs() < 1e-9);
    }

    #[test]
    fn a_large_page_is_held_to_the_pixel_budget_without_distortion() {
        // A3 landscape plan.
        let (width, height, _) = render_size(1191.0, 842.0).unwrap();
        assert!((width * height) as f64 <= MAX_RENDER_PIXELS * 1.01);
        let aspect = width as f64 / height as f64;
        assert!((aspect - 1191.0 / 842.0).abs() < 0.01);
    }

    #[test]
    fn a_page_without_area_is_refused() {
        assert_eq!(render_size(0.0, 792.0), None);
        assert_eq!(render_size(f64::NAN, 792.0), None);
    }

    #[test]
    fn png_and_jpeg_files_are_sent_as_they_are() {
        let dir = tempfile::tempdir().unwrap();
        let png = dir.path().join("receipt.PNG");
        let encoded = b"\x89PNG\r\n\x1a\n then the chunks".to_vec();
        std::fs::write(&png, &encoded).unwrap();

        let image = page_image(&png, 1).unwrap();

        assert_eq!(image.mime, "image/png");
        assert_eq!(image.bytes, encoded);
        assert!(image.data_uri().starts_with("data:image/png;base64,iVBOR"));
    }

    #[test]
    fn an_image_has_only_page_one_and_must_be_what_it_claims() {
        let dir = tempfile::tempdir().unwrap();
        let fake = dir.path().join("scan.jpg");
        std::fs::write(&fake, b"not a jpeg").unwrap();

        assert!(matches!(
            page_image(&fake, 2),
            Err(OcrError::PageRender { page: 2, .. })
        ));
        let error = page_image(&fake, 1).unwrap_err().to_string();
        assert!(error.contains("image/jpeg"), "{error}");
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn a_pdf_page_renders_to_a_png_of_the_expected_size() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("blank.pdf");
        write_blank_pdf(&path, 612, 792);

        let image = page_image(&path, 1).unwrap();

        assert_eq!(image.mime, "image/png");
        let decoder = png::Decoder::new(std::io::Cursor::new(image.bytes));
        let reader = decoder.read_info().unwrap();
        assert_eq!((reader.info().width, reader.info().height), (1275, 1650));
        assert!(matches!(
            page_image(&path, 2),
            Err(OcrError::PageRender { page: 2, .. })
        ));
    }

    #[cfg(target_os = "macos")]
    fn write_blank_pdf(path: &Path, width: i64, height: i64) {
        use lopdf::{dictionary, Document, Object, Stream};
        let mut doc = Document::with_version("1.5");
        let pages_id = doc.new_object_id();
        let content_id = doc.add_object(Stream::new(dictionary! {}, Vec::new()));
        let page_id = doc.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "Contents" => content_id,
            "MediaBox" => vec![0.into(), 0.into(), width.into(), height.into()],
        });
        doc.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! {
                "Type" => "Pages",
                "Kids" => vec![page_id.into()],
                "Count" => 1,
            }),
        );
        let catalog_id = doc.add_object(dictionary! {
            "Type" => "Catalog",
            "Pages" => pages_id,
        });
        doc.trailer.set("Root", catalog_id);
        doc.save(path).unwrap();
    }
}
