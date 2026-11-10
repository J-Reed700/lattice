//! What OpenAI-style Chat Completions requests share across backends.

use crate::features::llm::engine::vision::user_parts;
use lattice_core::application::ports::llm_port::ContentPart;
use lattice_core::shared::error::Result;
use serde_json::{json, Value};

/// A user message's parts as OpenAI Chat Completions content: each picture an
/// `image_url` data URI, each text a `text` part, in the order given.
pub(crate) fn openai_content_parts(role: &str, parts: &[ContentPart]) -> Result<Vec<Value>> {
    user_parts(role, parts, |part| match part {
        ContentPart::Text { text } => json!({"type":"text","text":text}),
        ContentPart::Image(image) => {
            json!({"type":"image_url","image_url":{"url":image.data_uri()}})
        }
    })
}
