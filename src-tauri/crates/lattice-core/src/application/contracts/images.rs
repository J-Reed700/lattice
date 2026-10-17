//! Pictures an assistant answer shows inline: the `find_image` tool's input
//! and output, and the record the window needs to show a stored picture.
//!
//! Design: docs/design/2026-10-17-chat-images-and-diagrams.md §7.2.

use serde::{Deserialize, Serialize};

/// How many pictures one `find_image` call stores when the model names no
/// number.
pub const DEFAULT_IMAGE_LIMIT: usize = 3;

/// The most pictures one `find_image` call stores. Each one is a download,
/// a file and a row held for the life of the message.
pub const MAX_IMAGE_LIMIT: usize = 6;

fn default_image_limit() -> usize {
    DEFAULT_IMAGE_LIMIT
}

/// Input for the `find_image` tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FindImageInput {
    /// What the picture should show.
    pub query: String,
    /// Take the picture from this page (its `og:image` or lead image)
    /// instead of searching Wikimedia Commons.
    #[serde(default)]
    pub from_page_url: Option<String>,
    /// How many pictures to store; clamp 1..=6, default 3.
    #[serde(default = "default_image_limit")]
    pub limit: usize,
}

impl FindImageInput {
    /// `limit` clamped to `1..=MAX_IMAGE_LIMIT`.
    pub fn clamped_limit(&self) -> usize {
        self.limit.clamp(1, MAX_IMAGE_LIMIT)
    }
}

/// One stored picture as the model sees it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FoundImage {
    /// The ref the answer writes, e.g. `img-1`.
    pub r#ref: String,
    pub caption: String,
    pub source_title: Option<String>,
    pub source_url: String,
    pub license: Option<String>,
    pub width: u32,
    pub height: u32,
    /// Ready-to-paste markdown: `![caption](img-1)`.
    pub markdown: String,
}

/// The whole note when nothing was stored: a success, so the model answers
/// without a picture instead of retrying. Why nothing was stored is logged.
pub const NO_PICTURE_NOTE: &str = "No suitable picture found. Answer without one.";

/// What `find_image` returns to the model.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FindImageOutput {
    pub images: Vec<FoundImage>,
    /// Why fewer pictures than asked for were stored, when that happened.
    pub note: Option<String>,
}

/// Where a stored picture came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum ImageOrigin {
    WikimediaCommons,
    Page,
}

impl ImageOrigin {
    /// The value stored in `chat_images.origin`.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::WikimediaCommons => "wikimedia_commons",
            Self::Page => "page",
        }
    }

    /// Parse a stored `chat_images.origin` value.
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "wikimedia_commons" => Some(Self::WikimediaCommons),
            "page" => Some(Self::Page),
            _ => None,
        }
    }
}

/// What the window needs to show a stored picture. Travels in the stream
/// event and in `metadata.images`; the bytes travel over `lattice-image`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ChatImageDto {
    /// 32 lowercase hex, the file stem.
    pub id: String,
    /// `img-1` as written in the answer.
    pub r#ref: String,
    /// `<id>.jpg` — what the frontend hands `convertFileSrc`.
    pub file_name: String,
    pub mime: String,
    pub width: u32,
    pub height: u32,
    pub byte_size: u64,
    pub caption: String,
    pub source_url: String,
    pub source_title: Option<String>,
    pub origin: ImageOrigin,
    pub license: Option<String>,
    pub attribution: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn find_image_input_defaults_limit_to_three() {
        let input: FindImageInput = serde_json::from_str(r#"{"query":"heron"}"#).unwrap();
        assert_eq!(input.limit, DEFAULT_IMAGE_LIMIT);
        assert_eq!(input.from_page_url, None);
        assert_eq!(input.clamped_limit(), 3);
    }

    #[test]
    fn find_image_input_clamps_limit_to_one_through_six() {
        let zero: FindImageInput = serde_json::from_str(r#"{"query":"heron","limit":0}"#).unwrap();
        assert_eq!(zero.clamped_limit(), 1);
        let many: FindImageInput = serde_json::from_str(r#"{"query":"heron","limit":40}"#).unwrap();
        assert_eq!(many.clamped_limit(), MAX_IMAGE_LIMIT);
        let four: FindImageInput = serde_json::from_str(r#"{"query":"heron","limit":4}"#).unwrap();
        assert_eq!(four.clamped_limit(), 4);
    }

    #[test]
    fn chat_image_dto_serializes_camel_case_with_snake_origin() {
        let dto = ChatImageDto {
            id: "0".repeat(32),
            r#ref: "img-1".into(),
            file_name: format!("{}.jpg", "0".repeat(32)),
            mime: "image/jpeg".into(),
            width: 1280,
            height: 720,
            byte_size: 4096,
            caption: "A heron".into(),
            source_url: "https://commons.wikimedia.org/wiki/File:Heron.jpg".into(),
            source_title: Some("File:Heron.jpg".into()),
            origin: ImageOrigin::WikimediaCommons,
            license: Some("CC BY-SA 4.0".into()),
            attribution: None,
        };
        let value = serde_json::to_value(&dto).unwrap();
        assert_eq!(value["ref"], "img-1");
        assert_eq!(value["fileName"], dto.file_name);
        assert_eq!(value["byteSize"], 4096);
        assert_eq!(value["sourceUrl"], dto.source_url);
        assert_eq!(value["origin"], "wikimedia_commons");
        let back: ChatImageDto = serde_json::from_value(value).unwrap();
        assert_eq!(back, dto);
    }

    #[test]
    fn image_origin_round_trips_its_stored_value() {
        for origin in [ImageOrigin::WikimediaCommons, ImageOrigin::Page] {
            assert_eq!(ImageOrigin::parse(origin.as_str()), Some(origin));
        }
        assert_eq!(ImageOrigin::parse("svg"), None);
    }
}
