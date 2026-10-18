//! # Chat pictures: find, fetch, validate, store, lease, serve
//!
//! The `find_image` tool's backend. A picture is found on Wikimedia Commons
//! (or taken from a page the model names), fetched through the web feature's
//! guarded client and site pacer, sniffed and bounded, written under
//! `~/.lattice/chat-images/<id>.<ext>`, recorded in `chat_images` leased to
//! the turn's request, attached to the answer when it persists, and served to
//! the main webview over the private `lattice-image` URL scheme after a hash
//! check. The conversation feature reaches all of it through
//! `lattice_core::application::ports::ChatImagesPort`.
//! Design: `docs/design/2026-10-17-chat-images-and-diagrams.md` §5, §7.3–7.7.

pub mod commons;
pub mod fetch;
pub mod gc;
pub mod prepare;
pub mod protocol;
pub mod repository;
pub mod service;
pub mod store;

#[cfg(test)]
mod tests;

pub use fetch::ImageNetwork;
pub use gc::{ChatImageGc, ImageSweepReport};
pub use service::ChatImageService;

/// The most pixels one picture may declare in its header. A security bound
/// against decode bombs — a few hundred bytes of PNG can declare a
/// 100 000 × 100 000 canvas that the webview would try to allocate.
pub const MAX_IMAGE_PIXELS: u64 = 50_000_000;

/// The width Commons is asked to scale a picture to: sharp in the chat
/// column and the lightbox, a fraction of the original's bytes.
pub const COMMONS_THUMB_WIDTH: u32 = 1280;

/// The formats a stored picture may have, by sniffed MIME type, with the
/// file extension each is stored under. SVG is absent on purpose: it can
/// carry script (§5 T4).
pub const ALLOWED_FORMATS: &[(&str, &str)] = &[
    ("image/jpeg", "jpg"),
    ("image/png", "png"),
    ("image/gif", "gif"),
    ("image/webp", "webp"),
];

/// The extension a sniffed MIME type is stored under, if it is allowed.
pub fn extension_for_mime(mime: &str) -> Option<&'static str> {
    ALLOWED_FORMATS
        .iter()
        .find(|(allowed, _)| *allowed == mime)
        .map(|(_, ext)| *ext)
}

/// Whether `ext` is one a stored picture may have.
pub fn is_allowed_extension(ext: &str) -> bool {
    ALLOWED_FORMATS.iter().any(|(_, allowed)| *allowed == ext)
}
