//! Pictures inside a request: the parts of a multimodal message and the
//! helpers every counter and fallback reads them through.

use std::borrow::Cow;
use std::fmt;

use serde::{Deserialize, Serialize};

use super::{CompletionInput, CompletionRequest};

/// One part of a [`CompletionInput::Multimodal`] message.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentPart {
    Text { text: String },
    Image(ImagePart),
}

/// One picture as the model receives it: already fitted to the model's
/// declared geometry. `tokens` is what that geometry charges for
/// `width`×`height`; every counter adds it instead of the base64.
#[derive(Clone, Serialize, Deserialize)]
pub struct ImagePart {
    /// `image/jpeg` or `image/png`.
    pub mime: String,
    /// Standard alphabet, no `data:` prefix.
    pub data_base64: String,
    pub width: u32,
    pub height: u32,
    pub tokens: u32,
    /// What the picture is, for text fallbacks and logs: the file name, or the
    /// `find_image` caption.
    pub alt: String,
}

/// The bytes are left out: a request logged with `{:?}` would otherwise carry
/// every picture it holds.
impl fmt::Debug for ImagePart {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ImagePart")
            .field("mime", &self.mime)
            .field("data_base64_len", &self.data_base64.len())
            .field("width", &self.width)
            .field("height", &self.height)
            .field("tokens", &self.tokens)
            .field("alt", &self.alt)
            .finish()
    }
}

impl ImagePart {
    /// The `data:` URI the OpenAI-compatible formats take.
    pub fn data_uri(&self) -> String {
        format!("data:{};base64,{}", self.mime, self.data_base64)
    }

    /// The line a picture becomes where it cannot be shown.
    pub fn caption(&self) -> String {
        format!("[Picture: {} ({}×{})]", self.alt, self.width, self.height)
    }
}

impl CompletionInput {
    /// The words a tokenizer would read: `Message` content, `Multimodal` text
    /// parts and each image's alt, call names/arguments, results. Empty for
    /// `Native`.
    pub fn text(&self) -> Cow<'_, str> {
        match self {
            Self::Native { .. } => Cow::Borrowed(""),
            Self::Message { content, .. } => Cow::Borrowed(content),
            Self::Multimodal { parts, .. } => Cow::Owned(
                parts
                    .iter()
                    .map(|part| match part {
                        ContentPart::Text { text } => text.as_str(),
                        ContentPart::Image(image) => image.alt.as_str(),
                    })
                    .collect::<Vec<_>>()
                    .join("\n"),
            ),
            Self::ToolCall {
                name, arguments, ..
            } => Cow::Owned(format!("{name}\n{arguments}")),
            Self::ToolResult { output, .. } => Cow::Borrowed(output),
        }
    }

    /// Prompt tokens the pictures in this item cost, by their declared geometry.
    pub fn image_tokens(&self) -> usize {
        self.images().map(|image| image.tokens as usize).sum()
    }

    /// The pictures this item carries.
    pub fn images(&self) -> impl Iterator<Item = &ImagePart> {
        let parts = match self {
            Self::Multimodal { parts, .. } => parts.as_slice(),
            _ => &[],
        };
        parts.iter().filter_map(|part| match part {
            ContentPart::Image(image) => Some(image),
            ContentPart::Text { .. } => None,
        })
    }

    /// `Multimodal` → `Message` whose content is the text parts followed by one
    /// `[Picture: alt (W×H)]` line per image. Other variants unchanged.
    pub fn without_images(&self) -> CompletionInput {
        let Self::Multimodal { role, parts } = self else {
            return self.clone();
        };
        let text = parts.iter().filter_map(|part| match part {
            ContentPart::Text { text } => Some(text.clone()),
            ContentPart::Image(_) => None,
        });
        let captions = self.images().map(ImagePart::caption);
        let lines: Vec<String> = text
            .chain(captions)
            .filter(|line| !line.is_empty())
            .collect();
        Self::Message {
            role: role.clone(),
            content: lines.join("\n"),
        }
    }

    /// Whether this is a message from the user, with or without pictures.
    pub fn is_user(&self) -> bool {
        matches!(
            self,
            Self::Message { role, .. } | Self::Multimodal { role, .. } if role == "user"
        )
    }
}

impl CompletionRequest {
    /// Prompt tokens every picture in the request costs, by declared geometry.
    pub fn image_tokens(&self) -> usize {
        self.input.iter().map(CompletionInput::image_tokens).sum()
    }

    /// Whether any message carries a picture.
    pub fn has_images(&self) -> bool {
        self.input.iter().any(|item| item.images().next().is_some())
    }

    /// How many pictures the request carries.
    pub fn image_count(&self) -> usize {
        self.input.iter().map(|item| item.images().count()).sum()
    }
}
