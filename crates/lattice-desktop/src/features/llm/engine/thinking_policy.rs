//! Models whose output breaks with thinking off, read from the GGUF.

use std::path::Path;

use crate::features::llm::engine::gguf_metadata;
use lattice_core::application::ports::llm_port::ThinkingPolicy;

/// The thinking policy a local model file needs. Laguna XS 2.1 repeats one
/// special token until the output limit when its template's thinking is off,
/// so it thinks on every request.
pub fn thinking_policy_for(model_path: &Path) -> ThinkingPolicy {
    let architecture =
        gguf_metadata::read_model_info(model_path).and_then(|info| info.architecture);
    let file_name = model_path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default();
    if requires_thinking(architecture.as_deref(), file_name) {
        ThinkingPolicy::AlwaysOn
    } else {
        ThinkingPolicy::PerRequest
    }
}

/// The header's architecture decides when there is one; the file name is
/// only for a header this reader cannot parse.
fn requires_thinking(architecture: Option<&str>, file_name: &str) -> bool {
    match architecture {
        Some(architecture) => architecture.eq_ignore_ascii_case("laguna"),
        None => file_name.to_ascii_lowercase().contains("laguna"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn laguna_always_thinks_and_everything_else_follows_the_request() {
        assert!(requires_thinking(Some("laguna"), "model.gguf"));
        assert!(requires_thinking(None, "Laguna-XS-2.1-APEX-I-Compact.gguf"));
        assert!(!requires_thinking(Some("qwen35"), "Qwen3.5-9B-Q4_K_M.gguf"));
        assert!(
            !requires_thinking(Some("qwen35"), "laguna-renamed-qwen.gguf"),
            "the header outranks the file name"
        );
        assert!(!requires_thinking(None, "Qwen3.5-9B-Q4_K_M.gguf"));
    }

    #[test]
    fn an_unreadable_file_falls_back_to_its_name() {
        assert_eq!(
            thinking_policy_for(Path::new("/nonexistent/Laguna-XS-2.1.gguf")),
            ThinkingPolicy::AlwaysOn
        );
        assert_eq!(
            thinking_policy_for(Path::new("/nonexistent/Qwen3.5-9B-Q4_K_M.gguf")),
            ThinkingPolicy::PerRequest
        );
    }
}
