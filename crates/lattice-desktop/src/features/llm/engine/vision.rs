//! Which models may be sent pictures, and with what geometry.
//!
//! Capability is declared, never probed: a remote llama.cpp or Ollama model
//! sees only when the user declared it in Settings → Models; a cloud model
//! sees by its declaration or, without one, by its provider's documented
//! default. Every adapter is built with the answer and refuses pictures
//! without one.

use lattice_core::application::contracts::settings::LLMSettingsDto;
use lattice_core::domain::models::model_ref::ModelRef;
use lattice_core::domain::models::vision::{cloud_vision_default, VisionGeometry};
use lattice_core::shared::error::AppError;

/// The geometry `model` sees with under `settings`, or `None` when it cannot
/// see. Bundled local models are declared by the catalog instead and resolve
/// to `None` here.
pub(crate) fn declared_vision(
    settings: &LLMSettingsDto,
    model: &ModelRef,
) -> Option<VisionGeometry> {
    let declared = settings.vision_declaration(model);
    let geometry = match (model, declared) {
        (ModelRef::Local { .. }, _) => None,
        (_, Some(declaration)) => declaration.enabled.then_some(declaration.geometry),
        (ModelRef::Cloud { provider, model }, None) => cloud_vision_default(*provider, model),
        (ModelRef::Ollama { .. } | ModelRef::LlamaCpp { .. }, None) => None,
    };
    if let Some(geometry) = geometry {
        tracing::info!(
            model = %model,
            kind = kind(geometry),
            max_tokens = geometry.max_tokens(),
            "llm.vision_declared"
        );
    }
    geometry
}

fn kind(geometry: VisionGeometry) -> &'static str {
    match geometry {
        VisionGeometry::Grid { .. } => "grid",
        VisionGeometry::Fixed { .. } => "fixed",
        VisionGeometry::Tiles { .. } => "tiles",
    }
}

/// The refusal of pictures sent to a model nobody declared able to see. The
/// chat falls back to caption lines before it gets here; this is the net.
pub(crate) fn not_set_up_to_see(model: &str) -> AppError {
    AppError::InvalidConfig(format!(
        "{model} is not set up to see images (Settings → Models → Vision)"
    ))
}

/// The refusal of pictures in a message that is not the user's: no provider
/// format carries them anywhere else.
pub(crate) fn pictures_outside_a_user_message(role: &str) -> AppError {
    AppError::InvalidInput(format!(
        "Only a user message can carry pictures; this one is from {role}"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use lattice_core::application::contracts::settings::VisionDeclarationDto;
    use lattice_core::domain::models::model_ref::CloudProvider;
    use lattice_core::domain::models::vision::VisionPreset;

    fn declare(settings: &mut LLMSettingsDto, model: &ModelRef, enabled: bool) {
        settings.vision_declarations.insert(
            model.to_string(),
            VisionDeclarationDto {
                enabled,
                geometry: VisionPreset::Qwen25Vl.geometry(),
            },
        );
    }

    #[test]
    fn a_remote_server_model_sees_only_when_declared_and_enabled() {
        let mut settings = LLMSettingsDto::default();
        let llama = ModelRef::LlamaCpp {
            model: "qwen-vl".into(),
        };
        let ollama = ModelRef::Ollama {
            model: "llava:7b".into(),
        };
        assert_eq!(declared_vision(&settings, &llama), None);
        assert_eq!(declared_vision(&settings, &ollama), None);
        declare(&mut settings, &llama, true);
        declare(&mut settings, &ollama, false);
        assert_eq!(
            declared_vision(&settings, &llama),
            Some(VisionPreset::Qwen25Vl.geometry())
        );
        assert_eq!(declared_vision(&settings, &ollama), None);
    }

    #[test]
    fn a_cloud_model_falls_back_to_its_providers_default_unless_declared() {
        let mut settings = LLMSettingsDto::default();
        let claude = ModelRef::Cloud {
            provider: CloudProvider::Anthropic,
            model: "claude-sonnet-4-5".into(),
        };
        assert_eq!(
            declared_vision(&settings, &claude),
            Some(VisionPreset::Anthropic.geometry())
        );
        declare(&mut settings, &claude, false);
        assert_eq!(declared_vision(&settings, &claude), None);
        let text_only = ModelRef::Cloud {
            provider: CloudProvider::Openai,
            model: "gpt-3.5-turbo".into(),
        };
        assert_eq!(declared_vision(&settings, &text_only), None);
    }
}
