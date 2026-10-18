//! A stored picture prepared for a model that can see: decoded, turned
//! upright from its EXIF orientation, downscaled to the size the model's
//! declared vision geometry takes (never upscaled), and re-encoded.
//!
//! Privacy: re-encoding writes pixels only. An attached photo's location,
//! camera and every other metadata tag stay in the original file on this
//! machine and never reach a model server, local or remote.
//!
//! Decoding runs on hostile bytes (any file the reader drops in, any picture
//! a page serves), so the decoder works under the security bounds below and
//! refuses before allocating. Design: docs/design/2026-10-17-chat-vision.md
//! §7, §13.

use std::io::Cursor;

use image::codecs::jpeg::JpegEncoder;
use image::codecs::png::PngEncoder;
use image::imageops::FilterType;
use image::{
    DynamicImage, ExtendedColorType, ImageDecoder, ImageEncoder, ImageFormat, ImageReader, Limits,
    Rgb, RgbImage,
};
use lattice_core::domain::models::vision::{FittedImage, VisionGeometry};

use super::fetch::ImageRejected;

/// The widest or tallest picture the decoder accepts. A security bound
/// against decompression bombs: a few hundred bytes can declare a canvas the
/// decoder would otherwise try to allocate. Far above any camera or screen.
pub const MAX_DECODE_SIDE: u32 = 16_384;

/// The most memory one decode may allocate. A security bound: a
/// 16 384 × 16 384 RGBA canvas is 1 GiB, so a header within
/// [`MAX_DECODE_SIDE`] can still ask for more than an app should take; the
/// decoder refuses before allocating. A 50 MP photo (the stored-picture
/// bound) decodes in about 200 MiB.
pub const MAX_DECODE_ALLOC: u64 = 512 * 1024 * 1024;

/// JPEG quality for photographs. High enough that a vision encoder reads
/// what the reader saw; a 2 MP photo comes out around 300–500 KB.
const JPEG_QUALITY: u8 = 85;

/// A picture ready for a request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreparedImage {
    /// `image/jpeg` or `image/png`: what was written.
    pub mime: &'static str,
    pub bytes: Vec<u8>,
    pub width: u32,
    pub height: u32,
    /// What `geometry` charges for `width`×`height`.
    pub tokens: u32,
}

impl PreparedImage {
    /// The extension the prepared cache stores this under.
    pub fn ext(&self) -> &'static str {
        if self.mime == "image/png" {
            "png"
        } else {
            "jpg"
        }
    }
}

/// The decoder limits [`prepare`] runs under.
fn decode_limits() -> Limits {
    let mut limits = Limits::default();
    limits.max_image_width = Some(MAX_DECODE_SIDE);
    limits.max_image_height = Some(MAX_DECODE_SIDE);
    limits.max_alloc = Some(MAX_DECODE_ALLOC);
    limits
}

fn format_for(mime: &str) -> Result<ImageFormat, ImageRejected> {
    match mime {
        "image/jpeg" => Ok(ImageFormat::Jpeg),
        "image/png" => Ok(ImageFormat::Png),
        "image/gif" => Ok(ImageFormat::Gif),
        "image/webp" => Ok(ImageFormat::WebP),
        other => Err(ImageRejected::Format(Some(other.to_string()))),
    }
}

fn decode_error(error: image::ImageError) -> ImageRejected {
    match error {
        image::ImageError::Limits(_) => ImageRejected::TooLarge,
        _ => ImageRejected::Corrupt,
    }
}

/// Decode `source` (already validated by magic bytes as `mime`), apply its
/// EXIF orientation, downscale to `geometry.fit` (or to the largest fit
/// within `max_tokens`), and re-encode: PNG when the source is PNG or GIF
/// (screenshots, diagrams and text, where edges matter), JPEG otherwise.
/// Blocking: run it on the blocking pool.
pub fn prepare(
    source: &[u8],
    mime: &str,
    geometry: VisionGeometry,
    max_tokens: Option<u32>,
) -> Result<PreparedImage, ImageRejected> {
    let format = format_for(mime)?;
    let mut reader = ImageReader::with_format(Cursor::new(source), format);
    reader.limits(decode_limits());
    let mut decoder = reader.into_decoder().map_err(decode_error)?;
    let orientation = decoder
        .orientation()
        .unwrap_or(image::metadata::Orientation::NoTransforms);
    let mut image = DynamicImage::from_decoder(decoder).map_err(decode_error)?;
    image.apply_orientation(orientation);

    let (width, height) = (image.width(), image.height());
    let fitted = match max_tokens {
        Some(max_tokens) => geometry
            .fit_within(width, height, max_tokens)
            .ok_or(ImageRejected::OverBudget { max_tokens })?,
        None => geometry.plan(width, height),
    };
    // Never enlarged, whatever the geometry rounds to: a side shorter than
    // one grid square is sent as it is, and the encoder pads it.
    let fitted = if fitted.width > width || fitted.height > height {
        let (width, height) = (fitted.width.min(width), fitted.height.min(height));
        FittedImage {
            width,
            height,
            tokens: geometry.tokens(width, height),
        }
    } else {
        fitted
    };
    let lossless = matches!(format, ImageFormat::Png | ImageFormat::Gif);
    let mut pixels = flatten_on_white(image);
    if (fitted.width, fitted.height) != (width, height) {
        pixels =
            image::imageops::resize(&pixels, fitted.width, fitted.height, FilterType::Lanczos3);
    }

    let mut bytes = Vec::new();
    let encoded = if lossless {
        PngEncoder::new(&mut bytes).write_image(
            pixels.as_raw(),
            fitted.width,
            fitted.height,
            ExtendedColorType::Rgb8,
        )
    } else {
        JpegEncoder::new_with_quality(&mut bytes, JPEG_QUALITY).write_image(
            pixels.as_raw(),
            fitted.width,
            fitted.height,
            ExtendedColorType::Rgb8,
        )
    };
    encoded.map_err(|_| ImageRejected::Corrupt)?;
    Ok(PreparedImage {
        mime: if lossless { "image/png" } else { "image/jpeg" },
        bytes,
        width: fitted.width,
        height: fitted.height,
        tokens: fitted.tokens,
    })
}

/// RGB pixels, with any transparency composited onto white: the encoders
/// write RGB, and a transparent screenshot or logo read as black would hide
/// dark text and lines.
fn flatten_on_white(image: DynamicImage) -> RgbImage {
    if !image.color().has_alpha() {
        return image.into_rgb8();
    }
    let rgba = image.into_rgba8();
    let mut out = RgbImage::new(rgba.width(), rgba.height());
    for (source, target) in rgba.pixels().zip(out.pixels_mut()) {
        let [r, g, b, a] = source.0;
        let alpha = u16::from(a);
        let over_white = |channel: u8| {
            let blended = (u16::from(channel) * alpha + 255 * (255 - alpha) + 127) / 255;
            u8::try_from(blended).unwrap_or(u8::MAX)
        };
        *target = Rgb([over_white(r), over_white(g), over_white(b)]);
    }
    out
}

#[cfg(test)]
mod tests;
