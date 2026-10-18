use std::io::Cursor;

use image::codecs::gif::GifEncoder;
use image::codecs::jpeg::JpegEncoder;
use image::codecs::png::PngEncoder;
use image::codecs::webp::WebPEncoder;
use image::{
    DynamicImage, ExtendedColorType, ImageDecoder, ImageEncoder, ImageFormat, ImageReader, Rgb,
    RgbImage, Rgba, RgbaImage,
};
use lattice_core::domain::models::vision::VisionGeometry;

use super::*;

const GRID_28: VisionGeometry = VisionGeometry::CONSERVATIVE_DEFAULT;

fn photo(width: u32, height: u32) -> RgbImage {
    RgbImage::from_fn(width, height, |x, y| {
        Rgb([(x % 256) as u8, (y % 256) as u8, ((x + y) % 256) as u8])
    })
}

fn jpeg(width: u32, height: u32, exif: Option<Vec<u8>>) -> Vec<u8> {
    let pixels = photo(width, height);
    let mut bytes = Vec::new();
    let mut encoder = JpegEncoder::new_with_quality(&mut bytes, 90);
    if let Some(exif) = exif {
        encoder.set_exif_metadata(exif).unwrap();
    }
    encoder
        .write_image(pixels.as_raw(), width, height, ExtendedColorType::Rgb8)
        .unwrap();
    bytes
}

/// A 2×1 PNG: an opaque black pixel and a fully transparent one.
fn transparent_png() -> Vec<u8> {
    let pixels = RgbaImage::from_fn(2, 1, |x, _| {
        if x == 0 {
            Rgba([0, 0, 0, 255])
        } else {
            Rgba([0, 0, 0, 0])
        }
    });
    let mut bytes = Vec::new();
    PngEncoder::new(&mut bytes)
        .write_image(pixels.as_raw(), 2, 1, ExtendedColorType::Rgba8)
        .unwrap();
    bytes
}

fn decode(bytes: &[u8]) -> DynamicImage {
    image::load_from_memory(bytes).unwrap()
}

/// A little-endian TIFF block holding one tag: Orientation = `value`.
fn exif_orientation(value: u8) -> Vec<u8> {
    vec![
        0x49, 0x49, 0x2A, 0x00, 0x08, 0x00, 0x00, 0x00, // header, IFD0 at 8
        0x01, 0x00, // one entry
        0x12, 0x01, 0x03, 0x00, 0x01, 0x00, 0x00, 0x00, // 0x0112 SHORT ×1
        value, 0x00, 0x00, 0x00, // the value, padded
        0x00, 0x00, 0x00, 0x00, // no next IFD
    ]
}

fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for byte in bytes {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            crc = if crc & 1 == 1 {
                (crc >> 1) ^ 0xEDB8_8320
            } else {
                crc >> 1
            };
        }
    }
    !crc
}

/// A PNG whose header declares `side`×`side` and carries no pixels: what a
/// decompression bomb looks like before it is decoded.
fn png_declaring(side: u32) -> Vec<u8> {
    let mut ihdr = b"IHDR".to_vec();
    ihdr.extend_from_slice(&side.to_be_bytes());
    ihdr.extend_from_slice(&side.to_be_bytes());
    ihdr.extend_from_slice(&[8, 6, 0, 0, 0]); // 8-bit RGBA
    let mut bytes = b"\x89PNG\r\n\x1a\n".to_vec();
    bytes.extend_from_slice(&13u32.to_be_bytes());
    bytes.extend_from_slice(&ihdr);
    bytes.extend_from_slice(&crc32(&ihdr).to_be_bytes());
    bytes
}

#[test]
fn a_photo_is_fitted_to_the_geometry_and_written_as_jpeg() {
    let source = jpeg(2000, 1500, None);
    let prepared = prepare(&source, "image/jpeg", GRID_28, None).unwrap();
    let plan = GRID_28.plan(2000, 1500);
    assert_eq!((prepared.width, prepared.height), (plan.width, plan.height));
    assert_eq!(prepared.tokens, plan.tokens);
    assert_eq!(prepared.mime, "image/jpeg");
    assert_eq!(prepared.ext(), "jpg");
    let decoded = decode(&prepared.bytes);
    assert_eq!(
        (decoded.width(), decoded.height()),
        (plan.width, plan.height)
    );
    assert!(prepared.width < 2000 && prepared.height < 1500);
}

#[test]
fn a_small_picture_is_never_upscaled() {
    let source = jpeg(400, 300, None);
    let prepared = prepare(&source, "image/jpeg", GRID_28, None).unwrap();
    assert!(prepared.width <= 400 && prepared.height <= 300);
    let fixed = VisionGeometry::Fixed {
        side: 896,
        tokens: 256,
    };
    let prepared = prepare(&source, "image/jpeg", fixed, None).unwrap();
    assert_eq!((prepared.width, prepared.height), (400, 300));
    assert_eq!(prepared.tokens, 256);
}

#[test]
fn transparency_is_flattened_onto_white_and_png_stays_png() {
    let prepared = prepare(&transparent_png(), "image/png", GRID_28, None).unwrap();
    assert_eq!(prepared.mime, "image/png");
    let decoded = decode(&prepared.bytes).into_rgb8();
    assert_eq!(decoded.get_pixel(0, 0), &Rgb([0, 0, 0]));
    assert_eq!(decoded.get_pixel(1, 0), &Rgb([255, 255, 255]));
}

#[test]
fn a_gif_is_written_as_png_and_a_webp_as_jpeg() {
    let pixels = RgbaImage::from_pixel(64, 48, Rgba([10, 120, 200, 255]));
    let mut gif = Vec::new();
    GifEncoder::new(&mut gif)
        .encode(pixels.as_raw(), 64, 48, ExtendedColorType::Rgba8)
        .unwrap();
    let prepared = prepare(&gif, "image/gif", GRID_28, None).unwrap();
    assert_eq!(prepared.mime, "image/png");
    assert_eq!(
        image::guess_format(&prepared.bytes).unwrap(),
        ImageFormat::Png
    );

    let mut webp = Vec::new();
    WebPEncoder::new_lossless(&mut webp)
        .encode(pixels.as_raw(), 64, 48, ExtendedColorType::Rgba8)
        .unwrap();
    let prepared = prepare(&webp, "image/webp", GRID_28, None).unwrap();
    assert_eq!(prepared.mime, "image/jpeg");
    assert_eq!(
        image::guess_format(&prepared.bytes).unwrap(),
        ImageFormat::Jpeg
    );
}

#[test]
fn exif_orientation_six_turns_the_picture_upright() {
    let source = jpeg(64, 32, Some(exif_orientation(6)));
    let prepared = prepare(&source, "image/jpeg", GRID_28, None).unwrap();
    assert!(
        prepared.height > prepared.width,
        "a landscape file marked 'rotate 90' is a portrait picture: {}×{}",
        prepared.width,
        prepared.height
    );
}

#[test]
fn re_encoding_drops_the_exif_block() {
    let source = jpeg(64, 32, Some(exif_orientation(6)));
    let prepared = prepare(&source, "image/jpeg", GRID_28, None).unwrap();
    let mut decoder = ImageReader::with_format(Cursor::new(&prepared.bytes), ImageFormat::Jpeg)
        .into_decoder()
        .unwrap();
    assert_eq!(decoder.exif_metadata().unwrap(), None);
}

#[test]
fn a_header_past_the_decode_bounds_is_refused_before_allocating() {
    let bomb = png_declaring(20_000);
    assert_eq!(
        prepare(&bomb, "image/png", GRID_28, None),
        Err(ImageRejected::TooLarge)
    );
}

#[test]
fn a_budget_shrinks_the_fit_and_a_fixed_encoder_below_it_is_refused() {
    let source = jpeg(2000, 1500, None);
    let full = prepare(&source, "image/jpeg", GRID_28, None).unwrap();
    let budget = full.tokens / 3;
    let shrunk = prepare(&source, "image/jpeg", GRID_28, Some(budget)).unwrap();
    assert!(shrunk.tokens <= budget);
    assert!(shrunk.width < full.width);

    let fixed = VisionGeometry::Fixed {
        side: 896,
        tokens: 256,
    };
    assert_eq!(
        prepare(&source, "image/jpeg", fixed, Some(100)),
        Err(ImageRejected::OverBudget { max_tokens: 100 })
    );
}

#[test]
fn bytes_that_do_not_decode_are_corrupt() {
    let mut source = jpeg(64, 32, None);
    source.truncate(40);
    assert_eq!(
        prepare(&source, "image/jpeg", GRID_28, None),
        Err(ImageRejected::Corrupt)
    );
}
