//! The geometry a cloud model sees with when nobody declared one.

use crate::domain::models::model_ref::CloudProvider;

use super::{VisionGeometry, VisionPreset};

/// The provider's published geometry for `model`, when the model family is
/// one that provider documents as taking pictures. `None` leaves the model
/// unable to see until the user declares a geometry in Settings.
pub fn cloud_vision_default(provider: CloudProvider, model: &str) -> Option<VisionGeometry> {
    let model = model.trim().to_ascii_lowercase();
    let sees = match provider {
        CloudProvider::Openai => ["gpt-4o", "gpt-4.1", "gpt-5", "o3", "o4"]
            .iter()
            .any(|family| model.starts_with(family)),
        CloudProvider::Anthropic => model.starts_with("claude-"),
        CloudProvider::Google => model.starts_with("gemini-"),
    };
    let preset = match provider {
        CloudProvider::Openai => VisionPreset::OpenAi,
        CloudProvider::Anthropic => VisionPreset::Anthropic,
        CloudProvider::Google => VisionPreset::Gemini,
    };
    sees.then(|| preset.geometry())
}
